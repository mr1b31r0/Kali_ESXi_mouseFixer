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
    XGCValues, XSetWindowAttributes, XVisualInfo,
    XDefaultRootWindow, XDefaultScreen, XDefaultDepth,
    XBlackPixel, XWhitePixel,
    XCreateGC, XFreeGC,
    XCreateWindow, XMapWindow, XMoveWindow,
    XFlush, XQueryPointer,
    XSetForeground, XFillPolygon, XFillRectangle,
    XClearWindow, XMatchVisualInfo, XCreateColormap,
    CoordModeOrigin, Complex,
    CWBackPixel, CWBorderPixel, CWColormap, CWOverrideRedirect, CWEventMask,
    InputOutput, StructureNotifyMask, TrueColor, AllocNone,
};
use x11::xfixes::{XFixesSetWindowShapeRegion, XFixesQueryExtension, XFixesCreateRegion};

// ── Cursor geometry ────────────────────────────────────────────────────────────
// Classic arrow pointer, hot-spot at (0,0).
// Coordinates are in an 18-unit grid; scaled by SCALE at runtime.

const SCALE: f32 = 18.0; // base size in pixels — bump for HiDPI

/// Outer arrow fill — drawn in opaque black (or foreground colour).
const ARROW_FILL: &[(f32, f32)] = &[
    (0.0,  0.0),
    (0.0,  14.0),
    (3.5,  10.5),
    (6.5,  17.0),
    (8.5,  16.2),
    (5.5,  9.5),
    (10.0, 9.5),
];

/// Inner highlight — drawn in opaque white, gives visibility on dark backgrounds.
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

// ARGB pixel values for a 32-bit visual.
const ARGB_TRANSPARENT: c_ulong = 0x0000_0000;
const ARGB_BLACK:       c_ulong = 0xFF00_0000;
const ARGB_WHITE:       c_ulong = 0xFFFF_FFFF;

// ── Main ───────────────────────────────────────────────────────────────────────

fn main() {
    unsafe { run() }
}

unsafe fn run() {
    // Try $DISPLAY, then :0 / :1 / :2 so the service survives when DISPLAY
    // isn't inherited from the graphical session (e.g. systemd user unit).
    let dpy: *mut Display = {
        let mut d = xlib::XOpenDisplay(ptr::null());
        if d.is_null() {
            for name in &[b":0\0".as_ptr(), b":1\0".as_ptr(), b":2\0".as_ptr()] {
                d = xlib::XOpenDisplay(*name as *const i8);
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
    let black  = XBlackPixel(dpy, screen);
    let white  = XWhitePixel(dpy, screen);

    // ── Prefer ARGB visual (real compositor transparency) ──────────────────────
    // If the compositor (Cinnamon / GNOME / Picom) is running, a 32-bit ARGB
    // visual gives us a fully transparent background — no black rectangle at all.
    // Falls back to the default visual with pseudo-transparency on bare X11.
    let mut vinfo: XVisualInfo = std::mem::zeroed();
    let has_argb = XMatchVisualInfo(dpy, screen, 32, TrueColor, &mut vinfo) != 0;

    let (visual, depth, colormap, bg_pixel) = if has_argb {
        let cmap = XCreateColormap(dpy, root, vinfo.visual, AllocNone);
        (vinfo.visual, 32, cmap, ARGB_TRANSPARENT)
    } else {
        // No compositor — fall back to default visual.
        // XClearWindow will fill with pseudo-transparent root background.
        let depth = XDefaultDepth(dpy, screen);
        let cmap  = xlib::XDefaultColormap(dpy, screen);
        (xlib::XDefaultVisual(dpy, screen), depth, cmap, 0u64)
    };

    // ── Create overlay window ──────────────────────────────────────────────────
    let mut swa: XSetWindowAttributes = std::mem::zeroed();
    swa.background_pixel  = bg_pixel;
    swa.border_pixel      = 0;
    swa.colormap          = colormap;
    swa.override_redirect = xlib::True;  // bypass WM — always on top
    swa.event_mask        = StructureNotifyMask;

    let w = win_w();
    let h = win_h();

    let win: Window = XCreateWindow(
        dpy, root,
        0, 0, w, h,
        0,
        depth,
        InputOutput as c_uint,
        visual,
        (CWBackPixel | CWBorderPixel | CWColormap | CWOverrideRedirect | CWEventMask) as c_ulong,
        &mut swa,
    );

    // ── Make window click-through via XFixes input shape ──────────────────────
    let mut fixes_ev = 0i32;
    let mut fixes_er = 0i32;
    if XFixesQueryExtension(dpy, &mut fixes_ev, &mut fixes_er) != 0 {
        let empty = XFixesCreateRegion(dpy, ptr::null_mut(), 0);
        XFixesSetWindowShapeRegion(dpy, win, 2 /* ShapeInput */, 0, 0, empty);
    }

    // ── Drawing contexts ───────────────────────────────────────────────────────
    let mut gcv: XGCValues = std::mem::zeroed();
    let gc_bg:    GC = XCreateGC(dpy, win as Drawable, 0, &mut gcv);
    let gc_black: GC = XCreateGC(dpy, win as Drawable, 0, &mut gcv);
    let gc_white: GC = XCreateGC(dpy, win as Drawable, 0, &mut gcv);

    if has_argb {
        // ARGB path: use proper 32-bit pixel values
        XSetForeground(dpy, gc_bg,    ARGB_TRANSPARENT);
        XSetForeground(dpy, gc_black, ARGB_BLACK);
        XSetForeground(dpy, gc_white, ARGB_WHITE);
    } else {
        // Fallback path: system black/white
        XSetForeground(dpy, gc_black, black);
        XSetForeground(dpy, gc_white, white);
    }

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
            if has_argb {
                // Clear to fully transparent, then draw arrow
                XFillRectangle(dpy, win as Drawable, gc_bg, 0, 0, w, h);
            } else {
                // Clear to root background (pseudo-transparent)
                XClearWindow(dpy, win);
            }
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

unsafe fn draw_arrow(dpy: *mut Display, win: Window, gc_black: GC, gc_white: GC) {
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
