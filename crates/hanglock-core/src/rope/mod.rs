//! The hanging object: a Verlet cord, a plate on the end of it, and the state machine that
//! decides when none of that is moving.
//!
//! ## Step order, and why it is this order
//!
//! ```text
//! pin anchor -> integrate -> drive held node -> relax -> project stretch
//!              -> (relevel, while braking) -> sector stop -> card attitude -> sleep check
//! ```
//!
//! Each edge of that sequence carries weight. Pinning first means a moved anchor drags the rope
//! *this* step rather than next, which is what makes a display change look physical instead of a
//! teleport. Relaxing before projecting lets the iterative solve do the work it is good at and
//! leaves the ceiling to handle only what it could not converge — the projection alone would fight
//! itself. [`constraints::relevel`] runs after both so it cannot be undone by a correction, and
//! before the stop so an eased return never ends up pressed against the sector limit.
//!
//! Time advances only in fixed slices ([`RopeConfig::fixed_dt`]), so behaviour is identical at
//! 60 Hz and 120 Hz — asserted exactly, `tests/rope.rs`.

pub mod card;
pub mod config;
pub mod constraints;
pub mod drag;
pub mod node;

pub use card::Card;
pub use config::{CardSpec, Posture, RopeConfig};
pub use node::Node;

use crate::placement::Rect;
use crate::vec2::Vec2;

/// One cord, one plate. Owned by exactly one thing: the overlay's frame clock.
pub struct Rope {
    pub cfg: RopeConfig,
    pub card: CardSpec,
    pub anchor: Vec2,
    /// Display scale. The model stays in logical px; this is applied to gravity and to lengths at
    /// construction so a 150 % display gets the same *physical* rope, not a slower one.
    pub scale: f64,
    pub nodes: Vec<Node>,
    /// Anchor-to-centre-of-mass rest length, physical px.
    pub hang: f64,
    pub seg_len: f64,
    pub att: Card,
    pub host: Option<Rect>,
    pub sleeping: bool,
    pub braking: bool,
    acc: f64,
    still: u64,
    nstep: u64,
    reference: Option<[Vec2; 4]>,
    pub drag: drag::Drag,
}

impl Rope {
    #[must_use]
    pub fn new(
        cfg: RopeConfig,
        card: CardSpec,
        posture: Posture,
        anchor: Vec2,
        scale: f64,
    ) -> Self {
        let mut s = Self {
            cfg,
            card,
            anchor,
            scale,
            nodes: Vec::new(),
            hang: card.hang * scale,
            seg_len: 1.0,
            att: Card::new(posture),
            host: None,
            sleeping: false,
            braking: false,
            acc: 0.0,
            still: 0,
            nstep: 0,
            reference: None,
            drag: drag::Drag::default(),
        };
        s.seg_len = (s.hang / cfg.segments.max(1) as f64).max(0.5);
        s.reset(cfg.initial_angle);
        s
    }

    /// Rebuild the chain hanging at `angle` from vertical, motionless.
    pub fn reset(&mut self, angle: f64) {
        let n = self.cfg.segments + 1;
        let d = Vec2::new(angle.sin(), angle.cos());
        self.nodes = (0..n)
            .map(|i| {
                let off = d.scale(i as f64 * self.seg_len);
                let pos = self.anchor.add(off);
                if i == 0 {
                    Node::PINNED
                } else if i + 1 == n {
                    Node::free(pos, self.cfg.mass_card)
                } else {
                    Node::free(pos, self.cfg.cord_mass)
                }
            })
            .collect();
        self.acc = 0.0;
        self.still = 0;
        self.nstep = 0;
        self.reference = None;
        self.braking = false;
        self.att = Card::new(self.att.posture);
        self.drag = drag::Drag::default();
        self.sleeping = false;
    }

    /// For a reduced-motion user: hang it at rest instead of swinging it into place. Same object,
    /// no entrance.
    pub fn reset_to_hanging(&mut self) {
        self.reset(0.0);
    }

    /// The last node is the plate's centre of mass; the joint the cord visually ends at sits
    /// [`CardSpec::attach_inset`] above it, along the plate's own up direction.
    #[must_use]
    pub fn card_centre(&self) -> Vec2 {
        self.nodes.last().map_or(self.anchor, |n| n.pos)
    }

    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.drag.held.is_some()
    }

    /// Longest link over its rest length. A test quantity, and a debug one: it is the number that
    /// turns "never stretches unrealistically" from an opinion about a screenshot into an assert.
    #[must_use]
    pub fn max_stretch(&self) -> f64 {
        let mut w = 1.0_f64;
        for i in 0..self.nodes.len().saturating_sub(1) {
            let d = self.nodes[i].pos.dist(self.nodes[i + 1].pos) / self.seg_len;
            if d > w {
                w = d;
            }
        }
        w
    }

    /// The plate's corners right now — what the painter draws and what stillness is measured on.
    #[must_use]
    pub fn corners(&self) -> [Vec2; 4] {
        self.att.corners(self.card_centre(), &self.card, self.scale)
    }

    /// Drop the accumulated time deficit without waking. A machine that has been asleep for nine
    /// hours has "missed" hundreds of thousands of frames; paying them back would look like the
    /// clock teleported, and not paying them back is correct because nothing moved while it slept.
    pub fn acc_reset(&mut self) {
        self.acc = 0.0;
    }

    /// Wake the solver so the next [`Rope::step`] does work. Every input path calls this, and
    /// nothing else does.
    pub fn wake(&mut self) {
        self.sleeping = false;
        self.still = 0;
        self.reference = None;
        // The brake must not survive into a new drag: it is a settle aid, and a drag is the
        // opposite of settling. Leaving it on makes the cord feel sticky while the cursor is on it.
        self.braking = false;
    }

    /// Move the anchor (a reposition, a monitor change, a DPI change) and wake, without discarding
    /// motion. A settled object that is *moved* should swing; one that is *rescaled* should too.
    pub fn set_anchor(&mut self, anchor: Vec2) {
        if anchor == self.anchor {
            return;
        }
        self.anchor = anchor;
        self.wake();
    }

    /// Re-fit to a new scale or hang length, keeping the current motion so the change swings into
    /// place rather than snapping.
    pub fn refit(&mut self, scale: f64, hang_logical: f64) {
        let scale = scale.max(0.25);
        let hang = (hang_logical * scale).max(24.0);
        let seg = (hang / self.cfg.segments.max(1) as f64).max(0.5);
        let rebuild = (seg - self.seg_len).abs() > 1e-9;
        // Growth is applied to the motion as well as to the geometry: the same swing at 200 % of
        // the size *is* twice the pixel velocity, and `reset` would hand the cord a zero velocity
        // and let it fall dead. Read before the rebuild, because the rebuild overwrites it.
        let growth = if self.seg_len > 1e-9 {
            seg / self.seg_len
        } else {
            1.0
        };
        let carried: Vec<Vec2> = if rebuild {
            self.nodes.iter().map(|n| n.displacement().scale(growth)).collect()
        } else {
            Vec::new()
        };
        let (theta, omega) = (self.att.theta, self.att.omega);
        self.scale = scale;
        self.hang = hang;
        self.seg_len = seg;
        if rebuild {
            let angle = Card::lean(self.card_centre(), self.nodes[1].pos);
            self.reset(angle);
            for (node, delta) in self.nodes.iter_mut().zip(&carried) {
                node.prev = node.pos.sub(*delta);
            }
            self.att.theta = theta;
            self.att.omega = omega;
        }
        self.wake();
    }

    /// Advance by real elapsed time. Returns whether any physics happened, which is how the caller
    /// decides whether to publish a frame at all.
    ///
    /// The accumulator clamp matters more than it looks: without it, a 3 s stall (a modal loop, a
    /// resume from sleep, a breakpoint) becomes 720 catch-up steps in one call, and the object
    /// teleports. With it, the pause costs one clamped frame.
    pub fn step(&mut self, dt: f64) -> bool {
        if self.sleeping || dt <= 0.0 || self.nodes.len() < 2 {
            return false;
        }
        self.acc = (self.acc + dt).min(self.cfg.max_frame);
        let h = self.cfg.fixed_dt;
        let mut taken = 0;
        while self.acc >= h {
            self.advance(h);
            self.sleep_check();
            self.acc -= h;
            taken += 1;
            if self.sleeping {
                break;
            }
        }
        taken > 0
    }

    fn advance(&mut self, dt: f64) {
        let cfg = self.cfg;
        let gravity = cfg.gravity * self.scale;
        let lim = cfg.max_speed * dt;
        let dv = cfg.friction_acc * self.scale * dt * dt;

        // 1. pin
        self.nodes[0].pos = self.anchor;
        self.nodes[0].prev = self.anchor;

        // 2. integrate
        for i in 1..self.nodes.len() {
            if self.drag.held == Some(i) {
                continue;
            }
            if self.nodes[i].inv_mass <= 0.0 {
                continue;
            }
            let cur = self.nodes[i].pos;
            let prev = self.nodes[i].prev;
            let brake = if self.braking { cfg.brake_step } else { 1.0 };
            let mut vel = cur.sub(prev).scale(cfg.damping * brake);
            // The friction bite and the speed cap are measured on the DAMPED velocity — the
            // reference takes hypot(vx, vy) after scaling; measuring the undamped delta
            // divides by a slightly larger denominator and shrinks every link a hair more.
            let moved = vel.len();
            if moved > 1e-12 {
                let shrink = (moved - dv).max(0.0) / moved;
                vel = vel.scale(shrink);
            }
            if moved > lim {
                vel = vel.scale(lim / moved);
            }
            self.nodes[i].prev = cur;
            self.nodes[i].pos = Vec2::new(cur.x + vel.x, cur.y + vel.y + gravity * dt * dt);
        }

        // 3. drive the held node (see drag.rs)
        drag::drive(self, dt, lim);

        // 4/5. satisfy the cord, then guarantee the ceiling
        let held = self.drag.held;
        let rest = self.seg_len;
        constraints::relax(&mut self.nodes, rest, &self.cfg, held);
        constraints::project_stretch(&mut self.nodes, rest, &self.cfg, held);

        // 6. while braking, ease back under the anchor
        if self.braking {
            let ax = self.anchor.x;
            constraints::relevel(&mut self.nodes, ax, cfg.relevel);
            self.att.theta *= 1.0 - 8.0 * dt;
        }

        // 7/8. sector limit, then the plate's own attitude
        constraints::apply_stop(&mut self.nodes, self.anchor, &self.cfg);
        let (tail, before) = {
            let n = self.nodes.len();
            (self.nodes[n - 1].pos, self.nodes[n - 2].pos)
        };
        let lean = Card::lean(tail, before);
        self.att.step(lean, cfg.mass_card, dt);
    }

    /// Stillness, measured on the drawn silhouette across a window of fixed steps.
    ///
    /// Two obvious versions of this are wrong. A per-step speed test with a threshold under
    /// `relax_tol` asks relaxation to converge finer than its own tolerance, so a settled cord
    /// never sleeps (measured on the reference model: 11 s awake, and counting). A test on the
    /// plate's centre alone sleeps while the cord above it is still creeping. So: compare the four
    /// plate corners against a sample from a full window ago, and sleep only when nothing that is
    /// drawn has moved more than a quarter of a pixel for `settle_time` seconds.
    fn sleep_check(&mut self) {
        if self.drag.held.is_some() {
            self.still = 0;
            self.reference = None;
            return;
        }
        self.nstep += 1;
        let w = self.cfg.still_window.max(2);
        if self.nstep % w != 0 {
            return;
        }
        let now = self.corners();
        let Some(prev) = self.reference else {
            // No sample to compare against — which is the state immediately after a release, since
            // the drag path clears the reference. Returning a verdict here would read "moved 0 px"
            // and engage the settle brake for a whole window at the start of every throw, at
            // 0.90 per step. Only compare once there is something to compare to.
            self.reference = Some(now);
            return;
        };
        let mut moved = 0.0_f64;
        for i in 0..4 {
            let d = now[i].dist(prev[i]);
            if d > moved {
                moved = d;
            }
        }
        self.reference = Some(now);
        let s = self.scale;
        if moved > self.cfg.brake_off_move * s {
            self.braking = false;
        } else if moved < self.cfg.brake_on_move * s {
            self.braking = true;
        }
        if moved > self.cfg.sleep_max_move * s {
            self.still = 0;
            return;
        }
        self.still += w;
        if self.still as f64 * self.cfg.fixed_dt >= self.cfg.settle_time {
            self.sleeping = true;
        }
    }
}

impl Default for Rope {
    fn default() -> Self {
        Self::new(
            RopeConfig::default(),
            CardSpec::default(),
            Posture::PLATE,
            Vec2::ZERO,
            1.0,
        )
    }
}
