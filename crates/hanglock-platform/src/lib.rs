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

use hanglock_core::placement::{Monitor, Rect};
use hanglock_core::vec2::Vec2;

/// Everything the user can ask for from outside the overlay. One enum, because the tray menu, the
/// card's context menu and (later) a settings window all offer the same commands and must not be
/// able to disagree about what exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    ToggleVisible,
    ToggleTopmost,
    ToggleSeconds,
    Toggle12Hour,
    SetPosture(hanglock_core::ids::PostureKind),
    HangUp,
    HangDown,
    Bigger,
    Smaller,
    ResetPosition,
    Diagnostics,
    Quit,
}

/// Pointer and wheel input, already converted into the overlay's own device-pixel space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    /// Left button down at a point. The host decides whether that point is on the object; the
    /// model's answer is `Rope::begin_drag`'s return value.
    Press { at: Vec2 },
    Move { at: Vec2, vel: Vec2, dt: f64 },
    Release { at: Vec2, vel: Vec2 },
    /// A wheel notch: positive is "hang longer".
    Wheel { delta: i32 },
    /// Right button: the host shows a menu at this point.
    Context { at: Vec2 },
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
    /// Whole-window click-through, for the `click_through = "always"` case. The per-pixel case is
    /// not here because it is not a toggle: it is `HitTest`, below.
    fn set_ignore_input(&mut self, ignore: bool);
    fn show(&mut self, visible: bool);
    #[must_use]
    fn frame(&self) -> Rect;
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
