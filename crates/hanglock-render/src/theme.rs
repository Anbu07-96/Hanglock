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
    pub style: ClockStyle,
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
    pub shell: Rgba,
    pub face: Rgba,
    pub inner_rim: Rgba,
    pub rope_width: f64,
    pub rope_edge: f64,
    pub rim_width: f64,
    pub inset: f64,
    pub eyelet_radius: f64,
    pub eyelet_width: f64,
    pub seconds_scale: f64,
}

impl Default for Theme {
    fn default() -> Self {
        Self::for_style(ClockStyle::default())
    }
}

impl Theme {
    #[must_use]
    pub fn for_style(style: ClockStyle) -> Self {
        match style {
            ClockStyle::ModernMinimal => Self {
                style,
                plate_top: Rgba::rgb(0.145, 0.155, 0.172, 0.985),
                plate_bottom: Rgba::rgb(0.085, 0.092, 0.105, 0.99),
                rim: Rgba::rgb(0.72, 0.76, 0.82, 0.32),
                rim_top: Rgba::rgb(1.0, 1.0, 1.0, 0.25),
                rim_bottom: Rgba::rgb(0.0, 0.0, 0.0, 0.42),
                ink: Rgba::rgb(0.96, 0.945, 0.89, 1.0),
                accent: Rgba::rgb(0.82, 0.82, 0.80, 0.92),
                cord: Rgba::rgb(0.18, 0.19, 0.21, 0.96),
                cord_lit: Rgba::rgb(0.72, 0.74, 0.78, 0.28),
                cord_shadow: Rgba::rgb(0.0, 0.0, 0.0, 0.18),
                mount: Rgba::rgb(0.13, 0.14, 0.16, 0.98),
                shadow_alpha: 0.14,
                shadow_blur: 3.2,
                shadow_drop: 1.4,
                time_cap: 0.36,
                suffix_cap: 0.31,
                suffix_gap: 0.15,
                shell: Rgba::rgb(0.075, 0.082, 0.094, 1.0),
                face: Rgba::rgb(0.105, 0.114, 0.128, 1.0),
                inner_rim: Rgba::rgb(0.0, 0.0, 0.0, 0.34),
                rope_width: 1.25,
                rope_edge: 0.24,
                rim_width: 3.0,
                inset: 5.0,
                eyelet_radius: 3.3,
                eyelet_width: 1.15,
                seconds_scale: 0.62,
            },
            ClockStyle::PremiumMetalGlass => Self {
                style,
                plate_top: Rgba::rgb(0.10, 0.105, 0.115, 0.97),
                plate_bottom: Rgba::rgb(0.035, 0.038, 0.045, 0.99),
                rim: Rgba::rgb(0.72, 0.67, 0.59, 0.82),
                rim_top: Rgba::rgb(0.98, 0.92, 0.82, 0.72),
                rim_bottom: Rgba::rgb(0.08, 0.07, 0.065, 0.88),
                ink: Rgba::rgb(0.97, 0.91, 0.80, 1.0),
                accent: Rgba::rgb(0.88, 0.82, 0.72, 0.96),
                cord: Rgba::rgb(0.10, 0.10, 0.11, 0.98),
                cord_lit: Rgba::rgb(0.73, 0.68, 0.60, 0.30),
                cord_shadow: Rgba::rgb(0.0, 0.0, 0.0, 0.25),
                mount: Rgba::rgb(0.42, 0.39, 0.35, 1.0),
                shadow_alpha: 0.18,
                shadow_blur: 4.2,
                shadow_drop: 2.0,
                time_cap: 0.34,
                suffix_cap: 0.29,
                suffix_gap: 0.16,
                shell: Rgba::rgb(0.30, 0.285, 0.26, 1.0),
                face: Rgba::rgb(0.045, 0.048, 0.055, 0.97),
                inner_rim: Rgba::rgb(0.0, 0.0, 0.0, 0.62),
                rope_width: 1.35,
                rope_edge: 0.28,
                rim_width: 6.0,
                inset: 9.0,
                eyelet_radius: 3.6,
                eyelet_width: 1.25,
                seconds_scale: 0.60,
            },
            ClockStyle::SoftMattePlayful => Self {
                style,
                plate_top: Rgba::rgb(0.20, 0.16, 0.20, 0.99),
                plate_bottom: Rgba::rgb(0.115, 0.085, 0.115, 1.0),
                rim: Rgba::rgb(0.50, 0.38, 0.42, 0.50),
                rim_top: Rgba::rgb(0.82, 0.72, 0.72, 0.25),
                rim_bottom: Rgba::rgb(0.05, 0.03, 0.05, 0.48),
                ink: Rgba::rgb(0.98, 0.91, 0.80, 1.0),
                accent: Rgba::rgb(0.78, 0.49, 0.34, 0.98),
                cord: Rgba::rgb(0.20, 0.17, 0.18, 0.98),
                cord_lit: Rgba::rgb(0.68, 0.55, 0.54, 0.25),
                cord_shadow: Rgba::rgb(0.0, 0.0, 0.0, 0.17),
                mount: Rgba::rgb(0.28, 0.20, 0.22, 1.0),
                shadow_alpha: 0.12,
                shadow_blur: 3.8,
                shadow_drop: 1.2,
                time_cap: 0.35,
                suffix_cap: 0.32,
                suffix_gap: 0.14,
                shell: Rgba::rgb(0.155, 0.115, 0.15, 1.0),
                face: Rgba::rgb(0.095, 0.073, 0.09, 1.0),
                inner_rim: Rgba::rgb(0.03, 0.02, 0.03, 0.38),
                rope_width: 1.45,
                rope_edge: 0.32,
                rim_width: 7.5,
                inset: 10.0,
                eyelet_radius: 4.0,
                eyelet_width: 1.4,
                seconds_scale: 0.64,
            },
        }
    }

    #[must_use]
    pub fn all() -> [Self; 3] {
        ClockStyle::ALL.map(Self::for_style)
    }
}

#[allow(dead_code)]
fn legacy_theme_for_reference() -> Theme {
    Theme {
            style: ClockStyle::ModernMinimal,
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
            shell: Rgba::rgb(0.08, 0.08, 0.09, 1.0),
            face: Rgba::rgb(0.10, 0.10, 0.11, 1.0),
            inner_rim: Rgba::rgb(0.0, 0.0, 0.0, 0.3),
            rope_width: 1.7,
            rope_edge: 0.34,
            rim_width: 4.0,
            inset: 6.0,
            eyelet_radius: 3.5,
            eyelet_width: 1.25,
            seconds_scale: 0.62,
        }
    }
use hanglock_core::ids::ClockStyle;
