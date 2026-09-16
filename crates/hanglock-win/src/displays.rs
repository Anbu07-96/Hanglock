//! Enumerating displays, in the shape `hanglock-core`'s placement maths expects.

use crate::sys;
use hanglock_core::placement::{Monitor, Rect};

/// All monitors, in device px, each with the scale the window on it should use.
#[must_use]
pub fn enumerate(primary_hwnd: sys::HWND) -> Vec<Monitor> {
    // EnumDisplayMonitors is the only API that yields *every* monitor; it takes a callback, which a
    // no-unsafe-core design would rather not export, so the monitors are walked via the pair of
    // calls that need no callback: MonitorFromWindow for the current one, and
    // GetSystemMetrics(SM_CMONITORS)-indexed lookups for the rest.
    let count = unsafe { sys::GetSystemMetrics(sys::SM_CMONITORS) }.max(1) as usize;
    let mut out = Vec::with_capacity(count);
    let mut index = 0u32;
    // The only safe way to enumerate without a callback is from a known point on each display; the
    // virtual screen origin plus SM_CXVIRTUALSCREEN gives a scan line that crosses every monitor in
    // practice, and the primary is always resolvable from the window.
    let vx = unsafe { sys::GetSystemMetrics(sys::SM_XVIRTUALSCREEN) };
    let vy = unsafe { sys::GetSystemMetrics(sys::SM_YVIRTUALSCREEN) };
    let vw = unsafe { sys::GetSystemMetrics(sys::SM_CXVIRTUALSCREEN) };
    if vw <= 0 || count == 1 {
        out.push(monitor_for(primary_hwnd, 0));
        return out;
    }
    let mut seen: Vec<(i32, i32)> = Vec::new();
    // One probe per 64 px along the virtual screen's top band is enough to touch every monitor's
    // rectangle, and MonitorFromPoint collapses duplicates. Bounded: a 7680 px wide desktop is 120
    // probes, once, on a display change.
    let mut x = vx;
    let mut guard = 0;
    while x < vx + vw && guard < 4096 {
        guard += 1;
        let pt = sys::POINT { x: x + 8, y: vy + 8 };
        let hmon = unsafe { sys::MonitorFromPoint(pt, sys::MONITOR_DEFAULTTONEAREST) };
        if !hmon.is_null() {
            let (r, w, flags) = info(hmon);
            let key = (r.left, r.top);
            if !seen.contains(&key) {
                seen.push(key);
                let scale = scale_for_rect(r, primary_hwnd);
                out.push(Monitor {
                    index,
                    bounds: Rect::new(r.left as f64, r.top as f64, r.right as f64, r.bottom as f64),
                    work: Rect::new(w.left as f64, w.top as f64, w.right as f64, w.bottom as f64),
                    scale,
                    taskbar_top: false,
                    taskbar_auto_hidden: false,
                    primary: flags & sys::MONITORINFOF_PRIMARY != 0,
                });
                index += 1;
            }
        }
        x += 64;
    }
    if out.is_empty() {
        out.push(monitor_for(primary_hwnd, 0));
    }
    out
}

fn info(hmon: *mut std::ffi::c_void) -> (sys::WINRECT, sys::WINRECT, u32) {
    let mut mi = sys::MONITORINFO {
        cb_size: std::mem::size_of::<sys::MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        if sys::GetMonitorInfoW(hmon, &mut mi) == 0 {
            return (sys::WINRECT::default(), sys::WINRECT::default(), 0);
        }
    }
    (mi.rc_monitor, mi.rc_work, mi.dw_flags)
}

/// The scale to use for a monitor rect. `GetDpiForWindow` answers for a window, not a monitor, and
/// creating a probe window per display to learn its DPI is worse than deriving it once from the
/// window that is going to live there: the caller re-runs this on `WM_DPICHANGED`, which is exactly
/// when the answer changes.
fn scale_for_rect(rect: sys::WINRECT, hwnd: sys::HWND) -> f64 {
    let _ = rect;
    let dpi = unsafe { sys::GetDpiForWindow(hwnd) };
    let dpi = if dpi == 0 { sys::dpi_of_window(hwnd) } else { dpi };
    (dpi as f64 / 96.0).max(0.5)
}

#[must_use]
pub fn monitor_for(hwnd: sys::HWND, index: u32) -> Monitor {
    let hmon = unsafe { sys::MonitorFromWindow(hwnd, sys::MONITOR_DEFAULTTONEAREST) };
    let (r, w, flags) = if hmon.is_null() {
        let cx = unsafe { sys::GetSystemMetrics(sys::SM_CXSCREEN) };
        let cy = unsafe { sys::GetSystemMetrics(sys::SM_CYSCREEN) };
        (
            sys::WINRECT { left: 0, top: 0, right: cx, bottom: cy },
            sys::WINRECT { left: 0, top: 0, right: cx, bottom: cy },
            sys::MONITORINFOF_PRIMARY,
        )
    } else {
        info(hmon)
    };
    let dpi = unsafe { sys::GetDpiForWindow(hwnd) };
    let dpi = if dpi == 0 { sys::dpi_of_window(hwnd) } else { dpi };
    let (taskbar_top, auto_hide) = sys::query_taskbar(hwnd).unwrap_or((false, false));
    Monitor {
        index,
        bounds: Rect::new(r.left as f64, r.top as f64, r.right as f64, r.bottom as f64),
        work: Rect::new(w.left as f64, w.top as f64, w.right as f64, w.bottom as f64),
        scale: (dpi as f64 / 96.0).max(0.5),
        taskbar_top,
        taskbar_auto_hidden: auto_hide,
        primary: flags & sys::MONITORINFOF_PRIMARY != 0,
    }
}
