//! Every tunable in one place. If a number changes the feel, it changes here and nowhere else.
//!
//! The units are logical pixels and seconds, matching [`crate::vec2::Vec2`]. The thresholds that
//! look like distances (`sleep_max_move`, `brake_on_move`, `brake_off_move`) are **per
//! still window**, not per second: they are compared against how far a corner travelled between
//! samples, so [`RopeConfig::still_window`] is what turns them into speeds. Change one and the
//! others move with it — that coupling is the reason they are documented together here.

use crate::vec2::Vec2;

/// The rope, the plate and the frame clock, as data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RopeConfig {
    /// Links in the chain; nodes are one more. 16 is enough to read as a continuous cord at the
    /// lengths Hanglock hangs (110-260 logical px) and cheap enough to run 240 times a second.
    pub segments: usize,
    /// Downward acceleration, px/s². Tuned so a 150 px cord has a ~1.6 s period: slow enough to
    /// read as a heavy object, quick enough that a swing resolves before you look away.
    pub gravity: f64,
    /// Viscous carry per step. Small on purpose: air drag alone cannot settle a hanging object in
    /// a bounded time *and* let a throw carry, so the settling is `friction_acc`'s job.
    pub damping: f64,
    /// Dry (Coulomb) deceleration in px/s², applied to each node's carried displacement. This is
    /// the term that makes "swings freely, settles quickly" one model instead of a contradiction.
    pub friction_acc: f64,
    /// Physics advances in fixed slices, whatever the display does.
    pub fixed_dt: f64,
    /// Accumulator clamp: a stall or a resume-from-sleep cannot fire a catch-up burst.
    pub max_frame: f64,
    /// Hard one-sided ceiling on link length. 2 % of a ~9 px link is 0.2 px: the point is not a
    /// round number, it is "no longer visible as stretch".
    pub max_stretch: f64,
    /// Relaxation exits once no correction in a whole pass exceeds this (px).
    pub relax_tol: f64,
    /// Relaxation pass budget. Corrections travel about one link per pass, so this must exceed
    /// `segments` or a yank leaves the far end unaware; 8x nodes converges without ever costing
    /// the budget when the early-out fires, which is most passes.
    pub relax_cap: usize,
    /// Per-node speed rail (px/s). Unreachable by hand; it makes divergence impossible.
    pub max_speed: f64,
    /// How far the held point may be dragged, as a fraction of the cord's rest length. Below 1.0
    /// so a taut cord keeps a little slack and never reads as a rigid bar.
    pub reach_ratio: f64,
    /// Half-angle of the sector the plate may occupy — while swinging too, not only while dragged.
    pub sweep_deg: f64,
    /// Restitution at that sector limit: a thunk, not a bounce.
    pub stop_rest: f64,
    /// Mass of the plate, at the cord's end. The bob.
    pub mass_card: f64,
    /// Mass of one interior cord node. A cord nearly massless next to the plate is what lets a
    /// throw keep its energy: at parity the pointer's work goes into swinging the rope instead of
    /// the clock, and a released clock barely moves (measured: 8 % of pendulum energy at
    /// `mass_card / cord_mass` ≈ 3).
    pub cord_mass: f64,
    /// px any plate corner may travel per window and still count as unchanged.
    pub sleep_max_move: f64,
    /// Fixed steps between stillness samples. 24 at 240 Hz is 0.1 s.
    pub still_window: u64,
    /// Seconds of unchanged picture before the solver stops.
    pub settle_time: f64,
    /// Below this px/window the settle brake engages. 1.6 px/0.1 s ≈ 16 px/s, comfortably under
    /// any throw worth feeling.
    pub brake_on_move: f64,
    /// Above this it lets go again. The gap between the two is deliberate hysteresis: a brake
    /// that chatters at one threshold looks like a stutter.
    pub brake_off_move: f64,
    /// Carried-displacement multiplier while braking.
    pub brake_step: f64,
    /// Per-step ease of the hanging line back under the anchor, while braking. A solver with
    /// friction can park a few degrees off vertical, and a clock frozen crooked reads as broken.
    pub relevel: f64,
    /// Release offset at first appearance, rad from vertical: it swings into place rather than
    /// being spawned at rest.
    pub initial_angle: f64,
}

/// The plate the cord carries, in logical px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardSpec {
    pub width: f64,
    pub height: f64,
    /// Length of the bracket between the last cord node and the plate's top edge. The cord's final
    /// node is the plate's **centre of mass**, so the visible joint sits this far above it.
    pub bracket: f64,
    pub corner: f64,
    /// Default hang: anchor to centre of mass, px.
    pub hang: f64,
}

/// How much the plate is allowed to tilt. `max_angle` is a legibility limit, not a physical one:
/// the numbers must stay readable mid-swing, so this is where that product requirement enters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Posture {
    pub stiffness: f64,
    pub damping: f64,
    pub max_angle: f64,
}

impl Posture {
    pub const NATURAL: Posture = Posture {
        stiffness: 120.0,
        damping: 9.0,
        max_angle: 0.4538,
    };
    pub const PLATE: Posture = Posture {
        stiffness: 420.0,
        damping: 24.0,
        max_angle: 0.1571,
    };
    pub const MOUNTED: Posture = Posture {
        stiffness: 1400.0,
        damping: 46.0,
        max_angle: 0.0436,
    };
    pub const LOCKED: Posture = Posture {
        stiffness: 4000.0,
        damping: 90.0,
        max_angle: 0.0,
    };

    #[must_use]
    pub fn from_id(id: &str) -> Self {
        match id {
            "natural" => Self::NATURAL,
            "mounted" => Self::MOUNTED,
            "locked" => Self::LOCKED,
            _ => Self::PLATE,
        }
    }
}

impl Default for RopeConfig {
    fn default() -> Self {
        Self {
            segments: 16,
            gravity: 2400.0,
            damping: 0.9997,
            friction_acc: 215.0,
            fixed_dt: 1.0 / 240.0,
            max_frame: 0.10,
            max_stretch: 1.02,
            relax_tol: 0.02,
            relax_cap: 8,
            max_speed: 5200.0,
            reach_ratio: 0.985,
            sweep_deg: 52.0,
            stop_rest: 0.22,
            mass_card: 1.0,
            cord_mass: 0.020,
            sleep_max_move: 0.25,
            still_window: 24,
            settle_time: 0.30,
            brake_on_move: 1.6,
            brake_off_move: 3.2,
            brake_step: 0.90,
            relevel: 0.014,
            initial_angle: 0.30,
        }
    }
}

impl Default for CardSpec {
    fn default() -> Self {
        Self {
            width: 252.0,
            height: 96.0,
            bracket: 9.0,
            corner: 17.0,
            hang: 150.0,
        }
    }
}

impl CardSpec {
    /// Where the centre of mass sits relative to the plate's centre: nowhere — the final cord node
    /// *is* the centre of mass, and the plate is drawn around it. This returns the distance from
    /// that node to the plate's top edge, which is where the cord visibly disappears behind it.
    #[must_use]
    pub fn attach_inset(&self) -> f64 {
        self.height * 0.5 + self.bracket
    }

    /// The plate's corners, in local (unrotated) space, for hit testing.
    #[must_use]
    pub fn half(&self) -> Vec2 {
        Vec2::new(self.width * 0.5, self.height * 0.5)
    }

    /// The cord's rest length for a given hang, in logical px.
    #[must_use]
    pub fn rest_len(&self, segments: usize, hang: f64) -> f64 {
        (hang / segments as f64).max(0.5)
    }
}
