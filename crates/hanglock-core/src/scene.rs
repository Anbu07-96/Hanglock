//! The one thing handed to the renderer per frame.
//!
//! The solver owns live node state; the renderer gets an immutable copy. That split is not
//! ceremony: it is what makes it impossible for a draw pass to perturb the physics, and what lets a
//! frame be rendered from a hand-written scene in a test. It is copied rather than borrowed so a
//! future off-thread solver can publish across the boundary unchanged.
//!
//! Sizes here are **physical** pixels: [`Scene::new`] is the last place the display scale is
//! applied, so the painter has one coordinate space and no scale factor to remember.

use crate::clock::format::FaceText;
use crate::ids::ClockStyle;
use crate::rope::{CardSpec, Rope, RopeConfig};
use crate::vec2::Vec2;

/// Longest cord the settings will allow, so a scene needs no allocation.
pub const MAX_NODES: usize = 65;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RopePoint {
    pub pos: Vec2,
}

/// One frame's worth of everything: the cord, the plate, the digits, and how opaque it all is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scene {
    pub nodes: [Vec2; MAX_NODES],
    pub node_count: u8,
    pub anchor: Vec2,
    pub card_centre: Vec2,
    /// Radians. The plate rotates with the cord, and the digits rotate with the plate.
    pub theta: f64,
    pub card_w: f64,
    pub card_h: f64,
    pub corner: f64,
    pub bracket: f64,
    /// Where the cord disappears behind the plate, as a fraction of the plate's half height.
    pub attach_inset: f64,
    pub scale: f64,
    pub opacity: f64,
    pub text: FaceText,
    pub style: ClockStyle,
    pub dragging: bool,
    /// The solver has stopped. The painter uses this for nothing visible; the *frame clock* uses it
    /// to stop asking for frames. It travels in the scene so a debug read-out can show the truth.
    pub asleep: bool,
}

impl Scene {
    #[must_use]
    pub fn new(
        rope: &Rope,
        cfg_unused: &RopeConfig,
        text: FaceText,
        style: ClockStyle,
        opacity: f64,
    ) -> Self {
        let _ = cfg_unused;
        let s = rope.scale;
        let card = CardSpec {
            width: rope.card.width * s,
            height: rope.card.height * s,
            bracket: rope.card.bracket * s,
            corner: rope.card.corner * s,
            hang: rope.hang,
        };
        let mut nodes = [Vec2::ZERO; MAX_NODES];
        let n = rope.nodes.len().min(MAX_NODES);
        for (dst, src) in nodes[..n].iter_mut().zip(rope.nodes.iter()) {
            *dst = src.pos;
        }
        Self {
            nodes,
            node_count: n as u8,
            anchor: rope.anchor,
            card_centre: rope.card_centre(),
            theta: rope.att.theta,
            card_w: card.width,
            card_h: card.height,
            corner: card.corner,
            bracket: card.bracket,
            attach_inset: rope.card.attach_inset() * s,
            scale: s,
            opacity: opacity.clamp(0.0, 1.0),
            text,
            style,
            dragging: rope.is_dragging(),
            asleep: rope.sleeping,
        }
    }

    /// The visible cord: nodes up to where the plate covers the rest. Trimming by "is this point
    /// inside the plate" rather than by a fixed length means the joint is in the right place whether
    /// the cord is hanging straight or whipping — a bent tail covers more cord than a straight one.
    #[must_use]
    pub fn drawn_cord_len(&self) -> usize {
        let n = self.node_count as usize;
        let mut last = n.saturating_sub(1);
        while last > 1 && self.point_in_plate(self.nodes[last]) {
            last -= 1;
        }
        last
    }

    #[must_use]
    pub fn point_in_plate(&self, p: Vec2) -> bool {
        let radius = self.card_w.min(self.card_h) * 0.5;
        let dx = p.x - self.card_centre.x;
        let dy = p.y - self.card_centre.y;
        dx * dx + dy * dy <= radius * radius
    }
}
