//! The boundary a second operating system would sit behind.
//!
//! Nothing here is Windows-shaped, and nothing here *does* anything: these are the questions the
//! model asks an environment, and `hanglock-win` answers them. A `hanglock-macos` would answer them
//! differently, and `hanglock-core` plus `hanglock-render` would not notice — that is the whole
//! reason this crate exists as its own compilation unit rather than as a module in the app.
//!
//! It is deliberately small. The tempting version of this file has a `trait Windowing` with forty
//! methods, at which point it is a private interface with a public cost, and every added method is
//! then owed twice. What is here is what genuinely differs between platforms: how to make a window
//! invisible-but-on-top, how to push pixels into it, when a frame is due, what the mouse is doing,
//! what displays exist, and where "now" comes from.
//!
//! Note that the app is free to hold *concrete* types (`hanglock_win::Window`) rather than `dyn
//! Trait`: these traits are the contract a second backend implements, not a dynamic-dispatch
//! mechanism the first one has to pay for.

#![forbid(unsafe_code)]

pub mod panel;

use hanglock_core::anchor::Anchor;
use hanglock_core::ids::{ClickThrough, PostureKind};
use hanglock_core::placement::{Monitor, Rect};
use hanglock_core::vec2::Vec2;

/// Everything the user can ask for from outside the overlay. One enum, because the tray menu, the
/// card's context menu and the settings window all offer the same commands and must not be able to
/// disagree about what exists — which is also why it carries values (`SetAnchor`, `SetClickThrough`)
/// rather than only toggles: a window with radio buttons needs to *set*, and inventing a second
/// vocabulary for it is how two surfaces drift.
///
/// Every command is one user intent, so the model can answer any of them without knowing which one
/// it was asked for; none of them is "the settings changed", which would hand the window the job of
/// deciding what is valid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    ToggleVisible,
    ToggleTopmost,
    ToggleSeconds,
    Toggle12Hour,
    ToggleMeridiem,
    SetPosture(PostureKind),
    SetClickThrough(ClickThrough),
    /// Which display to hang from, by the index the settings file stores — not the position in a
    /// list, which changes when a monitor is unplugged.
    SetMonitor(u32),
    /// The hang point, as the document holds it: a ratio and a drop, both validated on arrival.
    SetAnchor(Anchor),
    HangUp,
    HangDown,
    Bigger,
    Smaller,
    /// Put the clock back at the top centre of the display it is on, and make it visible. The
    /// recovery path for a clock that has been moved somewhere the user cannot click.
    ResetPosition,
    /// `launch_at_login`, applied: the model records the wish and the platform writes the registry.
    ToggleLaunchAtLogin,
    /// Bring up the settings window, or focus the one that is already up.
    OpenSettings,
    /// The one-paragraph "what is this, and how do I get out of it" box.
    About,
    Diagnostics,
    Quit,
}

/// Pointer and wheel input, already converted into the overlay's own device-pixel space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    /// Left button down at a point. The host decides whether that point is on the object; the
    /// model's answer is `Rope::begin_drag`'s return value.
    ///
    /// `alt` is whether Alt was down *at the press*, not whether it is down now: re-anchoring is
    /// latched for the whole gesture, so a user who releases Alt mid-drag keeps moving the hang point
    /// rather than having the clock snap back into swinging under their hand. Alt is held down long
    /// before the button is, which is what makes a press-time sample the readable one.
    Press {
        at: Vec2,
        alt: bool,
    },
    Move {
        at: Vec2,
        vel: Vec2,
        dt: f64,
    },
    Release {
        at: Vec2,
        vel: Vec2,
    },
    /// A wheel notch: positive is "hang longer".
    Wheel {
        delta: i32,
    },
    /// Right button: the host shows a menu at this point.
    Context {
        at: Vec2,
    },
    /// Pointer entered or left the interactive region. Drives the cursor shape, and nothing else:
    /// there is no hover *effect*, because an effect that redraws on hover would keep a settled
    /// object awake.
    Hover(bool),
}

/// The frame clock. `rate_hz` of 0 means "stop the clock entirely", which is the difference between
/// a settled app that idles at 0 % and one that idles at 0.6 %.
pub trait FrameClock {
    /// 0 = stopped. Otherwise the requested tick rate; the host may round to the display.
    fn set_rate(&mut self, rate_hz: u32);
    #[must_use]
    fn rate(&self) -> u32;
}

/// Pushing a finished frame into the window.
pub trait Presenter {
    /// Resize the surface. `w`/`h` in device px. Implementations must keep the buffer's contents
    /// undefined after a resize, and must not assume the caller will repaint.
    fn resize(&mut self, w: u32, h: u32);
    /// Present `pixels` (premultiplied BGRA, stride `4 * w`), updating only `rect` when given.
    ///
    /// The rect is not an optimisation footnote: presenting a quarter-size rect is the difference
    /// between a settled clock costing a 240×60 copy a second and a whole-window copy, and the
    /// measurement that says the presentation is the dominant cost of any animated transparent
    /// window.
    fn present(&mut self, pixels: &[u8], rect: Option<Rect>);
}

/// Window-level behaviour the model can toggle but does not implement.
pub trait OverlayHost {
    fn set_topmost(&mut self, topmost: bool);
    /// Move and/or resize in device px. Must not activate the window.
    fn set_frame(&mut self, frame: Rect);
    /// The mouse mode, as the two window styles that implement it. Which of the three modes a host
    /// turns into which style is the backend's business; what the seam fixes is that one call sets the
    /// whole pair, because `WS_EX_TRANSPARENT` and the hit test's answer have to agree or the clock
    /// becomes a window that ignores some clicks and eats others.
    fn set_input_mode(&mut self, mode: ClickThrough);
    fn show(&mut self, visible: bool);
    #[must_use]
    fn frame(&self) -> Rect;
    /// Whether the tray icon is up. The only way out of `click_through = "always"` is a control the
    /// overlay cannot swallow, so the model refuses that mode when this answers `false` rather than
    /// trusting a user to find the settings file.
    #[must_use]
    fn tray_present(&self) -> bool;
}

/// The two windows a user can be shown that are not the overlay: the settings form and the About
/// box. Kept as a trait for the same reason as `Tray` — a second platform will build these from
/// different parts, and the model must not be able to notice.
///
/// A window that could *write* settings would need its own validation, its own undo, and its own
/// story for what happens when the app quits while it is open. These cannot: they are given rows to
/// draw and hand back a `Command`, so closing them is never part of applying anything.
pub trait Dialogs {
    /// Draw these rows, bringing the window up or refocusing the one already up.
    fn show_settings(&mut self, groups: Vec<panel::Group>);
    /// Replace the rows if the window is open; do nothing otherwise. Called after every command the
    /// model answers, which is what keeps the checkboxes honest without the window polling anything.
    fn sync_settings(&mut self, groups: &[panel::Group]);
    #[must_use]
    fn settings_open(&self) -> bool;
    fn close_settings(&mut self);
    /// The About box, with the words the model supplies. Blocks, on platforms where that is what a
    /// message box does.
    fn about(&mut self, text: &str);
}

/// The system tray. Menus are built by the host because a native menu is the only one worth having;
/// `commands` is the flat list the app owns, so the model decides what exists and the host decides
/// what it looks like.
pub trait Tray {
    fn set_tooltip(&mut self, text: &str);
    /// Block until the menu is dismissed; `None` if the user clicked away.
    fn show_menu(&mut self, at_screen: Vec2, commands: &[Command]) -> Option<Command>;
    /// Called when the icon is clicked. The host decides which button means what.
    fn set_handler(&mut self, handler: Box<dyn FnMut(Command)>);
}

/// Display enumeration.
pub trait DisplaySource {
    fn monitors(&mut self) -> Vec<Monitor>;
    fn primary_index(&mut self) -> u32;
}

/// Implemented by the platform for `hanglock_core::clock::TimeSource`. Split from it so the model
/// stays free of the FFI types that "now" arrives through.
pub trait Clock {
    #[must_use]
    fn unix_ms(&self) -> i64;
    /// Offset from UTC in seconds, including whatever the OS currently thinks about daylight saving.
    #[must_use]
    fn local_offset_secs(&self) -> i32;
}

/// The question the whole design turns on, asked once per pixel the system tests: is this point part
/// of the object? Answers `Hit` or `Through`, and the host maps that to `HTCLIENT`/`HTTRANSPARENT`.
///
/// This is the seam that makes an idle Hanglock free where the reference implementation is not:
/// per-pixel hit testing is a property of the surface on Windows, so the hover affordance and the
/// click-through gate cost nothing, and no frame clock is needed to poll the cursor for either.
pub trait HitTest {
    #[must_use]
    fn hit(&self, device_point: Vec2) -> bool;
}

/// A source of events the model has to react to that are not input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemEvent {
    /// Display layout or scale changed. The overlay re-anchors and the rope re-fits, preserving
    /// motion.
    DisplaysChanged,
    /// This window's DPI changed.
    DpiChanged,
    /// The workstation resumed. Resynchronise the clock, drop the physics accumulator, and do not
    /// animate: nobody should come back to a swinging clock.
    Resumed,
    /// A settings write that changes geometry rather than pixels.
    Relayout,
}
