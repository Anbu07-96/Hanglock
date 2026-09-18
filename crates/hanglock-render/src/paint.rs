//! Every draw in Hanglock. Three primitives — capsule, rounded box, annulus — as analytic
//! coverage; the plate, the cord, the mount and the digits are all combinations of them.
//!
//! Order is fixed: cord, mount, plate, digits. It is the painter's version of "the cord
//! disappears behind the plate", and it is why the joint needs no bookkeeping: the plate is drawn
//! over the cord's end, so the cord cannot detach from the object it is holding no matter what the
//! solver does.

use crate::canvas::{centre, dist_seg, sdf_round_box, smooth, Canvas};
use crate::face_data::{self, ADVANCE, STROKE_RATIO, TRACKING};
use crate::theme::{Rgba, Theme};
use hanglock_core::ids::ClockStyle;
use hanglock_core::placement::Rect;
use hanglock_core::scene::Scene;
use hanglock_core::vec2::Vec2;

/// Rotate a point about `o` by `t`.
#[inline]
fn rot(o: Vec2, t: f64, p: Vec2) -> Vec2 {
    let (cs, sn) = (t.cos(), t.sin());
    let d = p.sub(o);
    Vec2::new(o.x + d.x * cs - d.y * sn, o.y + d.x * sn + d.y * cs)
}

/// An anti-aliased capsule: the set of points within `rad` of segment `a`-`b`.
///
/// `aa` is the ramp width in device px. At 1.05 px it covers exactly the 1-pixel-wide band the
/// eye reads as an edge; widening it to blur a shape instead costs a multiply per pixel and turns
/// a cord into fog, which is why blur is not a primitive here at all.
fn capsule(cv: &mut Canvas, a: Vec2, b: Vec2, rad: f64, col: Rgba, aa: f64) {
    if col.a <= 0.001 || rad <= 0.0 {
        return;
    }
    let pad = rad + 1.0;
    let x0 = (a.x.min(b.x) - pad).floor() as i64;
    let x1 = (a.x.max(b.x) + pad).ceil() as i64;
    let y0 = (a.y.min(b.y) - pad).floor() as i64;
    let y1 = (a.y.max(b.y) + pad).ceil() as i64;
    let (w, h) = (i64::from(cv.w), i64::from(cv.h));
    // `blend` takes (b, g, r) because the buffer is BGRA; naming them pb/pg/pr in that order is
    // the only defence against the class of bug where a swap costs a day of blue-tinted cord.
    let (pb, pg, pr) = (col.b * 255.0, col.g * 255.0, col.r * 255.0);
    for y in y0.max(0)..=y1.min(h - 1) {
        for x in x0.max(0)..=x1.min(w - 1) {
            let p = centre(x as i32, y as i32);
            let d = dist_seg(p, a, b) - rad;
            let cov = smooth(aa * 0.5, -aa * 0.5, d) * col.a;
            if cov <= 0.002 {
                continue;
            }
            cv.blend(
                x as i32,
                y as i32,
                (pb * cov) as u16,
                (pg * cov) as u16,
                (pr * cov) as u16,
                (cov * 255.0) as u16,
            );
        }
        cv.touch_row(y as i32, x0.max(0) as i32, x1.min(w - 1) as i32);
    }
}

/// A filled stroke with flat terminals. The old face reused the cord's capsules, which made every
/// numeral look handwritten. Clock type is an outline, not a rope: this primitive keeps the same
/// allocation-free coverage loop while giving the generated geometry crisp, designed ends.
fn flat_stroke(cv: &mut Canvas, a: Vec2, b: Vec2, rad: f64, col: Rgba, aa: f64) {
    let d = b.sub(a);
    let len = d.len();
    if len <= 1e-6 {
        capsule(cv, a, b, rad, col, aa);
        return;
    }
    let u = d.scale(1.0 / len);
    let pad = rad + 1.0;
    let x0 = (a.x.min(b.x) - pad).floor() as i64;
    let x1 = (a.x.max(b.x) + pad).ceil() as i64;
    let y0 = (a.y.min(b.y) - pad).floor() as i64;
    let y1 = (a.y.max(b.y) + pad).ceil() as i64;
    let (w, h) = (i64::from(cv.w), i64::from(cv.h));
    for y in y0.max(0)..=y1.min(h - 1) {
        for x in x0.max(0)..=x1.min(w - 1) {
            let p = centre(x as i32, y as i32).sub(a);
            let along = p.x * u.x + p.y * u.y;
            let across = (p.x * u.y - p.y * u.x).abs();
            let outside = (-along).max(along - len).max(0.0);
            let dist = if outside > 0.0 {
                (outside * outside + across * across).sqrt()
            } else {
                across
            };
            let cov = smooth(aa * 0.5, -aa * 0.5, dist - rad) * col.a;
            if cov > 0.002 {
                cv.blend(
                    x as i32,
                    y as i32,
                    (col.b * 255.0 * cov) as u16,
                    (col.g * 255.0 * cov) as u16,
                    (col.r * 255.0 * cov) as u16,
                    (cov * 255.0) as u16,
                );
            }
        }
        cv.touch_row(y as i32, x0.max(0) as i32, x1.min(w - 1) as i32);
    }
}

/// A filled rounded box, optionally with a vertical gradient. `v` is the top-to-bottom ramp.
fn box_fill(cv: &mut Canvas, c: Vec2, half: Vec2, r: f64, top: Rgba, bot: Rgba, aa: f64) {
    let pad = 1.5;
    let x0 = (c.x - half.x - pad).floor() as i64;
    let x1 = (c.x + half.x + pad).ceil() as i64;
    let y0 = (c.y - half.y - pad).floor() as i64;
    let y1 = (c.y + half.y + pad).ceil() as i64;
    let (w, h) = (i64::from(cv.w), i64::from(cv.h));
    for y in y0.max(0)..=y1.min(h - 1) {
        for x in x0.max(0)..=x1.min(w - 1) {
            let p = centre(x as i32, y as i32);
            let d = sdf_round_box(p.sub(c), half, r);
            let cov = smooth(aa * 0.5, -aa * 0.5, d);
            if cov <= 0.002 {
                continue;
            }
            let t = ((p.y - (c.y - half.y)) / (2.0 * half.y)).clamp(0.0, 1.0);
            let lerp = |a: f64, b: f64| a + (b - a) * t;
            let a = lerp(top.a, bot.a) * cov;
            cv.blend(
                x as i32,
                y as i32,
                (lerp(top.b, bot.b) * 255.0 * a) as u16,
                (lerp(top.g, bot.g) * 255.0 * a) as u16,
                (lerp(top.r, bot.r) * 255.0 * a) as u16,
                (a * 255.0) as u16,
            );
        }
        cv.touch_row(y as i32, x0.max(0) as i32, x1.min(w - 1) as i32);
    }
}

/// Distance-field soft shadow: the box's distance dilated by `blur`. One pass, no offscreen
/// surface, and a shadow that cannot detach from the shape it belongs to at any rotation.
fn box_shadow(cv: &mut Canvas, c: Vec2, half: Vec2, r: f64, drop: f64, blur: f64, alpha: f64) {
    if alpha <= 0.001 {
        return;
    }
    let pad = blur * 2.6;
    let x0 = (c.x - half.x - pad).floor() as i64;
    let x1 = (c.x + half.x + pad).ceil() as i64;
    let y0 = (c.y - half.y - pad).floor() as i64;
    let y1 = (c.y + half.y + drop + pad).ceil() as i64;
    let (w, h) = (i64::from(cv.w), i64::from(cv.h));
    for y in y0.max(0)..=y1.min(h - 1) {
        for x in x0.max(0)..=x1.min(w - 1) {
            let p = centre(x as i32, y as i32).sub(Vec2::new(0.0, drop));
            let d = sdf_round_box(p.sub(c), half, r);
            let cov = smooth(blur, -blur * 0.35, d) * alpha;
            if cov <= 0.002 {
                continue;
            }
            cv.blend(x as i32, y as i32, 0, 0, 0, (cov * 255.0) as u16);
        }
        cv.touch_row(y as i32, x0.max(0) as i32, x1.min(w - 1) as i32);
    }
}

/// A ring: the cord passes through it, and it is the one drawing that says "this hangs from
/// something" rather than "this is a window with a line drawn on it".
fn annulus(cv: &mut Canvas, c: Vec2, r: f64, w_: f64, col: Rgba) {
    let pad = r + w_ + 1.0;
    let x0 = (c.x - pad).floor() as i64;
    let x1 = (c.x + pad).ceil() as i64;
    let y0 = (c.y - pad).floor() as i64;
    let y1 = (c.y + pad).ceil() as i64;
    let (ww, h) = (i64::from(cv.w), i64::from(cv.h));
    for y in y0.max(0)..=y1.min(h - 1) {
        for x in x0.max(0)..=x1.min(ww - 1) {
            let p = centre(x as i32, y as i32);
            let d = (p.dist(c) - r).abs() - w_ * 0.5;
            let cov = smooth(0.55, -0.55, d) * col.a;
            if cov <= 0.002 {
                continue;
            }
            cv.blend(
                x as i32,
                y as i32,
                (col.b * 255.0 * cov) as u16,
                (col.g * 255.0 * cov) as u16,
                (col.r * 255.0 * cov) as u16,
                (cov * 255.0) as u16,
            );
        }
        cv.touch_row(y as i32, x0.max(0) as i32, x1.min(ww - 1) as i32);
    }
}

/// Total advance of a run, in device px, and the cap height it was measured against.
#[must_use]
pub fn text_advance(chars: usize, cap: f64) -> f64 {
    if chars == 0 {
        return 0.0;
    }
    chars as f64 * (cap * f64::from(ADVANCE) + cap * f64::from(TRACKING))
        - cap * f64::from(TRACKING)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DigitalLayout {
    cap: f64,
    main_x: f64,
    y: f64,
    primary_chars: usize,
    seconds_cap: f64,
    seconds_x: f64,
    suffix_cap: f64,
    suffix_x: f64,
}

impl DigitalLayout {
    fn glyph(self, index: usize) -> (f64, f64) {
        if index < self.primary_chars {
            (
                self.main_x
                    + index as f64
                        * (self.cap * f64::from(ADVANCE)
                            + self.cap * f64::from(TRACKING)),
                self.cap,
            )
        } else {
            let i = index - self.primary_chars;
            (
                self.seconds_x
                    + i as f64
                        * (self.seconds_cap * f64::from(ADVANCE)
                            + self.seconds_cap * f64::from(TRACKING)),
                self.seconds_cap,
            )
        }
    }
}

fn digital_layout(scene: &Scene, theme: &Theme) -> DigitalLayout {
    let radius = scene.card_w.min(scene.card_h) * 0.5;
    let main_count = scene.text.as_str().chars().count();
    let secondary_count = usize::from(main_count >= 8) * 3;
    let primary_count = main_count - secondary_count;
    let main_chars = primary_count as f64;
    let seconds_chars = secondary_count as f64;
    let suffix_chars = scene.text.suffix_str().chars().count() as f64;
    let primary_units =
        (main_chars * (f64::from(ADVANCE) + f64::from(TRACKING)) - f64::from(TRACKING)).max(0.0);
    let seconds_units = if seconds_chars > 0.0 {
        0.16 + seconds_chars * (f64::from(ADVANCE) + f64::from(TRACKING)) * theme.seconds_scale
            - f64::from(TRACKING) * theme.seconds_scale
    } else {
        0.0
    };
    let suffix_ratio = theme.suffix_cap;
    let suffix_units = suffix_chars * (f64::from(ADVANCE) + 0.16) * suffix_ratio;
    let gap_ratio = if suffix_chars > 0.0 {
        theme.suffix_gap
    } else {
        0.0
    };
    let units = (primary_units + seconds_units + suffix_units + gap_ratio).max(1.0);
    let cap = (scene.card_h * theme.time_cap)
        .min(radius * 0.58)
        .min(radius * 1.45 / units);
    let primary_width = primary_units * cap;
    let seconds_width = seconds_units * cap;
    let gap = gap_ratio * cap;
    let suffix_width = suffix_units * cap;
    let total = primary_width + seconds_width + gap + suffix_width;
    let y = scene.card_centre.y - cap * 0.5;
    DigitalLayout {
        cap,
        main_x: scene.card_centre.x - total * 0.5,
        y,
        primary_chars: primary_count,
        seconds_cap: cap * theme.seconds_scale,
        seconds_x: scene.card_centre.x - total * 0.5 + primary_width + 0.16 * cap,
        suffix_cap: cap * suffix_ratio,
        suffix_x: scene.card_centre.x - total * 0.5 + primary_width + seconds_width + gap,
    }
}

#[must_use]
pub fn time_cap(scene: &Scene, theme: &Theme) -> f64 {
    digital_layout(scene, theme).cap
}

/// The digits' bounding box, device px, including the meridiem. The app presents only this rect
/// when the rope has not moved, so the steady state of a clock is a small rectangle per second
/// rather than a whole layered window.
#[must_use]
pub fn text_bounds(scene: &Scene, theme: &Theme) -> Rect {
    let centre = scene.card_centre;
    let layout = digital_layout(scene, theme);
    let cap = layout.cap;
    let rad = f64::from(STROKE_RATIO) * 0.5 * cap;
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    let main = scene.text.as_str();
    let suffix = scene.text.suffix_str();
    let oy = layout.y;
    let scap = layout.suffix_cap;
    for (i, ch) in main.chars().enumerate() {
        let Some(g) = face_data::glyph(ch) else {
            continue;
        };
        let (gx, glyph_cap) = layout.glyph(i);
        for ln in g.lines {
            for p in *ln {
                let q = rot(
                    centre,
                    scene.theta,
                    Vec2::new(
                        gx + f64::from(p.x) * glyph_cap,
                        oy + (cap - glyph_cap) * 0.55 + f64::from(p.y) * glyph_cap,
                    ),
                );
                let glyph_rad = rad * glyph_cap / cap;
                b[0] = b[0].min(q.x - glyph_rad);
                b[1] = b[1].min(q.y - glyph_rad);
                b[2] = b[2].max(q.x + glyph_rad);
                b[3] = b[3].max(q.y + glyph_rad);
            }
        }
        for d in g.dots {
            let (gx, glyph_cap) = layout.glyph(i);
            let q = rot(
                centre,
                scene.theta,
                Vec2::new(
                    gx + f64::from(d.x) * glyph_cap,
                    oy + (cap - glyph_cap) * 0.55 + f64::from(d.y) * glyph_cap,
                ),
            );
            b[0] = b[0].min(q.x - f64::from(d.r) * glyph_cap);
            b[1] = b[1].min(q.y - f64::from(d.r) * glyph_cap);
            b[2] = b[2].max(q.x + f64::from(d.r) * glyph_cap);
            b[3] = b[3].max(q.y + f64::from(d.r) * glyph_cap);
        }
    }
    if !suffix.is_empty() {
        let sox = layout.suffix_x;
        let soy = oy + (cap - scap) * 0.35;
        for (i, ch) in suffix.chars().enumerate() {
            let Some(g) = face_data::glyph(ch) else {
                continue;
            };
            let gx = sox + i as f64 * (scap * f64::from(ADVANCE) + scap * 0.16);
            for ln in g.lines {
                for p in *ln {
                    let q = rot(
                        centre,
                        scene.theta,
                        Vec2::new(gx + f64::from(p.x) * scap, soy + f64::from(p.y) * scap),
                    );
                    b[0] = b[0].min(q.x);
                    b[1] = b[1].min(q.y);
                    b[2] = b[2].max(q.x);
                    b[3] = b[3].max(q.y);
                }
            }
        }
    }
    if b[0] > b[2] {
        return Rect::new(0.0, 0.0, 0.0, 0.0);
    }
    Rect::new(b[0], b[1], b[2], b[3])
}

fn draw_text(cv: &mut Canvas, scene: &Scene, theme: &Theme) {
    let centre = scene.card_centre;
    let layout = digital_layout(scene, theme);
    let cap = layout.cap;
    let rad = f64::from(STROKE_RATIO) * 0.5 * cap;
    let main = scene.text.as_str();
    let suffix = scene.text.suffix_str();
    let oy = layout.y;
    let scap = layout.suffix_cap;

    for (i, ch) in main.chars().enumerate() {
        let Some(g) = face_data::glyph(ch) else {
            continue;
        };
        let (gx, glyph_cap) = layout.glyph(i);
        let glyph_y = oy + (cap - glyph_cap) * 0.55;
        for ln in g.lines {
            if ln.len() < 2 {
                continue;
            }
            for w in ln.windows(2) {
                let a = rot(
                    centre,
                    scene.theta,
                    Vec2::new(
                        gx + f64::from(w[0].x) * glyph_cap,
                        glyph_y + f64::from(w[0].y) * glyph_cap,
                    ),
                );
                let b = rot(
                    centre,
                    scene.theta,
                    Vec2::new(
                        gx + f64::from(w[1].x) * glyph_cap,
                        glyph_y + f64::from(w[1].y) * glyph_cap,
                    ),
                );
                flat_stroke(cv, a, b, rad * glyph_cap / cap, theme.ink, 1.05);
            }
        }
        for d in g.dots {
            let p = rot(
                centre,
                scene.theta,
                Vec2::new(
                    gx + f64::from(d.x) * glyph_cap,
                    glyph_y + f64::from(d.y) * glyph_cap,
                ),
            );
            capsule(
                cv,
                p,
                p,
                f64::from(d.r) * glyph_cap * 0.95,
                theme.ink,
                1.05,
            );
        }
    }
    if !suffix.is_empty() {
        let sox = layout.suffix_x;
        let soy = oy + (cap - scap) * 0.35;
        for (i, ch) in suffix.chars().enumerate() {
            let Some(g) = face_data::glyph(ch) else {
                continue;
            };
            let gx = sox + i as f64 * (scap * f64::from(ADVANCE) + scap * 0.16);
            for ln in g.lines {
                if ln.len() < 2 {
                    continue;
                }
                for w in ln.windows(2) {
                    let a = rot(
                        centre,
                        scene.theta,
                        Vec2::new(
                            gx + f64::from(w[0].x) * scap,
                            soy + f64::from(w[0].y) * scap,
                        ),
                    );
                    let b = rot(
                        centre,
                        scene.theta,
                        Vec2::new(
                            gx + f64::from(w[1].x) * scap,
                            soy + f64::from(w[1].y) * scap,
                        ),
                    );
                    flat_stroke(
                        cv,
                        a,
                        b,
                        f64::from(STROKE_RATIO) * 0.5 * scap * 1.36,
                        theme.accent,
                        1.05,
                    );
                }
            }
        }
    }
}

/// Paint a whole frame. `cv` is cleared first by the caller, since the caller knows whether the
/// frame is a repaint or a fresh size.
/// The clamp against the top of the screen, and the ring the cord runs through.
fn paint_mount(cv: &mut Canvas, scene: &Scene, theme: &Theme) {
    let a = scene.anchor;
    let (rail, rail_h) = match theme.style {
        ClockStyle::ModernMinimal => (13.0, 2.6),
        ClockStyle::PremiumMetalGlass => (15.0, 3.1),
        ClockStyle::SoftMattePlayful => (12.0, 3.6),
    };
    box_shadow(
        cv,
        a,
        Vec2::new(rail * scene.scale, rail_h * scene.scale),
        3.0 * scene.scale,
        1.0,
        2.2,
        0.20,
    );
    box_fill(
        cv,
        Vec2::new(a.x, a.y + 0.7 * scene.scale),
        Vec2::new(rail * scene.scale, rail_h * scene.scale),
        3.0 * scene.scale,
        theme.mount,
        theme.mount,
        1.05,
    );
    annulus(
        cv,
        Vec2::new(a.x, a.y + 6.2 * scene.scale),
        theme.eyelet_radius * 0.72 * scene.scale,
        theme.eyelet_width * scene.scale,
        theme.mount,
    );
}

/// The plate body: one rounded-box distance field, evaluated under the inverse
/// rotation so a tilted object needs no transformed render target. Gradient, rim and
/// shadow band fall out of the same signed distance and corner half-widths.
fn paint_plate(cv: &mut Canvas, scene: &Scene, theme: &Theme) {
    let c = scene.card_centre;
    let radius = scene.card_w.min(scene.card_h) * 0.5;
    let shadow = theme.shadow_alpha * scene.opacity;
    let extent = radius + theme.shadow_blur * scene.scale + 3.0;
    box_shadow(
        cv,
        c,
        Vec2::new(radius, radius),
        radius,
        theme.shadow_drop * scene.scale,
        theme.shadow_blur * scene.scale,
        shadow,
    );
    let x0 = (c.x - extent).floor() as i64;
    let x1 = (c.x + extent).ceil() as i64;
    let y0 = (c.y - extent).floor() as i64;
    let y1 = (c.y + extent).ceil() as i64;
    let (w_, h_) = (i64::from(cv.w), i64::from(cv.h));
    for y in y0.max(0)..=y1.min(h_ - 1) {
        for x in x0.max(0)..=x1.min(w_ - 1) {
            let p = centre(x as i32, y as i32);
            let d = p.dist(c) - radius;
            let cov = smooth(0.55, -0.55, d);
            if cov <= 0.0005 {
                continue;
            }
            let t = ((p.y - (c.y - radius)) / (2.0 * radius)).clamp(0.0, 1.0);
            let lerp = |u: f64, v: f64| u + (v - u) * t;
            let edge_from_outer = radius - p.dist(c);
            let face_radius = radius - theme.inset * scene.scale;
            let face_d = p.dist(c) - face_radius;
            let nx = (p.x - c.x) / radius.max(1.0);
            let ny = (p.y - c.y) / radius.max(1.0);
            let directional = ((-nx - ny) * 0.5 + 0.5).clamp(0.0, 1.0);
            let (r, g, bch, al) = if edge_from_outer < theme.rim_width * scene.scale {
                let hi = directional;
                (
                    lerp(theme.rim_bottom.r, theme.rim_top.r) * hi + theme.rim.r * (1.0 - hi),
                    lerp(theme.rim_bottom.g, theme.rim_top.g) * hi + theme.rim.g * (1.0 - hi),
                    lerp(theme.rim_bottom.b, theme.rim_top.b) * hi + theme.rim.b * (1.0 - hi),
                    theme.rim.a,
                )
            } else if face_d <= 0.0 {
                let glass = if theme.style == ClockStyle::PremiumMetalGlass {
                    directional * 0.025
                } else {
                    0.0
                };
                (
                    theme.face.r + glass,
                    theme.face.g + glass,
                    theme.face.b + glass,
                    theme.face.a,
                )
            } else if face_d < 1.4 * scene.scale {
                (
                    theme.inner_rim.r,
                    theme.inner_rim.g,
                    theme.inner_rim.b,
                    theme.inner_rim.a,
                )
            } else {
                (
                    lerp(theme.plate_top.r, theme.plate_bottom.r),
                    lerp(theme.plate_top.g, theme.plate_bottom.g),
                    lerp(theme.plate_top.b, theme.plate_bottom.b),
                    lerp(theme.plate_top.a, theme.plate_bottom.a),
                )
            };
            let a2 = al * cov * scene.opacity;
            cv.blend(
                x as i32,
                y as i32,
                (bch * 255.0 * a2) as u16,
                (g * 255.0 * a2) as u16,
                (r * 255.0 * a2) as u16,
                (a2 * 255.0) as u16,
            );
        }
        cv.touch_row(y as i32, x0.max(0) as i32, x1.min(w_ - 1) as i32);
    }
    let up = Vec2::new(scene.theta.sin(), -scene.theta.cos());
    let eyelet = c.add(up.scale(radius - theme.eyelet_radius * 1.8 * scene.scale));
    annulus(
        cv,
        eyelet,
        theme.eyelet_radius * scene.scale,
        theme.eyelet_width * scene.scale,
        theme.accent,
    );
}

pub fn paint(scene: &Scene, cv: &mut Canvas, theme: &Theme) {
    cv.clear();
    let n = scene.node_count as usize;
    if n < 2 {
        return;
    }
    let pts = &scene.nodes[..n];

    // 1. the cord, cut where the plate covers it. Trimming by "inside the plate" rather than by a
    // fixed inset is what keeps the joint honest when the cord is bent.
    let mut last = n - 1;
    while last > 1 && scene.point_in_plate(pts[last]) {
        last -= 1;
    }
    let w = theme.rope_width * scene.scale;
    for i in 0..last {
        capsule(
            cv,
            pts[i].add(Vec2::new(1.3, 1.3)),
            pts[i + 1].add(Vec2::new(1.3, 1.3)),
            w * 1.28,
            theme.cord_shadow,
            1.05,
        );
    }
    for i in 0..last {
        capsule(cv, pts[i], pts[i + 1], w, theme.cord, 1.05);
    }
    for i in 0..last {
        let o = Vec2::new(-w * 0.24, -w * 0.24);
        capsule(
            cv,
            pts[i].add(o),
            pts[i + 1].add(o),
            w * theme.rope_edge,
            theme.cord_lit,
            1.05,
        );
    }

    // 2. the mount.
    paint_mount(cv, scene, theme);

    // 3. the plate.
    paint_plate(cv, scene, theme);

    // 4. digits, last so nothing can overprint them.
    draw_text(cv, scene, theme);
}
