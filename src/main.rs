//! softcursor — software mouse cursor overlay for Kali Linux on VMware ESXi 6.x
//!
//! VMware ESXi 6.x SVGA drivers fail to render the hardware cursor on modern
//! Kali guests. This daemon creates a borderless, always-on-top X11 window that
//! draws a classic arrow pointer at the real cursor position. The window is fully
//! input-transparent (clicks/keys pass straight through to whatever is underneath).
//!
//! Built WITH CSK by NoHatHacker.com

use std::ffi::CString;
use std::os::raw::{c_int, c_uint, c_ulong};
use std::ptr;
use std::thread;
use std::time::Duration;

use x11::xlib::{
    self, Display, Drawable, GC, Window, XEvent,
    XGCValues, XSetWindowAttributes,
    XDefaultRootWindow, XDefaultScreen, XDefaultDepth,
    XBlackPixel, XWhitePixel,
    XCreateGC, XFreeGC,
    XCreateWindow, XMapWindow, XMoveWindow,
    XFlush, XQueryPointer, XSync,
    XSetForeground, XDrawLines, XFillPolygon,
    XClearWindow,
    CoordModeOrigin, Complex,
    CWBackPixel, CWBorderPixel, CWColormap, CWOverrideRedirect, CWEventMask,
    InputOutput, StructureNotifyMask,
};
use x11::xfixes::{XFixesSetWindowShapeRegion, XFixesQueryExtension};

// ── Cursor geometry ────────────────────────────────────────────────────────────
// Classic arrow pointer, hot-spot at (0,0), scaled to SCALE pixels.
// Coordinates are unit fractions of SIZE; we multiply by SCALE at runtime.

const SCALE: i16 = 18; // pixels; bump for HiDPI

/// Outer arrow polygon (fill in black).
const ARROW_FILL: &[(f32, f32)] = &[
    (0.0,  0.0),
    (0.0,  14.0),
    (3.5,  10.5),
    (6.5,  17.0),
    (8.5,  16.2),
    (5.5,  9.5),
    (10.0, 9.5),
    (0.0,  0.0),
];

/// Inner highlight polygon (fill in white, slightly inset, gives depth).
const ARROW_HIGHLIGHT: &[(f32, f32)] = &[
    (1.0,  1.5),
    (1.0,  11.5),
    (3.8,  9.0),
    (6.2,  14.8),
    (7.2,  14.4),
    (4.5,  8.5),
    (8.0,  8.5),
    (1.0,  1.5),
];

// Total bounding box of the cursor window.
const WIN_W: u32 = (SCALE as u32) + 4;
const WIN_H: u32 = (SCALE as u32) * 2;

// ── Main ───────────────────────────────────────────────────────────────────────

fn main() {
    unsafe { run() }
}

unsafe fn run() {
    let dpy: *mut Display = xlib::XOpenDisplay(ptr::null());
    if dpy.is_null() {
        eprintln!("softcursor: cannot open X display (is DISPLAY set?)");
        std::process::exit(1);
    }

    let screen = XDefaultScreen(dpy);
    let root   = XDefaultRootWindow(dpy);
    let depth  = XDefaultDepth(dpy, screen);
    let black  = XBlackPixel(dpy, screen);
    let white  = XWhitePixel(dpy, screen);

    // ── Create overlay window ──────────────────────────────────────────────────
    let mut swa: XSetWindowAttributes = std::mem::zeroed();
    swa.background_pixel  = 0;           // transparent bg (needs compositor) or black
    swa.border_pixel      = 0;
    swa.override_redirect = xlib::True;  // bypass WM: no decorations, always on top
    swa.event_mask        = StructureNotifyMask;

    let win: Window = XCreateWindow(
        dpy, root,
        0, 0,                  // initial position
        WIN_W, WIN_H,          // size
        0,                     // border width
        depth,
        InputOutput as c_uint,
        xlib::CopyFromParent as *mut xlib::Visual,
        (CWBackPixel | CWBorderPixel | CWOverrideRedirect | CWEventMask) as c_ulong,
        &mut swa,
    );

    // ── Make window click-through via XFixes input shape ──────────────────────
    // An empty input shape means ALL pointer/keyboard events pass through the
    // window to whatever is underneath — the overlay is visually present but
    // completely transparent to input.
    let mut fixes_ev = 0i32;
    let mut fixes_er = 0i32;
    if XFixesQueryExtension(dpy, &mut fixes_ev, &mut fixes_er) != 0 {
        // ShapeInput = 2 (XShape.h)
        XFixesSetWindowShapeRegion(dpy, win, 2, 0, 0, x11::xfixes::XFixesCreateRegion(dpy, ptr::null_mut(), 0));
    }

    // ── Create drawing contexts ────────────────────────────────────────────────
    let mut gcv: XGCValues = std::mem::zeroed();
    let gc_black: GC = XCreateGC(dpy, win as Drawable, 0, &mut gcv);
    let gc_white: GC = XCreateGC(dpy, win as Drawable, 0, &mut gcv);
    XSetForeground(dpy, gc_black, black);
    XSetForeground(dpy, gc_white, white);

    XMapWindow(dpy, win);
    XFlush(dpy);

    // ── Poll loop ─────────────────────────────────────────────────────────────
    // XQueryPointer is cheap (~1µs) at 60 fps — no event queue needed.
    let mut last_x: c_int = -9999;
    let mut last_y: c_int = -9999;

    loop {
        let mut root_ret: Window = 0;
        let mut child_ret: Window = 0;
        let mut root_x: c_int = 0;
        let mut root_y: c_int = 0;
        let mut win_x: c_int  = 0;
        let mut win_y: c_int  = 0;
        let mut mask: c_uint  = 0;

        XQueryPointer(dpy, root,
            &mut root_ret, &mut child_ret,
            &mut root_x,  &mut root_y,
            &mut win_x,   &mut win_y,
            &mut mask,
        );

        if root_x != last_x || root_y != last_y {
            last_x = root_x;
            last_y = root_y;
            XMoveWindow(dpy, win, root_x, root_y);
            draw_cursor(dpy, win, gc_black, gc_white);
            XFlush(dpy);
        }

        // Drain any queued events (e.g. ConfigureNotify) to avoid queue buildup.
        let mut _ev: XEvent = std::mem::zeroed();
        while xlib::XPending(dpy) > 0 {
            xlib::XNextEvent(dpy, &mut _ev);
        }

        thread::sleep(Duration::from_millis(16)); // ~60 fps
    }
}

unsafe fn draw_cursor(dpy: *mut Display, win: Window, gc_black: GC, gc_white: GC) {
    XClearWindow(dpy, win);

    // Fill black arrow
    let fill_pts = polygon(ARROW_FILL);
    XFillPolygon(dpy, win as Drawable, gc_black,
        fill_pts.as_ptr() as *mut xlib::XPoint,
        fill_pts.len() as c_int,
        Complex, CoordModeOrigin,
    );

    // White highlight on top for readability on any background
    let hi_pts = polygon(ARROW_HIGHLIGHT);
    XFillPolygon(dpy, win as Drawable, gc_white,
        hi_pts.as_ptr() as *mut xlib::XPoint,
        hi_pts.len() as c_int,
        Complex, CoordModeOrigin,
    );
}

/// Convert float unit coordinates → scaled XPoint array.
fn polygon(pts: &[(f32, f32)]) -> Vec<xlib::XPoint> {
    pts.iter().map(|(x, y)| xlib::XPoint {
        x: (*x * SCALE as f32 / 18.0) as i16,
        y: (*y * SCALE as f32 / 18.0) as i16,
    }).collect()
}
