//! The settings document: its shape, its ranges, and a codec that cannot lose a user's preferences.
//!
//! Format is a hand-parsed subset of TOML — `key = value` lines under `[section]` headers. That is
//! a dependency decision, not a shortcut (see `docs/decisions/0002-zero-dependencies.md`), and it
//! stays a *valid* TOML file, so replacing this with `toml` + `serde` later is a swap of this
//! module, not a migration of anyone's settings.
//!
//! The rules that matter, and the reason each exists:
//!
//! *   **Unknown keys are ignored, missing keys take their default.** A settings file outlives the
//!     build that wrote it. A synthesised decoder that throws on a missing field means that adding
//!     one setting silently discards every existing user's preferences — the single most expensive
//!     mistake a utility like this can make.
//! *   **Numbers are clamped on the way in**, by the same ranges the UI uses, so a hand-edited or
//!     half-written file yields a sane overlay rather than one parked off screen.
//! *   **Nothing is dropped without a trace**: a file that cannot be read at all is renamed by the
//!     caller before defaults are written, never overwritten.

use crate::ids::{ClickThrough, PostureKind};

/// Written into every document so a future breaking change can migrate deliberately.
pub const SCHEMA: u32 = 1;

pub mod limits {
    pub const HANG: (f64, f64) = (70.0, 260.0);
    pub const SCALE: (f64, f64) = (0.75, 1.75);
    pub const OPACITY: (f64, f64) = (0.35, 1.0);
    pub const ANCHOR_RATIO: (f64, f64) = (0.0, 1.0);
    /// How far below the line a clock hangs the hang point may be moved, in logical px. The upper
    /// bound is a *reach*, not a suggestion, and the placement maths is what keeps the clock on a
    /// screen — so a 4k panel has to be able to hold a 4k drop.
    pub const ANCHOR_DROP: (f64, f64) = (0.0, 4096.0);
    pub const FPS_CAP: (u32, u32) = (24, 120);
    pub const MARGIN: (f64, f64) = (4.0, 48.0);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Overlay {
    pub enabled: bool,
    pub monitor_index: u32,
    /// 0 = left edge of the usable width, 1 = right edge. A ratio, not a pixel, so it survives a
    /// resolution or scale change.
    pub anchor_ratio: f64,
    /// Logical px below the line the clock hangs from: the other half of the same idea, and likewise
    /// a distance rather than a coordinate. `0` means "hung from the edge", which is where a first
    /// run puts the clock and what every file written before this key existed holds.
    pub anchor_drop: f64,
    pub hang: f64,
    pub scale: f64,
    pub opacity: f64,
    pub topmost: bool,
    pub click_through: ClickThrough,
    pub respect_taskbar: bool,
    pub margin: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub hour12: bool,
    pub seconds: bool,
    pub meridiem: bool,
    pub posture: PostureKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct General {
    pub launch_at_login: bool,
    pub fps_cap: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub schema: u32,
    pub overlay: Overlay,
    pub face: Face,
    pub general: General,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            overlay: Overlay {
                enabled: true,
                monitor_index: 0,
                anchor_ratio: 0.5,
                anchor_drop: 0.0,
                hang: 150.0,
                scale: 1.0,
                opacity: 1.0,
                topmost: true,
                click_through: ClickThrough::Hover,
                respect_taskbar: true,
                margin: 16.0,
            },
            face: Face {
                hour12: true,
                seconds: false,
                meridiem: true,
                posture: PostureKind::Plate,
            },
            general: General {
                launch_at_login: false,
                fps_cap: 60,
            },
        }
    }
}

impl Settings {
    /// The card the document asks for: the design size with the user's multiplier applied, and the
    /// cord length they chose.
    ///
    /// One function, because the painter, the solver and the placement maths must all be told the same
    /// size. A second place that multiplies by `overlay.scale` is how a clock ends up drawn at one size
    /// and clickable at another, and a place that forgets it is a clock whose plate is the right size
    /// but whose cord is too short for it.
    #[must_use]
    pub fn card(&self) -> crate::rope::config::CardSpec {
        let base = crate::rope::config::CardSpec::default();
        let k = self.overlay.scale;
        crate::rope::config::CardSpec {
            width: base.width * k,
            height: base.height * k,
            bracket: base.bracket,
            corner: base.corner,
            hang: self.overlay.hang,
        }
    }

    /// Where the clock hangs from, as the pair of numbers the document holds.
    #[must_use]
    pub fn anchor(&self) -> crate::anchor::Anchor {
        crate::anchor::Anchor::from_overlay(&self.overlay)
    }

    /// Pull every ranged value back inside its limits. Called after parsing, so the rest of the app
    /// can assume the document is valid and never re-check.
    pub fn sanitize(&mut self) {
        let o = &mut self.overlay;
        let d = Self::default();
        // `finite` first: `clamp` passes NaN straight through (`NaN < min` and `NaN > max` are both
        // false), and `"nan".parse::<f64>()` succeeds — so a hand-edited or truncated file can put a
        // NaN in the position of the plate, and a NaN coordinate propagates through the solver and the
        // painter into a window that never redraws. Found while writing the tests for this function,
        // which is the argument for having them.
        o.anchor_ratio = finite(o.anchor_ratio, d.overlay.anchor_ratio)
            .clamp(limits::ANCHOR_RATIO.0, limits::ANCHOR_RATIO.1);
        o.anchor_drop = finite(o.anchor_drop, d.overlay.anchor_drop)
            .clamp(limits::ANCHOR_DROP.0, limits::ANCHOR_DROP.1);
        o.hang = finite(o.hang, d.overlay.hang).clamp(limits::HANG.0, limits::HANG.1);
        o.scale = finite(o.scale, d.overlay.scale).clamp(limits::SCALE.0, limits::SCALE.1);
        o.opacity =
            finite(o.opacity, d.overlay.opacity).clamp(limits::OPACITY.0, limits::OPACITY.1);
        o.margin = finite(o.margin, d.overlay.margin).clamp(limits::MARGIN.0, limits::MARGIN.1);
        self.general.fps_cap = self
            .general
            .fps_cap
            .clamp(limits::FPS_CAP.0, limits::FPS_CAP.1);
    }

    /// Write the document. Keys are emitted in a fixed order so two runs on the same machine
    /// produce a byte-identical file, which makes a settings diff in a bug report mean something.
    #[must_use]
    pub fn to_toml(&self) -> String {
        let mut s = String::with_capacity(512);
        s.push_str("# Hanglock settings. Ranges are enforced on load; unknown keys are ignored.\n");
        s.push_str(&format!("schema = {}\n", self.schema));
        s.push_str("\n[overlay]\n");
        s.push_str(&format!("enabled = {}\n", yes(self.overlay.enabled)));
        s.push_str(&format!("monitor_index = {}\n", self.overlay.monitor_index));
        s.push_str(&format!(
            "anchor_ratio = {}\n",
            fix(self.overlay.anchor_ratio)
        ));
        s.push_str(&format!(
            "anchor_drop = {}\n",
            fix(self.overlay.anchor_drop)
        ));
        s.push_str(&format!("hang = {}\n", fix(self.overlay.hang)));
        s.push_str(&format!("scale = {}\n", fix(self.overlay.scale)));
        s.push_str(&format!("opacity = {}\n", fix(self.overlay.opacity)));
        s.push_str(&format!("topmost = {}\n", yes(self.overlay.topmost)));
        s.push_str(&format!(
            "click_through = \"{}\"\n",
            self.overlay.click_through.as_str()
        ));
        s.push_str(&format!(
            "respect_taskbar = {}\n",
            yes(self.overlay.respect_taskbar)
        ));
        s.push_str(&format!("margin = {}\n", fix(self.overlay.margin)));
        s.push_str("\n[face]\n");
        s.push_str(&format!("hour12 = {}\n", yes(self.face.hour12)));
        s.push_str(&format!("seconds = {}\n", yes(self.face.seconds)));
        s.push_str(&format!("meridiem = {}\n", yes(self.face.meridiem)));
        s.push_str(&format!("posture = \"{}\"\n", self.face.posture.as_str()));
        s.push_str("\n[general]\n");
        s.push_str(&format!(
            "launch_at_login = {}\n",
            yes(self.general.launch_at_login)
        ));
        s.push_str(&format!("fps_cap = {}\n", self.general.fps_cap));
        s
    }

    /// Parse, never fail. Returns the document (missing keys defaulted) and one warning per line it
    /// could not use, which the app surfaces in the tray's diagnostics rather than in a dialog.
    #[must_use]
    pub fn from_toml(text: &str) -> (Settings, Vec<String>) {
        let mut out = Settings::default();
        let mut warnings = Vec::new();
        let mut section = String::new();
        for (n, raw) in text.lines().enumerate() {
            let line = strip_comment(raw.trim());
            if line.is_empty() {
                continue;
            }
            if let Some(head) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                section = head.trim().to_ascii_lowercase().to_string();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                warnings.push(format!("line {}: not a key = value pair", n + 1));
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            let sec = section.as_str();
            // One arm per key, and a fall-through that ignores anything unknown. Every assignment
            // here is total: a value that will not parse keeps the default it already has, because a
            // half-readable file must not be allowed to reset the rest of it.
            match (sec, key) {
                ("", "schema") => out.schema = int_of(value, i64::from(out.schema)).max(1) as u32,
                ("overlay", "enabled") => out.overlay.enabled = bool_of(value),
                ("overlay", "monitor_index") => {
                    out.overlay.monitor_index = int_of(value, 0).max(0) as u32;
                }
                ("overlay", "anchor_ratio") => out.overlay.anchor_ratio = float_of(value, 0.5),
                ("overlay", "anchor_drop") => out.overlay.anchor_drop = float_of(value, 0.0),
                ("overlay", "hang") => out.overlay.hang = float_of(value, 150.0),
                ("overlay", "scale") => out.overlay.scale = float_of(value, 1.0),
                ("overlay", "opacity") => out.overlay.opacity = float_of(value, 1.0),
                ("overlay", "topmost") => out.overlay.topmost = bool_of(value),
                ("overlay", "click_through") => match ClickThrough::parse(value) {
                    Some(v) => out.overlay.click_through = v,
                    None => warnings.push(format!(
                        "line {}: unknown click_through {value:?}, keeping {}",
                        n + 1,
                        out.overlay.click_through.as_str(),
                    )),
                },
                ("overlay", "respect_taskbar") => out.overlay.respect_taskbar = bool_of(value),
                ("overlay", "margin") => out.overlay.margin = float_of(value, 16.0),
                ("face", "hour12") => out.face.hour12 = bool_of(value),
                ("face", "seconds") => out.face.seconds = bool_of(value),
                ("face", "meridiem") => out.face.meridiem = bool_of(value),
                ("face", "posture") => match PostureKind::parse(value) {
                    Some(v) => out.face.posture = v,
                    None => warnings.push(format!(
                        "line {}: unknown posture {value:?}, keeping {}",
                        n + 1,
                        out.face.posture.as_str(),
                    )),
                },
                ("general", "launch_at_login") => out.general.launch_at_login = bool_of(value),
                ("general", "fps_cap") => out.general.fps_cap = int_of(value, 60).max(0) as u32,
                _ => warnings.push(format!(
                    "line {}: ignored unknown key {key} in [{sec}]",
                    n + 1
                )),
            }
        }
        out.sanitize();
        (out, warnings)
    }
}

#[must_use]
fn finite(v: f64, fallback: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        fallback
    }
}

fn yes(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

/// Trailing zeros trimmed, but never scientific notation: a settings file is meant to be readable,
/// and `{}` on a float happily prints `1e-7`.
fn fix(v: f64) -> String {
    let t = format!("{v:.4}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    t.to_string()
}

fn bool_of(v: &str) -> bool {
    matches!(v.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on")
}

fn int_of(v: &str, fallback: i64) -> i64 {
    v.parse::<i64>().unwrap_or(fallback)
}

fn float_of(v: &str, fallback: f64) -> f64 {
    v.parse::<f64>().unwrap_or(fallback)
}

fn strip_comment(line: &str) -> &str {
    // `#` inside a quoted value would be data; none of the values this file holds can contain one,
    // so the simple scan is honest here and cheaper than a real lexer.
    match line.find('#') {
        Some(i) => line[..i].trim_end(),
        None => line,
    }
}
