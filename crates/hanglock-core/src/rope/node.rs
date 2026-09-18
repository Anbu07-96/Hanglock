//! One Verlet node.

use crate::vec2::Vec2;

/// A rope node: position, where it was, and how movable it is.
///
/// There is no velocity field. Velocity is the gap between [`Node::pos`] and [`Node::prev`],
/// which is the whole reason a released drag keeps moving: letting go simply stops writing the
/// position, and the gap the drag left behind *is* the velocity.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Node {
    pub pos: Vec2,
    pub prev: Vec2,
    /// Reciprocal mass. Zero pins the node: corrections scaled by zero move it not at all, which
    /// is how the anchor stays put without a special case in the solver — and how a dragged node
    /// becomes immovable for the duration of the drag.
    pub inv_mass: f64,
}

impl Node {
    /// A pinned node: the anchor.
    pub const PINNED: Node = Node {
        pos: Vec2::ZERO,
        prev: Vec2::ZERO,
        inv_mass: 0.0,
    };

    #[must_use]
    pub fn free(pos: Vec2, mass: f64) -> Self {
        Self {
            pos,
            prev: pos,
            inv_mass: 1.0 / mass.max(1e-6),
        }
    }

    #[must_use]
    pub fn displacement(&self) -> Vec2 {
        self.pos.sub(self.prev)
    }

    /// Sets the implied velocity for a given step length. Used by the drag, so the node carries the
    /// cursor's motion into the frame it is released.
    pub fn set_velocity(&mut self, v: Vec2, h: f64) {
        self.prev = Vec2::new(self.pos.x - v.x * h, self.pos.y - v.y * h);
    }
}
