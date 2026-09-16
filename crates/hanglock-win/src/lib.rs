//! The Windows backend.
//!
//! This is the only crate in the workspace that may contain `unsafe`, name a Win32 symbol, or know
//! that a device context exists. Everything it exposes to the app is safe and expressed in the
//! model's own units, and the `unsafe` is confined to [`sys`] (declarations) plus the small number
//! of calls in [`window`] and [`surface`] that use them.
//!
//! What lives here, and why it is worth the ~1.2k lines: a clock that hangs on the desktop has to
//! own five behaviours no toolkit gets right out of the box — per-pixel transparency, per-pixel
//! click-through, staying on top without ever taking focus, surviving DPI and monitor changes, and
//! stopping completely when nothing is moving. Frameworks either expose none of them, or expose them
//! as flags that fight each other. Here each is one function with a comment saying what it is for.

pub mod autostart;
pub mod displays;
pub mod icon;
pub mod surface;
pub mod sys;
pub mod time;
pub mod tray;
pub mod window;

pub use surface::Surface;
pub use window::{run, AppHook, Cursor, Host, HitShape, OverlayConfig, TIMER_SECOND, TIMER_TICK};
