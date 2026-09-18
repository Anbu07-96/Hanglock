//! Picking the plate up, moving it, letting it go. Input, not physics: this decides what the
//! solver is asked to do with the final node, and the solver decides what the cord does about it.

use super::card::Card;
use super::Rope;
use crate::vec2::Vec2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drag {
    /// Index of the held node. `None` is "nothing held", which is the only difference between a
    /// drag and free motion as far as the solver is concerned.
    pub held: Option<usize>,
    /// Where the cursor wants the held node to be, already clamped by [`Rope::reachable`].
    pub target: Vec2,
    /// Cursor velocity, px/s. Written into the node's history each step so the release carries it.
    pub vel: Vec2,
}

impl Rope {
    /// Grab the plate. Returns whether the press landed on it — the only hit test the app needs,
    /// since it is the same one the click-through gate and the hover cursor use.
    pub fn begin_drag(&mut self, at: Vec2, pad: f64) -> bool {
        let centre = self.card_centre();
        let hit = self
            .att
            .contains(centre, &self.card, self.scale, at, pad * self.scale);
        if !hit || self.nodes.len() < 2 {
            return false;
        }
        let last = self.nodes.len() - 1;
        self.drag = Drag {
            held: Some(last),
            target: self.reachable(at),
            vel: Vec2::ZERO,
        };
        self.wake();
        true
    }

    /// Update the held target with the cursor's position and velocity, px/s.
    pub fn move_drag(&mut self, at: Vec2, vel: Vec2) {
        if self.drag.held.is_none() {
            return;
        }
        let max = self.cfg.max_speed;
        self.drag.target = self.reachable(at);
        self.drag.vel = vel.limited_to(max);
    }

    /// Let go. Nothing is written on release: the gap the drag left in the node's history *is*
    /// the velocity, so the momentum the cursor had is the momentum the plate keeps. Measured on
    /// the reference model, a 425 px/s throw swings 82.9° where an undamped pendulum would
    /// describe 83.1°.
    pub fn end_drag(&mut self) {
        self.drag.held = None;
        self.drag.vel = Vec2::ZERO;
        self.wake();
    }

    /// Clamp a cursor position onto the region the cord can actually occupy: inside the reach
    /// circle, inside the sweep sector, inside the host rect, and never above the anchor.
    ///
    /// Without the reach part, pulling past the cord's length holds both ends further apart than
    /// the cord can span; the links have nowhere to go but stretch, and releasing fires the stored
    /// tension back as a snap. With it, the cord goes taut and the plate swings around the anchor —
    /// what a real cord does, and the difference between "taut" and "broken".
    #[must_use]
    pub fn reachable(&self, at: Vec2) -> Vec2 {
        let (ax, ay) = (self.anchor.x, self.anchor.y);
        let mut d = at.sub(self.anchor);
        let reach = self.hang * self.cfg.reach_ratio;
        let mut r = d.len();
        if r > reach && r > f64::EPSILON {
            d = d.scale(reach / r);
            r = reach;
        }
        if r > 1e-9 {
            let lim = self.cfg.sweep_deg.to_radians();
            let ang = d.x.atan2(d.y).clamp(-lim, lim);
            d = Vec2::new(ang.sin() * r, ang.cos() * r);
        }
        let mut p = Vec2::new(ax + d.x, ay + d.y);
        if let Some(h) = self.host {
            p.x = p.x.clamp(h.x0, h.x1);
            let floor = ay + 0.05 * self.hang;
            p.y = p.y.clamp(floor.max(h.y0), h.y1);
        }
        p
    }

    /// Whether the cursor is on the plate: the drag affordance and the click-through gate are the
    /// same question, so they are the same function.
    #[must_use]
    pub fn can_grab(&self, at: Vec2, pad: f64) -> bool {
        let centre = self.card_centre();
        self.att
            .contains(centre, &self.card, self.scale, at, pad * self.scale)
    }

    /// The plate's lean at the tail node, exposed for the debug read-out.
    #[must_use]
    pub fn lean(&self) -> f64 {
        let n = self.nodes.len();
        if n < 2 {
            return 0.0;
        }
        Card::lean(self.nodes[n - 1].pos, self.nodes[n - 2].pos)
    }
}

/// Step 3 of the frame: move the held node toward the cursor, at a finite rate.
///
/// The rate limit is not decoration. A held node that teleports hands the rest of the chain an
/// unsolvable configuration for one frame, and the visible cost is the cord going slack-long while
/// relaxation catches up. A real cursor cannot teleport either, so following at bounded speed is
/// the faithful model as well as the stable one: at human speeds the limit is never reached and
/// tracking is exact (asserted: 0 px error at 5 000 px/s).
pub(super) fn drive(rope: &mut Rope, h: f64, lim: f64) {
    let Some(i) = rope.drag.held else { return };
    let target = rope.drag.target;
    let cur = rope.nodes[i].pos;
    let step = target.sub(cur).limited_to(lim);
    rope.nodes[i].pos = cur.add(step);
    let v = rope.drag.vel;
    rope.nodes[i].set_velocity(v, h);
}
