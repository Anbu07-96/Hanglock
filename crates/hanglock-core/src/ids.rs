//! Small enums that are persisted as strings. Strings on disk, enums in code: a settings file that
//! says `posture = "plate"` survives a build that renamed the variant's fields, and a user's editor
//! does not have to know that 2 means Mounted.

/// How the overlay treats the mouse.
///
/// Three settings, in order of how much of the desktop the clock claims. The middle one is the
/// default and the only one where the object is both an object and not an obstacle; the two at the
/// ends exist because some people want a window that behaves like a window, and some want the clock
/// to be nowhere the mouse can go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClickThrough {
    /// The window's whole rectangle takes the mouse. Nothing falls through, so the clock is easy to
    /// hit with a trackpad and the desktop under the *box* is not: this is the mode for "treat it
    /// like a normal window".
    Solid,
    /// Clicks pass through everywhere except the plate and the hang ring. The default, and the mode
    /// the design is built around: you can grab the object, and the desktop underneath still works.
    #[default]
    Hover,
    /// Every pixel is transparent to the mouse. For a clock that should never be in the way; the
    /// trade is that it cannot be dragged or clicked at all, so the tray is the only control surface
    /// and `hanglock-win` refuses this mode when no tray icon could be installed.
    Always,
}

impl ClickThrough {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Hover => "hover",
            Self::Always => "always",
        }
    }
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "solid" => Some(Self::Solid),
            "hover" => Some(Self::Hover),
            "always" => Some(Self::Always),
            _ => None,
        }
    }

    /// What the tray and the settings panel call it. Words a user reads, so they live next to the
    /// enum rather than in each UI: two menus that name the same mode differently are two bugs.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Interactive (whole window)",
            Self::Hover => "Transparent areas click through",
            Self::Always => "Fully click-through",
        }
    }

    /// Whether the overlay answers the mouse over the object at all.
    #[must_use]
    pub fn interactive(self) -> bool {
        !matches!(self, Self::Always)
    }

    /// Whether the answer is "every pixel of the window" rather than "every pixel of the object".
    #[must_use]
    pub fn takes_window(self) -> bool {
        matches!(self, Self::Solid)
    }

    /// Whether input is dropped at the window style rather than by the hit test. `WS_EX_TRANSPARENT`
    /// is a whole-window switch, so only `Always` reaches for it.
    #[must_use]
    pub fn ignores_input(self) -> bool {
        matches!(self, Self::Always)
    }

    /// Every mode, in menu order: most claimed desktop first.
    pub const ALL: [Self; 3] = [Self::Solid, Self::Hover, Self::Always];
}

/// How far the plate may tilt while swinging. A legibility control, expressed as a physical one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PostureKind {
    /// Swings like a shop sign. The most alive, the least readable mid-swing.
    Natural,
    /// Default: tilts a few degrees and returns to level inside a second.
    #[default]
    Plate,
    /// Bolted to a rail.
    Mounted,
    /// Level always. For reduced-motion users, and for anyone who wants a clock, not a toy.
    Locked,
}

impl PostureKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Natural => "natural",
            Self::Plate => "plate",
            Self::Mounted => "mounted",
            Self::Locked => "locked",
        }
    }
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "natural" => Some(Self::Natural),
            "plate" => Some(Self::Plate),
            "mounted" => Some(Self::Mounted),
            "locked" => Some(Self::Locked),
            _ => None,
        }
    }

    /// What the tray and the settings panel call it. As [`ClickThrough::label`]: one list of words
    /// for both surfaces, because two menus that describe one setting differently are two bugs.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Natural => "Swings freely",
            Self::Plate => "Swings a little",
            Self::Mounted => "Rigid",
            Self::Locked => "Always level",
        }
    }

    /// Every posture, in menu order. The UI lists the whole set from here, so a fifth variant shows
    /// up in both surfaces without either of them being touched.
    pub const ALL: [Self; 4] = [Self::Natural, Self::Plate, Self::Mounted, Self::Locked];

    #[must_use]
    pub fn posture(self) -> crate::rope::config::Posture {
        match self {
            Self::Natural => crate::rope::config::Posture::NATURAL,
            Self::Plate => crate::rope::config::Posture::PLATE,
            Self::Mounted => crate::rope::config::Posture::MOUNTED,
            Self::Locked => crate::rope::config::Posture::LOCKED,
        }
    }
}
