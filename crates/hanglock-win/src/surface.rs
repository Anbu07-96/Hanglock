//! The layered surface: a top-down 32 bpp DIB section, presented with `UpdateLayeredWindow`.
//!
//! Chosen over a GPU swapchain because a layered window wants a CPU bitmap anyway, because there is
//! no device to create and no `DEVICE_REMOVED` to recover from at 60 Hz, and because it makes
//! "present nothing unless something changed" the trivial case rather than a synchronisation
//! problem. `docs/gate-a.md` carries the measurement; if the per-present copy ever shows up as the
//! bottleneck, a `DirectComposition` path replaces *this file only*, behind the two methods below.

use crate::sys;
use hanglock_core::placement::Rect;
use std::ffi::c_void;

/// The DIB the window composites from. `bits` is memory owned by GDI, mapped for writing, so the
/// pixels reaching the window are the pixels a caller handed to [`Surface::present`] — and the
/// painter itself needs no `unsafe` to get there.
pub struct Surface {
    w: u32,
    h: u32,
    dc: sys::HDC,
    bitmap: sys::HBITMAP,
    old: *mut c_void,
    bits: *mut u8,
    valid: bool,
}

impl Surface {
    #[must_use]
    pub fn new() -> Self {
        Self {
            w: 0,
            h: 0,
            dc: std::ptr::null_mut(),
            bitmap: std::ptr::null_mut(),
            old: std::ptr::null_mut(),
            bits: std::ptr::null_mut(),
            valid: false,
        }
    }

    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// (Re)allocate the DIB. Contents are undefined afterwards, which is why callers repaint fully
    /// after a resize rather than presenting a dirty rect.
    pub fn resize(&mut self, w: u32, h: u32) {
        let w = w.max(1);
        let h = h.max(1);
        if self.valid && w == self.w && h == self.h {
            return;
        }
        self.destroy();
        self.w = w;
        self.h = h;
        unsafe {
            let screen = sys::GetDC(std::ptr::null_mut());
            let dc = sys::CreateCompatibleDC(screen);
            let mut info = sys::BITMAPINFO::default();
            info.header.bi_size = std::mem::size_of::<sys::BITMAPINFOHEADER>() as u32;
            info.header.bi_width = w as i32;
            // Negative height means top-down rows. The painter writes row 0 first; without this,
            // everything on screen is upside down and the bug takes an embarrassing while to spot.
            info.header.bi_height = -(h as i32);
            info.header.bi_planes = 1;
            info.header.bi_bit_count = 32;
            info.header.bi_compression = sys::BI_RGB;
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bmp = sys::CreateDIBSection(
                dc,
                &info,
                sys::DIB_RGB_COLORS as u32,
                &mut bits,
                std::ptr::null_mut(),
                0,
            );
            sys::ReleaseDC(std::ptr::null_mut(), screen);
            if bmp.is_null() || bits.is_null() {
                if !dc.is_null() {
                    sys::DeleteDC(dc);
                }
                self.valid = false;
                return;
            }
            self.dc = dc;
            self.bitmap = bmp;
            self.old = sys::SelectObject(dc, bmp);
            self.bits = bits.cast::<u8>();
            // A fresh DIB is zeroed by GDI on some drivers and not on others; transparent-black is
            // also what a cleared layered window should be, so make it explicit.
            std::ptr::write_bytes(self.bits, 0, (w as usize) * (h as usize) * 4);
            self.valid = true;
        }
    }

    /// Copy `rect` (or everything) from the painter's buffer into the DIB and update the window.
    ///
    /// # Safety
    /// `hwnd` is handed to `UpdateLayeredWindowIndirect`/`UpdateLayeredWindow`, which dereference
    /// it; it must be a live layered window.
    ///
    /// The rect is not decoration: presenting only the digits' box each second is the difference
    /// between a settled clock copying ~20 KB per second and copying the whole surface ~60 times a
    /// second, and the reference project's measurements say the copy is what costs, not the drawing.
    pub unsafe fn present(&mut self, hwnd: sys::HWND, src: &[u8], rect: Option<Rect>) -> bool {
        if !self.valid || self.bits.is_null() {
            return false;
        }
        let stride = self.w as usize * 4;
        let (x0, y0, x1, y1) = match rect {
            Some(r) => {
                let a = (r.x0.floor() as i64).clamp(0, self.w as i64) as usize;
                let b = (r.y0.floor() as i64).clamp(0, self.h as i64) as usize;
                let c = (r.x1.ceil() as i64).clamp(a as i64, self.w as i64) as usize;
                let d = (r.y1.ceil() as i64).clamp(b as i64, self.h as i64) as usize;
                (a, b, c, d)
            }
            None => (0usize, 0usize, self.w as usize, self.h as usize),
        };
        if x1 <= x0 || y1 <= y0 {
            return true;
        }
        unsafe {
            for y in y0..y1 {
                let off = y * stride + x0 * 4;
                let n = (x1 - x0) * 4;
                if off + n > src.len() || off + n > self.w as usize * self.h as usize * 4 {
                    break;
                }
                std::ptr::copy_nonoverlapping(src.as_ptr().add(off), self.bits.add(off), n);
            }
            let blend = sys::BLENDFUNCTION {
                blend_op: sys::AC_SRC_OVER,
                blend_flags: 0,
                source_constant_alpha: 255,
                alpha_format: sys::AC_SRC_ALPHA,
            };
            let size = sys::SIZE {
                cx: self.w as i32,
                cy: self.h as i32,
            };
            let src_pt = sys::POINT { x: 0, y: 0 };
            let dirty = sys::WINRECT {
                left: x0 as i32,
                top: y0 as i32,
                right: x1 as i32,
                bottom: y1 as i32,
            };
            // Indirect, not plain: `UpdateLayeredWindow` has no update-rect parameter at all, so
            // this is the only call that lets a settled clock present its digits' box instead of
            // the whole surface. `pt_dst` is null, which means "do not move the window".
            let mut info = sys::UPDATELAYEREDWINDOWINFO {
                cb_size: std::mem::size_of::<sys::UPDATELAYEREDWINDOWINFO>() as u32,
                pt_dst: std::ptr::null(),
                psize: &size,
                hdc_src: self.dc,
                pt_src: &src_pt,
                pblend: &blend,
                pv_source_transform: std::ptr::null(),
                cr_key: 0,
                dw_flags: sys::ULW_ALPHA,
                prc_dirty: &dirty,
            };
            if sys::UpdateLayeredWindowIndirect(hwnd, &info) != 0 {
                return true;
            }
            // Any OS where the Indirect path is unavailable still needs to paint; the plain call
            // loses the dirty rect and updates the whole window, which is slower but correct.
            info.prc_dirty = std::ptr::null();
            let _ = info;
            sys::UpdateLayeredWindow(
                hwnd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &size,
                self.dc,
                &src_pt,
                0,
                &blend,
                sys::ULW_ALPHA,
            ) != 0
        }
    }

    fn destroy(&mut self) {
        unsafe {
            if !self.bitmap.is_null() {
                if !self.dc.is_null() {
                    sys::SelectObject(self.dc, self.old);
                }
                sys::DeleteObject(self.bitmap);
            }
            if !self.dc.is_null() {
                sys::DeleteDC(self.dc);
            }
        }
        self.bitmap = std::ptr::null_mut();
        self.dc = std::ptr::null_mut();
        self.old = std::ptr::null_mut();
        self.bits = std::ptr::null_mut();
        self.valid = false;
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.destroy();
    }
}

impl Default for Surface {
    /// All-null handles; the surface becomes real on the first `resize`.
    fn default() -> Self {
        Self::new()
    }
}
