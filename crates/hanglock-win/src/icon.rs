//! The Hanglock mark, built at runtime from a mask. No `.ico` file, no resource compiler, no image
//! decoder in the dependency graph — and the icon is drawn by our own code, which is the point.
//!
//! The mark is the product: a plate on a cord. Small enough to survive 16×16 because the plate is a
//! solid block and the cord is one pixel wide at that size, which is exactly what makes it legible in
//! a crowded notification area.

use crate::sys;

/// A 32×32 BGRA image plus the AND mask `CreateIcon` still wants (all zero: no colour keying).
const S: i32 = 32;

#[must_use]
pub fn create(instance: sys::HMODULE) -> sys::HICON {
    let mut xor = vec![0u8; (S as usize) * (S as usize) * 4];
    for y in 0..S {
        for x in 0..S {
            let a = coverage(f64::from(x), f64::from(y));
            if a <= 0.0 {
                continue;
            }
            let i = ((y * S + x) as usize) * 4;
            // Premultiplied-ish BGRA is what a colour icon wants; the alpha channel here is the
            // mask's opacity, so shade the colour by it too.
            let (r, g, b) = if y < 7 { (70u8, 78u8, 92u8) } else { (238u8, 244u8, 252u8) };
            xor[i] = b;
            xor[i + 1] = g;
            xor[i + 2] = r;
            xor[i + 3] = (a * 255.0) as u8;
        }
    }
    let and = vec![0u8; ((S as usize) * (S as usize) / 8) * 4]; // 32 px rows, 4-byte aligned
    unsafe {
        sys::CreateIcon(
            instance,
            S,
            S,
            1,
            32,
            and.as_ptr(),
            xor.as_ptr(),
        )
    }
}

/// Signed distance to the mark, per pixel centre: a cord from y=6 to y=17, then a rounded plate.
fn coverage(x: f64, y: f64) -> f64 {
    let cx = 15.5;
    let cord = if (6.0..17.0).contains(&y) {
        aa(1.1 - (x - cx).abs())
    } else {
        0.0
    };
    let hw = 13.0;
    let hh = 8.0;
    let pcx = 15.5;
    let pcy = 23.5;
    let dx = (x - pcx).abs() - (hw - 2.5);
    let dy = (y - pcy).abs() - (hh - 2.5);
    let d = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() + (dx.min(0.0).max(dy.min(0.0))) - 2.5;
    let plate = aa(-d);
    let digits = if y > 21.0 && y < 26.0 && x > 6.0 && x < 25.0 && ((x - 6.0) % 4.0) < 2.4 {
        aa(0.9) * 0.75
    } else {
        0.0
    };
    cord.max(plate).max(digits).clamp(0.0, 1.0)
}

#[inline]
fn aa(d: f64) -> f64 {
    (0.5 - d).clamp(0.0, 1.0)
}
