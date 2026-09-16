//! The pixel buffer, its bounds, and the only blend this renderer uses.

use hanglock_core::placement::Rect;
use hanglock_core::vec2::Vec2;

/// Premultiplied BGRA8, top-down.
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
    /// Union of everything drawn since the last [`Canvas::clear`], in device px, inclusive.
    /// Empty when nothing was drawn, which is how "present nothing" is expressed.
    pub drawn: [f64; 4],
}

impl Canvas {
    #[must_use]
    pub fn new(w: u32, h: u32) -> Self {
        Self { w, h, px: vec![0; (w as usize) * (h as usize) * 4], drawn: [0.0; 4] }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w == self.w && h == self.h {
            return;
        }
        self.w = w;
        self.h = h;
        self.px.clear();
        self.px.resize((w as usize) * (h as usize) * 4, 0);
    }

    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    #[inline]
    pub fn clear(&mut self) {
        for b in &mut self.px {
            *b = 0;
        }
        self.drawn = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    }

    /// Widen the drawn bounds across a whole scanline at once. The primitives here are spans, so
    /// tracking one row per scanline is exact and cheaper than a call per pixel.
    #[inline]
    pub fn touch_row(&mut self, y: i32, x0: i32, x1: i32) {
        let d = &mut self.drawn;
        d[0] = d[0].min(x0 as f64);
        d[1] = d[1].min(y as f64);
        d[2] = d[2].max(x1 as f64);
        d[3] = d[3].max(y as f64);
    }

    #[inline]
    pub fn touch(&mut self, x: i32, y: i32) {
        let d = &mut self.drawn;
        d[0] = d[0].min(x as f64);
        d[1] = d[1].min(y as f64);
        d[2] = d[2].max(x as f64);
        d[3] = d[3].max(y as f64);
    }

    /// Source-over with premultiplied src: `dst = src + dst * (1 - srcA)`.
    ///
    /// `x`/`y` are already clipped by the caller's loop bounds, so this stays one indexed add per
    /// channel and the bounds test lives where it can be hoisted.
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, b: u16, g: u16, r: u16, a: u16) {
        let i = (y as usize * self.w as usize + x as usize) * 4;
        if a >= 255 {
            self.px[i] = b as u8;
            self.px[i + 1] = g as u8;
            self.px[i + 2] = r as u8;
            self.px[i + 3] = 255;
            return;
        }
        let inv = 255 - a;
        self.px[i] = (b + (self.px[i] as u16 * inv) / 255).min(255) as u8;
        self.px[i + 1] = (g + (self.px[i + 1] as u16 * inv) / 255).min(255) as u8;
        self.px[i + 2] = (r + (self.px[i + 2] as u16 * inv) / 255).min(255) as u8;
        self.px[i + 3] = (a + (self.px[i + 3] as u16 * inv) / 255).min(255) as u8;
    }

    /// Read the alpha of one pixel — used by the tests that assert a soft shadow is present where
    /// it should be and, crucially, that fully transparent pixels are *exactly* zero so click
    /// through works.
    #[must_use]
    pub fn alpha_at(&self, x: u32, y: u32) -> u8 {
        let i = (y as usize * self.w as usize + x as usize) * 4 + 3;
        *self.px.get(i).unwrap_or(&0)
    }

    /// [`Canvas::blend`], with clipping and bounds tracking. Slightly slower per pixel, so the hot
    /// loops use `blend` directly with pre-clipped bounds and this for sparse writes.
    pub fn blend_clipped(&mut self, p: Vec2, b: u16, g: u16, r: u16, a: u16) {
        let x = p.x.floor() as i64;
        let y = p.y.floor() as i64;
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 || a == 0 {
            return;
        }
        self.blend(x as i32, y as i32, b, g, r, a);
        self.touch(x as i32, y as i32);
    }

    /// The rect to hand to the presenter: the drawn bounds, expanded and clipped, snapped outward to
    /// whole pixels.
    #[must_use]
    pub fn present_rect(&self) -> Option<Rect> {
        let d = self.drawn;
        if d[0] > d[2] || d[1] > d[3] {
            return None;
        }
        let x0 = (d[0].floor() as i64).clamp(0, self.w as i64 - 1) as f64;
        let y0 = (d[1].floor() as i64).clamp(0, self.h as i64 - 1) as f64;
        let x1 = (d[2].ceil() as i64 + 1).clamp(1, self.w as i64) as f64;
        let y1 = (d[3].ceil() as i64 + 1).clamp(1, self.h as i64) as f64;
        Some(Rect::new(x0, y0, x1, y1))
    }
}

/// Half-pixel-centred sample position for a whole-number pixel index. Every coverage evaluation in
/// this crate uses it, so a 1 px band is a 1 px band on both axes.
#[must_use]
pub fn centre(x: i32, y: i32) -> Vec2 {
    Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5)
}

/// Distance from `p` to segment `a`-`b`. The primitive of this renderer: a capsule is a distance,
/// a rounded box is a distance, and a glyph stroke is a run of capsules.
#[must_use]
pub fn dist_seg(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let v = b.sub(a);
    let l2 = v.x * v.x + v.y * v.y;
    if l2 <= 1e-12 {
        return p.dist(a);
    }
    let t = ((p.x - a.x) * v.x + (p.y - a.y) * v.y) / l2;
    let t = t.clamp(0.0, 1.0);
    p.dist(Vec2::new(a.x + v.x * t, a.y + v.y * t))
}

/// Signed distance to a rounded box centred on the origin. Negative inside, and exact outside —
/// which is all coverage needs, since coverage only reads `|d|` near zero.
#[must_use]
pub fn sdf_round_box(p: Vec2, half: Vec2, r: f64) -> f64 {
    let rr = r.min(half.x).min(half.y);
    let qx = (p.x.abs() - (half.x - rr)).max(0.0);
    let qy = (p.y.abs() - (half.y - rr)).max(0.0);
    (qx * qx + qy * qy).sqrt() - rr
}

/// Linear ramp from full coverage at `edge0` to none at `edge1`. The anti-aliaser, the shadow and
/// the 1 px rim are all this one function at different widths; `edge0 > edge1` for a downward ramp.
#[must_use]
pub fn smooth(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
