//! Small enums that are persisted as strings. Strings on disk, enums in code: a settings file that
//! says `posture = "plate"` survives a build that renamed the variant's fields, and a user's editor
//! does not have to know that 2 means Mounted.

/// How the overlay treats the mouse.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClickThrough {
    /// Clicks pass through everywhere except the plate. The default, and the only mode where the
    /// object is an object: you can grab it, and the desktop underneath still works.
    #[default]
    Hover,
    /// Every pixel is transparent to the mouse. For a clock that should never be in the way; the
    /// trade is that it cannot be dragged until this is turned off.
    Always,
}

impl ClickThrough {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hover => "hover",
            Self::Always => "always",
        }
    }
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "hover" => Some(Self::Hover),
            "always" => Some(Self::Always),
            _ => None,
        }
    }
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
