//! Wall-clock time, split into a source (platform) and a pure projection of it into a face.

pub mod format;

pub use format::{Civil, FaceText};

/// Where "now" comes from. Injectable so every formatting rule below is testable at an exact
/// instant — including the two that are always wrong in shipping clocks: the minute and the
/// meridiem boundary.
pub trait TimeSource {
    /// Milliseconds since the Unix epoch, UTC.
    fn unix_ms(&self) -> i64;
    /// Local offset from UTC in seconds, DST included, as the OS understands it right now.
    fn local_offset_secs(&self) -> i32;
}

/// Local civil time from an instant. The only reason this is a function rather than a field: it can
/// be tested against a frozen [`TimeSource`].
#[must_use]
pub fn local_now(src: &dyn TimeSource) -> Civil {
    let ms = src.unix_ms() + i64::from(src.local_offset_secs()) * 1000;
    format::civil_from_unix_ms(ms)
}

/// Whether the seconds field changed, which is all the steady state of a clock ever is: the sole
/// reason the settled overlay repaints at 1 Hz and nothing else.
#[must_use]
pub fn ticked(prev: &Civil, now: &Civil) -> bool {
    prev.second != now.second
        || prev.minute != now.minute
        || prev.hour != now.hour
        || prev.day != now.day
}
