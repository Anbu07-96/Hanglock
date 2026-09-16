//! Where the overlay goes, in and out of a display.
//!
//! Kept free of every platform type so the anchoring rules can be tested exactly, with no monitor
//! attached: negative-origin secondaries, mixed per-monitor scale, a top-docked taskbar, an overlay
//! bigger than the screen. That list is the whole reason a first-week Windows user keeps the clock
//! on screen.
//!
//! Two coordinate spaces are in play and the boundary is explicit. Monitors arrive in **physical
//! pixels** (that is what Win32 reports). Everything computed between the box and the anchor is done
//! in **logical pixels** — because the rope's length, the plate and the margins are all authored in
//! logical units — and converted back once, at the end. Converting per-quantity instead of once is
//! how the rounding drift accumulates into a visibly detached cord at 150 %.

use crate::rope::config::{CardSpec, RopeConfig};
use crate::vec2::Vec2;

/// A rectangle in one coordinate space. Half-open arithmetic on purpose: `w()` is `x1 - x0`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    #[must_use]
    pub const fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }
    #[must_use]
    pub fn w(&self) -> f64 {
        self.x1 - self.x0
    }
    #[must_use]
    pub fn h(&self) -> f64 {
        self.y1 - self.y0
    }
    #[must_use]
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.x0 && p.x < self.x1 && p.y >= self.y0 && p.y < self.y1
    }
    #[must_use]
    pub fn scaled(&self, k: f64) -> Self {
        Self::new(self.x0 * k, self.y0 * k, self.x1 * k, self.y1 * k)
    }
    #[must_use]
    pub fn shifted(&self, d: Vec2) -> Self {
        Self::new(self.x0 + d.x, self.y0 + d.y, self.x1 + d.x, self.y1 + d.y)
    }
}

/// One display, as the platform layer reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Monitor {
    pub index: u32,
    /// Full extent, physical px. Secondary displays left of the primary have negative origins.
    pub bounds: Rect,
    /// `bounds` minus taskbar and any docked appbar, physical px.
    pub work: Rect,
    pub scale: f64,
    /// True when the taskbar is docked to the *top*, in which case hanging over it means the
    /// anchor has to move down instead of the clock being drawn behind the taskbar.
    pub taskbar_top: bool,
    pub taskbar_auto_hidden: bool,
    /// The display the desktop is on. Used only as the fallback when a saved index is stale.
    pub primary: bool,
}

impl Monitor {
    #[must_use]
    pub fn bounds_logical(&self) -> Rect {
        self.bounds.scaled(1.0 / self.scale.max(0.01))
    }
    #[must_use]
    pub fn work_logical(&self) -> Rect {
        self.work.scaled(1.0 / self.scale.max(0.01))
    }
}

/// What the window layer needs: the physical frame to create or move, and where the anchor sits
/// inside it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Placement {
    pub frame: Rect,
    /// The hang point, in physical px, in *frame-local* coordinates (so the painter's origin is
    /// the frame's top-left and the anchor is a constant offset within it).
    pub anchor: Vec2,
    pub size_logical: Vec2,
    pub scale: f64,
    /// True when the swept box was larger than the display: the sector is then clipped, and the
    /// reach clamp in the solver is what keeps the plate on screen.
    pub clipped: bool,
}

/// The area the object can occupy while swinging, in logical px, relative to the anchor.
///
/// The window is this box — not a circle, and not a guess — because the sector stop in
/// [`RopeConfig::sweep_deg`] bounds the swing. Sizing to the swept area is what keeps a present
/// small enough to be cheap, and the coupling is intentional: widen the sweep and the window grows
/// with it, which is the honest cost of a livelier swing.
#[must_use]
pub fn swept_box(cfg: &RopeConfig, card: &CardSpec, hang: f64, margin: f64, scale: f64) -> Rect {
    let s = scale.max(0.01);
    let r = hang * s;
    let half_w = r * cfg.sweep_deg.to_radians().sin() + card.width * 0.5 * s + margin;
    let top = card.bracket * s + 14.0 * s + margin;
    let bottom = r + card.height * 0.5 * s + margin * 2.0;
    Rect::new(-half_w, -top, half_w, bottom)
}

/// Place the overlay on `m`, hanging at `anchor_ratio` along the top of the usable width.
///
/// `anchor_ratio` (0..=1) rather than an absolute x: a ratio survives the resolution and scale
/// changes that happen while the app is running, which is what makes a re-anchor after
/// `WM_DPICHANGED` land where the user put it instead of at the left edge.
#[must_use]
pub fn place(
    m: &Monitor,
    cfg: &RopeConfig,
    card: &CardSpec,
    hang: f64,
    scale_mult: f64,
    anchor_ratio: f64,
    margin: f64,
    respect_taskbar: bool,
) -> Placement {
    let s = m.scale.max(0.01);
    let bounds = m.bounds_logical();
    let work = m.work_logical();
    let box_logical = swept_box(cfg, card, hang, margin, scale_mult);
    let size = Vec2::new(box_logical.w(), box_logical.h());

    // The hang line: the physical top of the display, unless a top-docked taskbar should be
    // avoided, in which case the top of the work area.
    let hang_y = if respect_taskbar && m.taskbar_top && !m.taskbar_auto_hidden {
        work.y0.max(bounds.y0)
    } else {
        bounds.y0
    };

    let usable = if respect_taskbar { work } else { bounds };
    let centre_x = usable.x0 + usable.w() * anchor_ratio.clamp(0.0, 1.0);
    let mut x0 = centre_x - size.x * 0.5;
    let mut y0 = hang_y - (-box_logical.y0);
    let mut clipped = false;

    if size.x <= bounds.w() {
        x0 = x0.clamp(bounds.x0, bounds.x1 - size.x);
    } else {
        // Wider than the display: centre it, and let the solver's reach clamp keep the plate on
        // screen. Refusing to place it, or clamping the size, are both worse than symmetric
        // overflow on a 4K-to-laptop dock change.
        x0 = bounds.x0 + (bounds.w() - size.x) * 0.5;
        clipped = true;
    }
    if size.y <= bounds.h() {
        y0 = y0.clamp(bounds.y0, bounds.y1 - size.y);
    } else {
        y0 = bounds.y0;
        clipped = true;
    }

    let frame = Rect::new(x0, y0, x0 + size.x, y0 + size.y);
    Placement {
        frame: frame.scaled(s),
        anchor: Vec2::new(-box_logical.x0, -box_logical.y0).scale(s),
        size_logical: size,
        scale: s,
        clipped,
    }
}

/// Pick a monitor from a persisted index, degrading to the primary rather than refusing to show
/// anything. A saved index that no longer exists is the common case (monitor unplugged), and the
/// wrong answer is a clock that never appears.
#[must_use]
pub fn pick_monitor<'a>(mons: &[&'a Monitor], wanted: u32, primary: u32) -> &'a Monitor {
    mons.iter().copied().find(|m| m.index == wanted).unwrap_or_else(|| {
        mons.iter()
            .copied()
            .find(|m| m.index == primary)
            .or(mons.first())
            .copied()
            .unwrap_or(&DEFAULT_MONITOR)
    })
}

/// Stand-in used when enumeration somehow yields nothing (no desktop, session 0). Zero-size is
/// fine: the caller shows nothing and retries on the next display change.
pub static DEFAULT_MONITOR: Monitor = Monitor {
    index: 0,
    bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
    work: Rect::new(0.0, 0.0, 0.0, 0.0),
    scale: 1.0,
    taskbar_top: false,
    taskbar_auto_hidden: false,
    primary: true,
};
