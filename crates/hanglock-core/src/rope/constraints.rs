//! Constraint solving: relax toward rest length, then project anything still too long, then apply
//! the sector stop. Three passes with three different jobs, in an order that matters.
//!
//! All of them share corrections by inverse mass, so a heavy plate barely moves when a light cord
//! is yanked and the anchor does not move at all. That single mechanism replaces the special cases
//! a force-based solver would need.

use super::config::RopeConfig;
use super::node::Node;
use crate::vec2::Vec2;

/// The inverse mass the solver sees for a node: a held node is immovable, exactly like the anchor,
/// which is what lets the cursor drive the plate without the solve fighting it.
#[must_use]
pub fn effective_inv_mass(nodes: &[Node], i: usize, held: Option<usize>) -> f64 {
    if held == Some(i) {
        0.0
    } else {
        nodes[i].inv_mass
    }
}

/// Gauss-Seidel relaxation: pull every link toward its rest length.
///
/// Later passes see earlier corrections, so convergence is fast; the loop exits on the largest
/// correction in a pass rather than running a fixed budget. Written as a loop with an explicit
/// break because the exit condition is the point — a filtered iterator would keep iterating and
/// quietly run the whole budget every step, which is a real bug this design has to avoid.
///
/// - Returns: the largest correction applied in the last pass, so the caller can assert
///   convergence in tests.
pub fn relax(nodes: &mut [Node], rest_len: f64, cfg: &RopeConfig, held: Option<usize>) -> f64 {
    let mut largest = f64::INFINITY;
    let links = nodes.len() - 1;
    for _ in 0..cfg.relax_cap.saturating_mul(links.max(1)) {
        let mut worst = 0.0_f64;
        for i in 0..links {
            let ia = effective_inv_mass(nodes, i, held);
            let ib = effective_inv_mass(nodes, i + 1, held);
            let total = ia + ib;
            if total <= 0.0 {
                continue;
            }
            let pa = nodes[i].pos;
            let pb = nodes[i + 1].pos;
            let delta = pb.sub(pa);
            let dist = delta.len();
            if dist <= f64::EPSILON {
                continue;
            }
            let ratio = (dist - rest_len) / dist / total;
            let corr = delta.scale(ratio);
            let ca = corr.scale(ia);
            let cb = corr.scale(ib);
            nodes[i].pos = pa.add(ca);
            nodes[i + 1].pos = pb.sub(cb);
            let moved = ca.len().max(cb.len());
            if moved > worst {
                worst = moved;
            }
        }
        largest = worst;
        if worst < cfg.relax_tol {
            break;
        }
    }
    largest
}

/// The hard guarantee behind "does not visibly stretch".
///
/// Relaxation targets the rest length and is iterative, so a violent frame can leave a link long.
/// This enforces the ceiling as a **one-sided** constraint: links inside it are untouched, links
/// over it are pulled back, and — critically — the correction is *shared* between the two ends
/// rather than snapping the offending node onto the limit. Snapping oscillates instead of
/// converging, because with the chain pinned at both ends each sweep undoes the last one's work.
pub fn project_stretch(nodes: &mut [Node], rest_len: f64, cfg: &RopeConfig, held: Option<usize>) {
    let limit = rest_len * cfg.max_stretch;
    let links = nodes.len() - 1;
    for _ in 0..40 {
        let mut moved = false;
        for i in 0..links {
            let ia = effective_inv_mass(nodes, i, held);
            let ib = effective_inv_mass(nodes, i + 1, held);
            let total = ia + ib;
            if total <= 0.0 {
                continue;
            }
            let a = nodes[i].pos;
            let b = nodes[i + 1].pos;
            let d = b.sub(a);
            let dist = d.len();
            if dist <= limit || dist <= f64::EPSILON {
                continue;
            }
            let corr = d.scale((dist - limit) / dist / total);
            nodes[i].pos = a.add(corr.scale(ia));
            nodes[i + 1].pos = b.sub(corr.scale(ib));
            moved = true;
        }
        if !moved {
            break;
        }
    }
}

/// The sector stop: the plate cannot leave the sweep, swinging or dragged.
///
/// Not a nicety. An unbounded clock on a 150 px cord reaches ~100° of swing, which puts the plate
/// off the monitor and behind the taskbar; and a fixed *sector*-shaped swept box is what lets the
/// overlay window be sized once at startup instead of following the object. The cord's length is
/// already handled by inextensibility, so this clamps the angle only, keeps the tangential part of
/// the implied velocity, and takes a bite out of it — hitting the stop reads as a thunk.
pub fn apply_stop(nodes: &mut [Node], anchor: Vec2, cfg: &RopeConfig) {
    let last = nodes.len() - 1;
    let (ax, ay) = (anchor.x, anchor.y);
    let p = nodes[last].pos;
    let ang = (p.x - ax).atan2(p.y - ay);
    let lim = cfg.sweep_deg.to_radians();
    if ang >= -lim && ang <= lim {
        return;
    }
    let side = if ang > 0.0 { 1.0 } else { -1.0 };
    let r = Vec2::new(p.x - ax, p.y - ay).len();
    let (ca, sa) = (lim.cos(), lim.sin());
    let pinned = Vec2::new(ax + side * r * sa, ay + r * ca);
    let q = nodes[last].prev;
    // Reflect the angular component of the carried displacement, keep the rest.
    let va = (p.x - q.x) * side * ca - (p.y - q.y) * sa;
    nodes[last].pos = pinned;
    nodes[last].prev = Vec2::new(
        pinned.x - ((p.x - q.x) - va * side * ca * (1.0 + cfg.stop_rest)),
        pinned.y - ((p.y - q.y) + va * sa * (1.0 + cfg.stop_rest)),
    );
}

/// While the settle brake is on, ease the hanging line back under the anchor. See
/// [`RopeConfig::relevel`].
pub fn relevel(nodes: &mut [Node], anchor_x: f64, amount: f64) {
    for i in 1..nodes.len() {
        nodes[i].pos.x += (anchor_x - nodes[i].pos.x) * amount;
    }
}
