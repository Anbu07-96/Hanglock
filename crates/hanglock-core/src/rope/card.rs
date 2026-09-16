//! The plate's attitude: a damped follower with a legibility clamp.
//!
//! The plate is not a full rigid body. Its orientation is driven by the cord's lean through a
//! second-order follower, because that is the smallest model that cannot go unstable and that
//! visibly lags the cord the way a heavy object does. A real rigid body (torque, inertia, and the
//! two-cord V-mount that replaces this clamp with geometric stiffness) stays behind this same
//! interface: [`Card::corners`] is the only thing the renderer and the sleep test need.

use super::config::{CardSpec, Posture};
use crate::vec2::Vec2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Card {
    /// Radians, clockwise-positive in canvas space. 0 is level.
    pub theta: f64,
    /// rad/s
    pub omega: f64,
    pub posture: Posture,
    /// How much of the cord's lean the plate takes up. Below 1: the plate is heavier and steadier
    /// than the last link of the cord.
    pub gain: f64,
}

impl Card {
    pub fn new(posture: Posture) -> Self {
        Self {
            theta: 0.0,
            omega: 0.0,
            posture,
            gain: 0.55,
        }
    }

    /// The cord's lean at the tail: the direction of the last link, measured from straight down.
    /// `before` is the node above `tail`; with fewer than two nodes there is no direction.
    #[must_use]
    pub fn lean(tail: Vec2, before: Vec2) -> f64 {
        (tail.x - before.x).atan2(tail.y - before.y)
    }

    /// One fixed step of the attitude follower, then the hard clamp.
    pub fn step(&mut self, lean: f64, mass_card: f64, h: f64) {
        let amax = self.posture.max_angle;
        let target = if amax > 0.0 {
            (self.gain * lean * (1.0 + 0.25 * mass_card)).clamp(-amax, amax)
        } else {
            0.0
        };
        let (k, c) = (self.posture.stiffness, self.posture.damping);
        self.omega += (k * (target - self.theta) - c * self.omega) * h;
        self.theta += self.omega * h;
        if self.theta.abs() > amax {
            self.theta = if self.theta > 0.0 { amax } else { -amax };
            // A mostly-dead stop against the posture limit: a little recoil so the clamp reads as
            // the plate hitting the bracket's travel, not as a frame being cut off.
            self.omega *= -0.15;
        }
    }

    /// Plate corners in absolute space, in the order the painter and the hit test expect.
    ///
    /// These are what the stillness test measures: the drawn silhouette, not an internal node. A
    /// sleep rule based on node velocities can keep the app awake over motion nobody can see, and
    /// can equally freeze while the shape is still changing.
    #[must_use]
    pub fn corners(&self, centre: Vec2, card: &CardSpec, scale: f64) -> [Vec2; 4] {
        let hw = card.width * 0.5 * scale;
        let hh = card.height * 0.5 * scale;
        let (cs, sn) = (self.theta.cos(), self.theta.sin());
        let local = [
            Vec2::new(-hw, -hh),
            Vec2::new(hw, -hh),
            Vec2::new(hw, hh),
            Vec2::new(-hw, hh),
        ];
        let mut out = [Vec2::ZERO; 4];
        for (o, l) in out.iter_mut().zip(local) {
            *o = Vec2::new(
                centre.x + l.x * cs - l.y * sn,
                centre.y + l.x * sn + l.y * cs,
            );
        }
        out
    }

    /// Whether `(x, y)` is on the plate, in its own rotated frame. The hit region for grab, hover,
    /// drag start and click-through — one function, so what can be clicked and what is drawn
    /// cannot disagree.
    #[must_use]
    pub fn contains(&self, centre: Vec2, card: &CardSpec, scale: f64, p: Vec2, pad: f64) -> bool {
        let hw = card.width * 0.5 * scale + pad;
        let hh = card.height * 0.5 * scale + pad;
        // Inverse-rotate the point into the plate's frame: the test is exact for the rotated rect,
        // and it is the same expression the painter uses, so the clickable shape is the drawn one.
        let (cs, sn) = (self.theta.cos(), self.theta.sin());
        let (dx, dy) = (p.x - centre.x, p.y - centre.y);
        let lx = dx * cs + dy * sn;
        let ly = -dx * sn + dy * cs;
        lx.abs() <= hw && ly.abs() <= hh
    }
}
