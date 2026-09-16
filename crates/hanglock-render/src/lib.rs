//! `Scene` → a buffer of premultiplied BGRA, plus the dirty rect to present.
//!
//! ## Why software, when Direct2D exists
//!
//! Three reasons, in order of how much they matter here.
//!
//! 1.  **The frame is tiny and the primitives are analytic.** A cord is a polyline of capsules and
//!     the plate is one rounded box; both have closed-form distance fields, so coverage — and
//!     therefore anti-aliasing, round caps, joins and a soft shadow — falls out of a `smoothstep`
//!     on a distance instead of a tessellation, a mask, or a per-frame blur pass. The whole paint is
//!     a few tens of thousands of pixels of arithmetic.
//! 2.  **A layered window wants a CPU buffer anyway.** `UpdateLayeredWindow` takes a bitmap; a GPU
//!     path has to get its result into one (or into a redirecting surface the compositor then
//!     blends), so the obvious saving is not free.
//! 3.  **Nothing to lose.** No device to create, no `DXGI_ERROR_DEVICE_REMOVED` to recover from at
//!     60 Hz, no driver matrix, works in a VM and over RDP. The reference project's most instructive
//!     measurement is that redrawing a transparent window dominated the cost of swinging — the cost
//!     is in presentation, and no renderer choice avoids it, so the choice should be the one that
//!     cannot fail.
//!
//! The rule inherited from that project is enforced here: **no filters in the frame path**. Blur
//! and drop-shadow primitives rasterise an offscreen surface per frame; every soft edge in Hanglock
//! is a distance-field ramp, which is exact, cheaper, and cannot leak memory.
//!
//! ## Buffer format
//!
//! Premultiplied BGRA8, top-down, stride `w * 4`. Premultiplied because it is what
//! `BLENDFUNCTION{AC_SRC_ALPHA}` requires, and because source-over on premultiplied values is
//! `dst = src + dst * (1 - srcA)`: one multiply per channel, no divide, and no alpha-dependent
//! colour drift while primitives overlap.

#![forbid(unsafe_code)]

pub mod canvas;
pub mod face_data;
pub mod paint;
pub mod png;
pub mod theme;

pub use canvas::Canvas;
pub use paint::paint;
pub use theme::Theme;
