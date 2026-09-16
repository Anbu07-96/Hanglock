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

/// Inverse rotation: device space into the plate's own frame.
#[inline]
fn unrot(o: Vec2, t: f64, p: Vec2) -> Vec2 {
    let (cs, sn) = (t.cos(), t.sin());
    let d = p.sub(o);
    Vec2::new(o.x + d.x * cs + d.y * sn, o.y - d.x * sn + d.y * cs)
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
            cv.blend(x as i32, y as i32, (col.b * 255.0 * cov) as u16, (col.g * 255.0 * cov) as u16, (col.r * 255.0 * cov) as u16, (cov * 255.0) as u16);
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
    chars as f64 * (cap * f64::from(ADVANCE) + cap * f64::from(TRACKING)) - cap * f64::from(TRACKING)
}

#[must_use]
pub fn time_cap(scene: &Scene, theme: &Theme) -> f64 {
    scene.card_h * theme.time_cap
}

/// The digits' bounding box, device px, including the meridiem. The app presents only this rect
/// when the rope has not moved, so the steady state of a clock is a small rectangle per second
/// rather than a whole layered window.
#[must_use]
pub fn text_bounds(scene: &Scene, theme: &Theme) -> Rect {
    let centre = scene.card_centre;
    let cap = time_cap(scene, theme);
    let rad = f64::from(STROKE_RATIO) * 0.5 * cap;
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    let adv = |s: &str| s.chars().count();
    let main = scene.text.as_str();
    let total = text_advance(adv(main), cap);
    let suffix = scene.text.suffix_str();
    let scap = scene.card_h * theme.suffix_cap;
    let stotal = text_advance(adv(suffix), scap);
    let gap = if suffix.is_empty() { 0.0 } else { theme.suffix_gap * scene.scale };
    let ox = centre.x - (total + gap + stotal) * 0.5;
    let oy = centre.y - scene.card_h * 0.5 + (scene.card_h - cap) * 0.5;
    for (i, ch) in main.chars().enumerate() {
        let Some(g) = face_data::glyph(ch) else { continue };
        let gx = ox + i as f64 * (cap * f64::from(ADVANCE) + cap * f64::from(TRACKING));
        for ln in g.lines {
            for p in ln.iter() {
                let q = rot(centre, scene.theta, Vec2::new(gx + f64::from(p.x) * cap, oy + f64::from(p.y) * cap));
                b[0] = b[0].min(q.x - rad);
                b[1] = b[1].min(q.y - rad);
                b[2] = b[2].max(q.x + rad);
                b[3] = b[3].max(q.y + rad);
            }
        }
        for d in g.dots.iter() {
            let q = rot(centre, scene.theta, Vec2::new(gx + f64::from(d.x) * cap, oy + f64::from(d.y) * cap));
            b[0] = b[0].min(q.x - f64::from(d.r) * cap);
            b[1] = b[1].min(q.y - f64::from(d.r) * cap);
            b[2] = b[2].max(q.x + f64::from(d.r) * cap);
            b[3] = b[3].max(q.y + f64::from(d.r) * cap);
        }
    }
    if !suffix.is_empty() {
        let sox = ox + total + gap;
        let soy = oy - scap * 0.12;
        for (i, ch) in suffix.chars().enumerate() {
            let Some(g) = face_data::glyph(ch) else { continue };
            let gx = sox + i as f64 * (scap * f64::from(ADVANCE) + scap * 0.16);
            for ln in g.lines {
                for p in ln.iter() {
                    let q = rot(centre, scene.theta, Vec2::new(gx + f64::from(p.x) * scap, soy + f64::from(p.y) * scap));
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
    let cap = time_cap(scene, theme);
    let rad = f64::from(STROKE_RATIO) * 0.5 * cap;
    let main = scene.text.as_str();
    let total = text_advance(main.chars().count(), cap);
    let suffix = scene.text.suffix_str();
    let scap = scene.card_h * theme.suffix_cap;
    let stotal = text_advance(suffix.chars().count(), scap);
    let gap = if suffix.is_empty() { 0.0 } else { theme.suffix_gap * scene.scale };
    let ox = centre.x - (total + gap + stotal) * 0.5;
    let oy = centre.y - scene.card_h * 0.5 + (scene.card_h - cap) * 0.5;

    for (i, ch) in main.chars().enumerate() {
        let Some(g) = face_data::glyph(ch) else { continue };
        let gx = ox + i as f64 * (cap * f64::from(ADVANCE) + cap * f64::from(TRACKING));
        for ln in g.lines {
            if ln.len() < 2 {
                continue;
            }
            for w in ln.windows(2) {
                let a = rot(centre, scene.theta, Vec2::new(gx + f64::from(w[0].x) * cap, oy + f64::from(w[0].y) * cap));
                let b = rot(centre, scene.theta, Vec2::new(gx + f64::from(w[1].x) * cap, oy + f64::from(w[1].y) * cap));
                capsule(cv, a, b, rad, theme.ink, 1.05);
            }
        }
        for d in g.dots.iter() {
            let p = rot(centre, scene.theta, Vec2::new(gx + f64::from(d.x) * cap, oy + f64::from(d.y) * cap));
            capsule(cv, p, p, f64::from(d.r) * cap * 0.95, theme.ink, 1.05);
        }
    }
    if !suffix.is_empty() {
        let sox = ox + total + gap;
        let soy = oy - scap * 0.12;
        for (i, ch) in suffix.chars().enumerate() {
            let Some(g) = face_data::glyph(ch) else { continue };
            let gx = sox + i as f64 * (scap * f64::from(ADVANCE) + scap * 0.16);
            for ln in g.lines {
                if ln.len() < 2 {
                    continue;
                }
                for w in ln.windows(2) {
                    let a = rot(centre, scene.theta, Vec2::new(gx + f64::from(w[0].x) * scap, soy + f64::from(w[0].y) * scap));
                    let b = rot(centre, scene.theta, Vec2::new(gx + f64::from(w[1].x) * scap, soy + f64::from(w[1].y) * scap));
                    capsule(cv, a, b, f64::from(STROKE_RATIO) * 0.5 * scap * 1.36, theme.accent, 1.05);
                }
            }
        }
    }
}

/// Paint a whole frame. `cv` is cleared first by the caller, since the caller knows whether the
/// frame is a repaint or a fresh size.
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
    let w = (1.7 * scene.scale).max(scene.card_h * 0.026);
    for i in 0..last {
        capsule(cv, pts[i].add(Vec2::new(1.3, 1.3)), pts[i + 1].add(Vec2::new(1.3, 1.3)), w * 1.6, theme.cord_shadow, 1.05);
    }
    for i in 0..last {
        capsule(cv, pts[i], pts[i + 1], w, theme.cord, 1.05);
    }
    for i in 0..last {
        let o = Vec2::new(-w * 0.30, -w * 0.30);
        capsule(cv, pts[i].add(o), pts[i + 1].add(o), w * 0.34, theme.cord_lit, 1.05);
    }

    // 2. the mount: a clamp against the top of the screen, and the ring the cord runs through.
    let a = scene.anchor;
    box_shadow(cv, a, Vec2::new(21.0 * scene.scale, 4.2 * scene.scale), 3.0 * scene.scale, 1.0, 2.2, 0.20);
    box_fill(
        cv,
        Vec2::new(a.x, a.y + 0.7 * scene.scale),
        Vec2::new(21.0 * scene.scale, 5.4 * scene.scale),
        3.0 * scene.scale,
        theme.mount,
        theme.mount,
        1.05,
    );
    annulus(cv, Vec2::new(a.x, a.y + 8.6 * scene.scale), 3.5 * scene.scale, 1.25 * scene.scale, theme.mount);

    // 3. the plate. Rotated, so every evaluation goes through the inverse transform; that is what
    // lets the same rounded-box distance serve a tilted object without a transformed render target.
    let c = scene.card_centre;
    let half = Vec2::new(scene.card_w * 0.5, scene.card_h * 0.5);
    let rot_extent = (half.x.abs() * scene.theta.cos().abs() + half.y.abs() * scene.theta.sin().abs()) + 1.0;
    let rot_extent_y = (half.x.abs() * scene.theta.sin().abs() + half.y.abs() * scene.theta.cos().abs()) + 1.0;
    let extent = Vec2::new(rot_extent, rot_extent_y);
    box_shadow(
        cv,
        c,
        extent,
        scene.corner,
        theme.shadow_drop * scene.scale,
        theme.shadow_blur * scene.scale,
        theme.shadow_alpha * scene.opacity,
    );

    // Body, gradient and rim, painted in the plate's frame.
    let x0 = (c.x - extent.x - 1.5).floor() as i64;
    let x1 = (c.x + extent.x + 1.5).ceil() as i64;
    let y0 = (c.y - extent.y - 1.5).floor() as i64;
    let y1 = (c.y + extent.y + 1.5).ceil() as i64;
    let (w_, h_) = (i64::from(cv.w), i64::from(cv.h));
    for y in y0.max(0)..=y1.min(h_ - 1) {
        for x in x0.max(0)..=x1.min(w_ - 1) {
            let p = centre(x as i32, y as i32);
            let l = unrot(c, scene.theta, p);
            let d = sdf_round_box(l.sub(c), half, scene.corner);
            let cov = smooth(0.55, -0.55, d);
            if cov <= 0.0005 {
                continue;
            }
            let t = ((l.y - (c.y - half.y)) / (2.0 * half.y)).clamp(0.0, 1.0);
            let lerp = |a: f64, b: f64| a + (b - a) * t;
            let (mut r, mut g, mut bch, mut al) = (
                lerp(theme.plate_top.r, theme.plate_bottom.r),
                lerp(theme.plate_top.g, theme.plate_bottom.g),
                lerp(theme.plate_top.b, theme.plate_bottom.b),
                lerp(theme.plate_top.a, theme.plate_bottom.a),
            );
            let ad = d.abs();
            if ad < 0.9 {
                (r, g, bch, al) = (theme.rim.r, theme.rim.g, theme.rim.b, theme.rim.a);
            } else if ad < 1.6 {
                if l.y < c.y - half.y + 1.6 {
                    (r, g, bch, al) = (theme.rim_top.r, theme.rim_top.g, theme.rim_top.b, theme.rim_top.a);
                } else if l.y > c.y + half.y - 2.0 {
                    r = theme.rim_bottom.r;
                    g = theme.rim_bottom.g;
                    bch = theme.rim_bottom.b;
                    al = al.max(theme.rim_bottom.a);
                }
            }
            let a2 = al * cov * scene.opacity;
            cv.blend(x as i32, y as i32, (bch * 255.0 * a2) as u16, (g * 255.0 * a2) as u16, (r * 255.0 * a2) as u16, (a2 * 255.0) as u16);
        }
        cv.touch_row(y as i32, x0.max(0) as i32, x1.min(w_ - 1) as i32);
    }

    // 4. digits, last so nothing can overprint them.
    draw_text(cv, scene, theme);
}
