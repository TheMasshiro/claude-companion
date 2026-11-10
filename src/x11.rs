//! The few X11 calls winit does not offer: window-manager hints, the pointer position,
//! the desktop work area, and an input shape so clicks on the transparent parts of the
//! window go through to whatever is underneath.

use crate::physics::V2;
use std::ffi::{c_int, c_long, c_uchar, c_uint, c_ulong};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use std::time::{Duration, Instant};
use x11_dl::xfixes::Xlib as XFixes; // x11-dl names the XFixes loader `Xlib` too
use x11_dl::xlib::{self, Display, Xlib};
use x11_dl::xss::XScreenSaverInfo;

/// `ShapeInput` from X11/extensions/shape.h.
const SHAPE_INPUT: c_int = 2;

/// The X screen-saver library, for idle time. Opened by hand because x11-dl looks for
/// `libXss.so.2` or the developer-only `libXss.so`, while distributions ship `libXss.so.1`.
struct Xss {
    alloc: unsafe extern "C" fn() -> *mut XScreenSaverInfo,
    query: unsafe extern "C" fn(*mut Display, c_ulong, *mut XScreenSaverInfo) -> c_int,
}

impl Xss {
    fn open() -> Option<Self> {
        for name in [c"libXss.so.1", c"libXss.so.2", c"libXss.so"] {
            unsafe {
                let lib = libc::dlopen(name.as_ptr(), libc::RTLD_LAZY);
                if lib.is_null() {
                    continue;
                }
                let alloc = libc::dlsym(lib, c"XScreenSaverAllocInfo".as_ptr());
                let query = libc::dlsym(lib, c"XScreenSaverQueryInfo".as_ptr());
                if !alloc.is_null() && !query.is_null() {
                    return Some(Xss { alloc: std::mem::transmute(alloc), query: std::mem::transmute(query) });
                }
                libc::dlclose(lib);
            }
        }
        None
    }
}

pub struct X11 {
    xlib: Xlib,
    xfixes: Option<XFixes>,
    xss: Option<Xss>,
    display: *mut Display,
    root: c_ulong,
    window: c_ulong,
    /// When we last moved the pointer, and when you last did anything.
    warped_at: Option<Instant>,
    active_at: Option<Instant>,
}

impl X11 {
    /// Shares winit's own Xlib connection; only used from the event-loop thread.
    pub fn new(window: &winit::window::Window) -> Option<Self> {
        let (RawDisplayHandle::Xlib(d), RawWindowHandle::Xlib(w)) =
            (window.display_handle().ok()?.as_raw(), window.window_handle().ok()?.as_raw())
        else {
            return None;
        };
        let xlib = Xlib::open().ok()?;
        let display = d.display?.as_ptr().cast();
        let root = unsafe { (xlib.XDefaultRootWindow)(display) };
        Some(X11 { xfixes: XFixes::open().ok(), xss: Xss::open(), xlib, display, root, window: w.window, warped_at: None, active_at: None })
    }

    /// Call before the window is mapped.
    /// - GNOME treats a new window that is denied focus as "demanding attention" and pops a
    ///   "Claude is ready" notification. WM_HINTS input = false (never wants keyboard focus) avoids that.
    /// - `_NET_WM_DESKTOP = 0xFFFFFFFF` puts it on every workspace.
    pub fn set_hints(&self) {
        let x = &self.xlib;
        // winit sets no WM_HINTS of its own on X11, so there is nothing to preserve
        let mut hints = xlib::XWMHints { flags: xlib::InputHint, input: xlib::False, ..unsafe { std::mem::zeroed() } };
        let all_desktops: c_long = 0xFFFF_FFFF; // format-32 properties are passed as C longs
        unsafe {
            (x.XSetWMHints)(self.display, self.window, &mut hints);
            let atom = (x.XInternAtom)(self.display, c"_NET_WM_DESKTOP".as_ptr(), xlib::False);
            let data = (&raw const all_desktops).cast();
            (x.XChangeProperty)(self.display, self.window, atom, xlib::XA_CARDINAL, 32, xlib::PropModeReplace, data, 1);
            (x.XFlush)(self.display);
        }
    }

    /// Pointer position in screen pixels, as the X server knows it.
    pub fn pointer(&self) -> Option<V2> {
        let (mut root, mut child): (c_ulong, c_ulong) = (0, 0);
        let (mut rx, mut ry, mut wx, mut wy): (c_int, c_int, c_int, c_int) = (0, 0, 0, 0);
        let mut mask: c_uint = 0;
        let ok = unsafe {
            (self.xlib.XQueryPointer)(self.display, self.root, &mut root, &mut child, &mut rx, &mut ry, &mut wx, &mut wy, &mut mask)
        };
        (ok != 0).then(|| V2::new(rx as f32, ry as f32))
    }

    /// Seconds since your last keyboard or mouse input (X screen-saver idle time), not counting
    /// our own pointer moves.
    pub fn away(&mut self) -> Option<f32> {
        let xss = self.xss.as_ref()?;
        let idle_ms = unsafe {
            let info = (xss.alloc)();
            if info.is_null() {
                return None;
            }
            let ok = (xss.query)(self.display, self.root, info);
            let idle = (*info).idle;
            (self.xlib.XFree)(info.cast());
            if ok == 0 {
                return None;
            }
            idle
        };
        let now = Instant::now();
        let input_at = now.checked_sub(Duration::from_millis(idle_ms as u64)).unwrap_or(now);
        // input that came after our last pointer move (plus a moment for it to land) is yours
        if self.warped_at.is_none_or(|w| input_at > w + Duration::from_millis(20)) {
            self.active_at = Some(self.active_at.map_or(input_at, |a| a.max(input_at)));
        }
        self.active_at.map(|a| (now - a).as_secs_f32())
    }

    /// Puts the pointer at `p` (screen pixels). Only used for pointer games while you are away.
    pub fn move_pointer(&mut self, p: V2) {
        unsafe {
            (self.xlib.XWarpPointer)(self.display, 0, self.root, 0, 0, 0, 0, p.x.round() as c_int, p.y.round() as c_int);
            (self.xlib.XFlush)(self.display);
        }
        self.warped_at = Some(Instant::now());
    }

    /// The desktop area not covered by panels or docks: x, y, width, height.
    pub fn work_area(&self) -> Option<[i32; 4]> {
        let x = &self.xlib;
        let (mut kind, mut format, mut count, mut after): (c_ulong, c_int, c_ulong, c_ulong) = (0, 0, 0, 0);
        let mut data: *mut c_uchar = std::ptr::null_mut();
        unsafe {
            let atom = (x.XInternAtom)(self.display, c"_NET_WORKAREA".as_ptr(), xlib::False);
            let status = (x.XGetWindowProperty)(
                self.display, self.root, atom, 0, 4, xlib::False, xlib::XA_CARDINAL,
                &mut kind, &mut format, &mut count, &mut after, &mut data,
            );
            if data.is_null() {
                return None;
            }
            let area = (status == 0 && format == 32 && count >= 4).then(|| {
                let v = std::slice::from_raw_parts(data.cast::<c_long>(), 4);
                [v[0] as i32, v[1] as i32, v[2] as i32, v[3] as i32]
            });
            (x.XFree)(data.cast());
            area
        }
    }

    /// Only this rectangle of the window takes mouse input; the rest is click-through.
    pub fn set_input_rect(&self, x: i32, y: i32, w: u32, h: u32) {
        let Some(xf) = &self.xfixes else { return };
        let mut rect = xlib::XRectangle { x: x as i16, y: y as i16, width: w as u16, height: h as u16 };
        unsafe {
            let region = (xf.XFixesCreateRegion)(self.display, &mut rect, 1);
            (xf.XFixesSetWindowShapeRegion)(self.display, self.window, SHAPE_INPUT, 0, 0, region);
            (xf.XFixesDestroyRegion)(self.display, region);
            (self.xlib.XFlush)(self.display);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_the_screen_saver_library_by_its_usual_name() {
        // only meaningful where the library is installed, as it is on most desktops
        let installed = ["/usr/lib/x86_64-linux-gnu", "/usr/lib64", "/usr/lib", "/usr/lib/aarch64-linux-gnu"]
            .iter()
            .any(|d| std::path::Path::new(d).join("libXss.so.1").exists());
        if installed {
            assert!(super::Xss::open().is_some());
        }
    }
}
