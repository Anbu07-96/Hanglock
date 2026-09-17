//! The hang point, as the two numbers the settings file holds — and what dragging it means.
//!
//! A position is never stored as a screen coordinate. Coordinates go stale in three ways this app
//! meets in ordinary use: the user drags a window onto a second display, the display's scale factor
//! changes under it (`WM_DPICHANGED`, which also resizes the clock), or the monitor order changes
//! when something is unplugged. A *ratio across the usable width* and a *drop below the hang line*
//! stay meaningful in all three, because both are measured from things that move with the display.
//!
//! So the model's job while the user drags with Alt held is to turn a device-pixel delta into those
//! two numbers, and to keep the result in the band where the clock is still reachable — reachable
//! meaning on a visible part of a monitor it was placed on, with its swing on screen too. The
//! "reachable" test is not duplicated here: it asks [`place`], the same function the overlay trusts
//! to put the window on screen, so the two cannot disagree about where the edge is.

use crate::placement::{hang_line, swept_box, Monitor};
use crate::rope::config::RopeConfig;
use crate::settings::{limits, Settings};
use crate::vec2::Vec2;

/// Where the clock hangs from: a ratio across the usable width, and a drop below the hang line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Anchor {
    /// 0 = the left edge of the usable width, 1 = the right edge.
    pub ratio: f64,
    /// Logical px below the hang line. Never negative, and small values are one position:
    /// [`place`] keeps the hang point at least [`ANCHOR_INSET`] from the top of the display so the
    /// clamp drawn there is not cut off, which on a display without a top-docked taskbar means the
    /// first 14 px of drop are already spent.
    pub drop: f64,
}

impl Anchor {
    /// The pair `hang from the middle of the top edge` — what a first run uses, and what
    /// [`crate::settings::Settings::default`] holds.
    pub const DEFAULT: Self = Self {
        ratio: 0.5,
        drop: 0.0,
    };

    /// Clamp into the ranges the settings file documents, so a caller can build an `Anchor` from
    /// anything — a parsed file, a drag, a number a user typed — without re-deriving the limits.
    ///
    /// Both are rounded to four decimals first, which is the precision [`crate::settings`] writes at.
    /// A position the writer cannot express is a position that moves when the file is saved: the user
    /// would see the clock settle a fraction away from where they left it, and the round-trip test
    /// below would be a diff rather than an equality.
    #[must_use]
    pub fn clamped(ratio: f64, drop: f64) -> Self {
        Self {
            ratio: whole(ratio, limits::ANCHOR_RATIO),
            drop: whole(drop, limits::ANCHOR_DROP),
        }
    }

    /// Both numbers as the settings document wants them.
    #[must_use]
    pub fn from_overlay(o: &crate::settings::Overlay) -> Self {
        Self::clamped(o.anchor_ratio, o.anchor_drop)
    }
}

/// The writer's unit: four decimals, so 0.0001 of the width or of a pixel. Rounding in the type that
/// the file is written from means the number on screen, the number in the file, and the number a later
/// build reads are the same number.
fn whole(v: f64, range: (f64, f64)) -> f64 {
    let inside = v.clamp(range.0, range.1);
    (inside * 10_000.0).round() / 10_000.0
}

/// Where an Alt-drag leaves the hang point.
///
/// The inputs are where the hang point *is* and where the cursor went, both in device px on the
/// display that holds the clock, and the document whose cord and margins decide how big a swing is
/// legal. The output is the pair of numbers the settings file holds, so a gesture and a hand-edited
/// file cannot disagree about what a position means.
///
/// Measuring from the position on screen rather than accumulating onto the stored pair is deliberate:
/// [`crate::placement::place`] pulls a frame that would hang off the top down onto the display, so the
/// stored drop and the visible anchor legitimately differ by the clamp's inset, and a gesture worked
/// out from the stored number would either skip that distance or refuse to move at all.
///
/// Two bounds are applied, and both are the file's own: the ratio to the usable width, and the drop to
/// the band in which the whole swept box — the plate at full stretch, not just the anchor — ends on
/// this display. Clamping the drop by *fitting the box* rather than by fitting the anchor is the
/// deliberate half: a hang point the user cannot undo is a clock that has become an obstacle, and the
/// way out of it is a tray menu item they have to find while the object answers no clicks.
#[must_use]
pub fn dragged(
    m: &Monitor,
    s: &Settings,
    cfg: &RopeConfig,
    anchor_device: Vec2,
    delta_device: Vec2,
) -> Anchor {
    let o = &s.overlay;
    let respect = o.respect_taskbar;
    let scale = m.scale.max(0.01);
    let usable = if respect { m.work_logical() } else { m.bounds_logical() };
    let bounds = m.bounds_logical();
    let want = Vec2::new(
        anchor_device.x + delta_device.x,
        anchor_device.y + delta_device.y,
    );
    let ratio = (want.x / scale - usable.x0) / usable.w().max(1.0);
    let box_logical = swept_box(cfg, &s.card(), o.hang, o.margin, o.scale);
    let line = hang_line(m, respect);
    let below = want.y / scale - line;
    // Where the swing's own bottom edge touches the bottom of the display. `max(0)` because a display
    // too short for the box has no drop that fits, and "hung from the line" is the least bad answer;
    // it is also the only answer `place` could honour there.
    let ceiling = (bounds.y1 - box_logical.y1 - line).max(0.0);
    Anchor::clamped(ratio, below.min(ceiling))
}
