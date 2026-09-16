//! Hanglock's model: the rope, the clock, where the overlay goes, and what the user chose.
//!
//! Two rules hold this crate apart from the rest of the workspace, and both are the reason it
//! can be tested without a desktop:
//!
//! *   **No platform types, no `unsafe`, no dependencies.** Nothing here knows what an `HWND`
//!     is, so the solver, the placement maths and the settings document run identically on Linux
//!     CI, on a Windows VM and in a headless test.
//! *   **Everything in logical pixels and seconds.** The window layer multiplies by the display's
//!     scale at the last moment; keeping the factor out of the model is what makes 100 %, 150 %
//!     and 200 % scaling testable rather than plausible.
//!
//! The solver and the painter are transcriptions of `tools/model/hanglock_ref.py`, constant for
//! constant, and `tests/golden_trace.rs` asserts they have not drifted.

#![forbid(unsafe_code)]

pub mod clock;
pub mod ids;
pub mod placement;
pub mod rope;
pub mod scene;
pub mod settings;
pub mod vec2;

/// Every number the physics depends on, in logical pixels per second squared.
pub mod units {
    /// Reference DPI: the size of one logical pixel at 100 % scaling.
    pub const LOGICAL_DPI: f64 = 96.0;
}
