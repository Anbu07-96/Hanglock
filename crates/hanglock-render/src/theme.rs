//! Hanglock's colours and the few metrics that are style rather than geometry. 0.0..1.0 floats
//! because the painter multiplies by coverage in f64 and converts once, at the write.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Rgba {
    #[must_use]
    pub const fn rgb(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }
}

/// The whole look, in one value. Phase 4 turns this into named themes; nothing else changes, which
/// is the point of it being data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub plate_top: Rgba,
    pub plate_bottom: Rgba,
    /// 1 px all round. Without it a dark plate on a dark wallpaper is a hole, not an object.
    pub rim: Rgba,
    /// The top edge catches light; the bottom edge grounds the plate.
    pub rim_top: Rgba,
    pub rim_bottom: Rgba,
    pub ink: Rgba,
    pub accent: Rgba,
    pub cord: Rgba,
    pub cord_lit: Rgba,
    pub cord_shadow: Rgba,
    pub mount: Rgba,
    pub shadow_alpha: f64,
    pub shadow_blur: f64,
    pub shadow_drop: f64,
    /// Text metrics, as multiples of the card's height, so they scale with the size setting without
    /// needing a second table.
    pub time_cap: f64,
    pub suffix_cap: f64,
    pub suffix_gap: f64,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            plate_top: Rgba::rgb(0.150, 0.166, 0.188, 0.94),
            plate_bottom: Rgba::rgb(0.072, 0.079, 0.092, 0.965),
            rim: Rgba::rgb(1.0, 1.0, 1.0, 0.20),
            rim_top: Rgba::rgb(1.0, 1.0, 1.0, 0.17),
            rim_bottom: Rgba::rgb(0.0, 0.0, 0.0, 0.40),
            ink: Rgba::rgb(0.965, 0.973, 0.985, 1.0),
            accent: Rgba::rgb(0.44, 0.74, 0.99, 1.0),
            // A mid-tone cord with a lit edge and a dark edge: near-black vanishes on a dark
            // desktop, near-white vanishes on a document, and only the pair of them reads anywhere.
            cord: Rgba::rgb(0.30, 0.325, 0.37, 0.95),
            cord_lit: Rgba::rgb(0.82, 0.86, 0.92, 0.34),
            cord_shadow: Rgba::rgb(0.0, 0.0, 0.0, 0.38),
            mount: Rgba::rgb(0.085, 0.095, 0.115, 0.97),
            shadow_alpha: 0.30,
            shadow_blur: 7.5,
            shadow_drop: 7.0,
            time_cap: 0.34,
            suffix_cap: 0.24,
            suffix_gap: 0.18,
        }
    }
}
