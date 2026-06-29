//! softcursor — software mouse cursor overlay for Kali Linux on VMware ESXi 6.x
//!
//! VMware ESXi 6.x SVGA drivers fail to render the hardware cursor on modern
//! Kali guests. This daemon creates a borderless, always-on-top X11 window that
//! draws a classic arrow pointer at the real cursor position. The window is fully
//! input-transparent (clicks/keys pass straight through to whatever is underneath).
//!
//! Built WITH CSK by NoHatHacker.com

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
    XCreatePixmap, XFreePixmap,
    XCreateWindow, XMapWindow, XMoveWindow,
    XFlush, XQueryPointer,
    XSetForeground, XFillPolygon, XFillRectangle,
    XClearWindow,
    CoordModeOrigin, Complex,
    CWBackPixel, CWBorderPixel, CWOverrideRedirect, CWEventMask,
    InputOutput, StructureNotifyMask,
};
use x11::xfixes::{XFixesSetWindowShapeRegion, XFixesQueryExtension, XFixesCreateRegion};

// XShape constants (from X11/extensions/shape.h)
const SHAPE_BOUNDING: c_int = 0;
const SHAPE_SET: c_int      = 0;

// XShapeCombineMask is in libXext — linked via build.rs
#[link(name = "Xext")]
extern "C" {
    fn XShapeCombineMask(
        display: *mut Display,
        dest: Window,
        dest_kind: c_int,
        x_off: c_int,
        y_off: c_int,
        src: xlib::Pixmap,
        op: c_int,
    );
}

// ── Cursor geometry ────────────────────────────────────────────────────────────
// Classic arrow pointer, hot-spot at (0,0).
// Coordinates are in a 18-unit grid; scaled by SCALE at runtime.

const SCALE: f32 = 18.0; // base size in pixels — increase for HiDPI

/// Outer arrow fill (black).
const ARROW_FILL: &[(f32, f32)] = &[
    (0.0,  0.0),
    (0.0,  14.0),
    (3.5,  10.5),
    (6.5,  17.0),
    (8.5,  16.2),
    (5.5,  9.5),
    (10.0, 9.5),
];

/// Inner highlight (white), slightly inset for visibility on dark backgrounds.
const ARROW_HIGHLIGHT: &[(f32, f32)] = &[
    (1.0,  1.5),
    (1.0,  11.5),
    (3.8,  9.0),
    (6.2,  14.8),
    (7.2,  14.4),
    (4.5,  8.5),
    (8.0,  8.5),
];

fn win_w() -> u32 { (SCALE * 10.0 / 18.0).ceil() as u32 + 2 }
fn win_h() -> u32 { (SCALE * 17.0 / 18.0).ceil() as u32 + 2 }

// ── Main ───────────────────────────────────────────────────────────────────────

fn main() {
    unsafe { run() }
}

unsafe fn run() {
    // Try $DISPLAY first, then fall back to :0 / :1 / :2 so the service
    // survives even when DISPLAY isn't inherited (e.g. systemd user session).
    let dpy: *mut Display = {
        let mut d = xlib::XOpenDisplay(ptr::null());
        if d.is_null() {
            for display in &[b":0\0".as_ptr(), b":1\0".as_ptr(), b":2\0".as_ptr()] {
                d = xlib::XOpenDisplay(*display as *const i8);
                if !d.is_null() { break; }
            }
        }
        d
    };
    if dpy.is_null() {
        eprintln!("softcursor: cannot open X display — tried $DISPLAY, :0, :1, :2");
        std::process::exit(1);
    }

    let screen = XDefaultScreen(dpy);
    let root   = XDefaultRootWindow(dpy);
    let depth  = XDefaultDepth(dpy, screen);
    let black  = XBlackPixel(dpy, screen);
    let white  = XWhitePixel(dpy, screen);

    // ── Create overlay window ──────────────────────────────────────────────────
    let mut swa: XSetWindowAttributes = std::mem::zeroed();
    swa.background_pixel  = black; // clipped by bounding shape — not visible
    swa.border_pixel      = 0;
    swa.override_redirect = xlib::True;  // bypass WM — no decorations, always on top
    swa.event_mask        = StructureNotifyMask;

    let w = win_w();
    let h = win_h();

    let win: Window = XCreateWindow(
        dpy, root,
        0, 0, w, h,
        0,
        depth,
        InputOutput as c_uint,
        xlib::CopyFromParent as *mut xlib::Visual,
        (CWBackPixel | CWBorderPixel | CWOverrideRedirect | CWEventMask) as c_ulong,
        &mut swa,
    );

    // ── Clip window to arrow shape (removes the black rectangle) ──────────────
    // Build a 1-bit pixmap matching the arrow polygon and apply it as
    // ShapeBounding. Pixels outside the polygon simply don't exist in the
    // window — no compositor needed, works on bare X11.
    apply_bounding_shape(dpy, win, w, h);

    // ── Make window click-through via XFixes input shape ──────────────────────
    // Empty input region → all pointer and keyboard events pass through.
    let mut fixes_ev = 0i32;
    let mut fixes_er = 0i32;
    if XFixesQueryExtension(dpy, &mut fixes_ev, &mut fixes_er) != 0 {
        let empty = XFixesCreateRegion(dpy, ptr::null_mut(), 0);
        // ShapeInput = 2
        XFixesSetWindowShapeRegion(dpy, win, 2, 0, 0, empty);
    }

    // ── Drawing contexts ───────────────────────────────────────────────────────
    let mut gcv: XGCValues = std::mem::zeroed();
    let gc_black: GC = XCreateGC(dpy, win as Drawable, 0, &mut gcv);
    let gc_white: GC = XCreateGC(dpy, win as Drawable, 0, &mut gcv);
    XSetForeground(dpy, gc_black, black);
    XSetForeground(dpy, gc_white, white);

    XMapWindow(dpy, win);
    XFlush(dpy);

    // ── Poll loop @ ~60 fps ────────────────────────────────────────────────────
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
            draw_arrow(dpy, win, gc_black, gc_white);
            XFlush(dpy);
        }

        // Drain event queue to prevent buildup.
        let mut ev: XEvent = std::mem::zeroed();
        while xlib::XPending(dpy) > 0 {
            xlib::XNextEvent(dpy, &mut ev);
        }

        thread::sleep(Duration::from_millis(16));
    }
}

/// Clip the window's bounding shape to the arrow polygon using a 1-bit mask
/// pixmap.  Pixels outside the arrow are excluded from the window entirely —
/// the black background rectangle becomes invisible without needing a compositor.
unsafe fn apply_bounding_shape(dpy: *mut Display, win: Window, w: u32, h: u32) {
    let mask: xlib::Pixmap = XCreatePixmap(dpy, win, w, h, 1);
    let mut gcv: XGCValues = std::mem::zeroed();
    let gc: GC = XCreateGC(dpy, mask as Drawable, 0, &mut gcv);

    // Clear entire mask to 0 (excluded).
    XSetForeground(dpy, gc, 0);
    XFillRectangle(dpy, mask as Drawable, gc, 0, 0, w, h);

    // Fill arrow polygon in 1 (included).
    XSetForeground(dpy, gc, 1);
    let pts = polygon(ARROW_FILL);
    XFillPolygon(
        dpy, mask as Drawable, gc,
        pts.as_ptr() as *mut xlib::XPoint,
        pts.len() as c_int,
        Complex, CoordModeOrigin,
    );

    XShapeCombineMask(dpy, win, SHAPE_BOUNDING, 0, 0, mask, SHAPE_SET);

    XFreeGC(dpy, gc);
    XFreePixmap(dpy, mask);
}

unsafe fn draw_arrow(dpy: *mut Display, win: Window, gc_black: GC, gc_white: GC) {
    XClearWindow(dpy, win);

    let fill_pts = polygon(ARROW_FILL);
    XFillPolygon(dpy, win as Drawable, gc_black,
        fill_pts.as_ptr() as *mut xlib::XPoint,
        fill_pts.len() as c_int,
        Complex, CoordModeOrigin,
    );

    let hi_pts = polygon(ARROW_HIGHLIGHT);
    XFillPolygon(dpy, win as Drawable, gc_white,
        hi_pts.as_ptr() as *mut xlib::XPoint,
        hi_pts.len() as c_int,
        Complex, CoordModeOrigin,
    );
}

/// Scale unit arrow coordinates to pixel XPoints.
fn polygon(pts: &[(f32, f32)]) -> Vec<xlib::XPoint> {
    pts.iter().map(|(x, y)| xlib::XPoint {
        x: (x * SCALE / 18.0).round() as i16,
        y: (y * SCALE / 18.0).round() as i16,
    }).collect()
}
