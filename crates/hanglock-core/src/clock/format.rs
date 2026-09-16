//! Instant -> digits. Deliberately no chrono, no `time`, no `SystemTime` arithmetic in the model:
//! the platform hands over milliseconds since the epoch and an offset, and everything else is
//! arithmetic that can be checked on any machine.

/// A date and time in components.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// Days-since-epoch to civil date, after Howard Hinnant's `civil_from_days`: branch-free, exact
/// across the 1970 and 2100 century rules, and free of any locale or platform call to test.
#[must_use]
pub fn civil_from_unix_ms(ms: i64) -> Civil {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    Civil {
        year: y + i64::from(i32::from(m <= 2)),
        month: m as u32,
        day: d as u32,
        hour: (sod / 3600) as u32,
        minute: (sod / 60 % 60) as u32,
        second: (sod % 60) as u32,
    }
}

/// What to show. `hour12` and `meridiem` are separate because a 12-hour face without a meridiem is
/// a legitimate choice, not a half-finished one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FaceOptions {
    pub hour12: bool,
    pub seconds: bool,
    pub meridiem: bool,
}

/// A rendered face: at most 8 glyphs of main text ("10:42:07" without separators is the longest
/// case this face draws) plus a 2-character meridiem. Fixed-size on purpose — a hanging clock that
/// allocates per second is a clock with a memory curve.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FaceText {
    pub main: [u8; 8],
    pub main_len: u8,
    pub suffix: [u8; 2],
    pub suffix_len: u8,
}

impl FaceText {
    #[must_use]
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.main[..usize::from(self.main_len)]).unwrap_or("")
    }
    #[must_use]
    pub fn suffix_str(&self) -> &str {
        std::str::from_utf8(&self.suffix[..usize::from(self.suffix_len)]).unwrap_or("")
    }
    /// Digits and separators only, for the layout pass: this face draws ':' as part of the run.
    #[must_use]
    pub fn glyph_count(&self) -> usize {
        usize::from(self.main_len)
    }
}

#[must_use]
pub fn format(c: &Civil, o: &FaceOptions) -> FaceText {
    let mut h = c.hour;
    let mut suffix = [b' '; 2];
    let mut suffix_len = 0u8;
    if o.hour12 {
        let pm = h >= 12;
        let mut hh = h % 12;
        if hh == 0 {
            hh = 12;
        }
        h = hh;
        if o.meridiem {
            suffix = if pm { [b'P', b'M'] } else { [b'A', b'M'] };
            suffix_len = 2;
        }
    }

    let mut buf = [b' '; 8];
    let mut n = 0usize;
    // No leading zero in 12-hour mode; always padded in 24-hour mode. Both are the convention, and
    // tabular digits make "9:05" as wide a slot as "10:05" would be otherwise — see face.rs.
    if !o.hour12 || h >= 10 {
        buf[n] = b'0' + (h / 10) as u8;
        n += 1;
    }
    buf[n] = b'0' + (h % 10) as u8;
    n += 1;
    buf[n] = b':';
    n += 1;
    buf[n] = b'0' + (c.minute / 10) as u8;
    n += 1;
    buf[n] = b'0' + (c.minute % 10) as u8;
    n += 1;
    if o.seconds {
        buf[n] = b':';
        n += 1;
        buf[n] = b'0' + (c.second / 10) as u8;
        n += 1;
        buf[n] = b'0' + (c.second % 10) as u8;
        n += 1;
    }
    FaceText {
        main: buf,
        main_len: n as u8,
        suffix,
        suffix_len,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i64, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> Civil {
        Civil {
            year: y,
            month: mo,
            day: d,
            hour: h,
            minute: mi,
            second: s,
        }
    }

    #[test]
    fn epoch_is_1970_01_01() {
        let c = civil_from_unix_ms(0);
        assert_eq!(c, at(1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn leap_day_exists_and_next_year_does_not() {
        // 2024-02-29T23:59:59Z -> +1s is 2024-03-01T00:00:00Z
        let a = civil_from_unix_ms(1_709_251_199_000);
        assert_eq!((a.year, a.month, a.day, a.hour), (2024, 2, 29, 23));
        let b = civil_from_unix_ms(1_709_251_200_000);
        assert_eq!((b.year, b.month, b.day, b.hour), (2024, 3, 1, 0));
    }

    #[test]
    fn century_rule() {
        // 1900 was not a leap year, 2000 was: the 29 Feb 1900 gap is 31 days apart.
        let d2000 = civil_from_unix_ms(951_782_400_000); // 2000-03-01
        assert_eq!((d2000.year, d2000.month, d2000.day), (2000, 3, 1));
    }

    #[test]
    fn before_the_epoch_rounds_down_not_toward_zero() {
        // 1969-12-31T23:59:59Z is -1s. Truncating division would land on 1969-12-31T00:00:00.
        let c = civil_from_unix_ms(-1000);
        assert_eq!(c, at(1969, 12, 31, 23, 59, 59));
    }

    #[test]
    fn twelve_hour_boundaries() {
        let o = FaceOptions {
            hour12: true,
            seconds: false,
            meridiem: true,
        };
        assert_eq!(format(&at(2026, 1, 1, 0, 5, 0), &o).as_str(), "12:05");
        assert_eq!(format(&at(2026, 1, 1, 0, 5, 0), &o).suffix_str(), "AM");
        assert_eq!(format(&at(2026, 1, 1, 12, 0, 0), &o).as_str(), "12:00");
        assert_eq!(format(&at(2026, 1, 1, 12, 0, 0), &o).suffix_str(), "PM");
        assert_eq!(format(&at(2026, 1, 1, 23, 59, 59), &o).as_str(), "11:59");
        assert_eq!(format(&at(2026, 1, 1, 23, 59, 59), &o).suffix_str(), "PM");
    }

    #[test]
    fn no_leading_zero_in_twelve_hour_and_always_in_twenty_four() {
        let o12 = FaceOptions {
            hour12: true,
            seconds: false,
            meridiem: false,
        };
        assert_eq!(format(&at(2026, 1, 1, 9, 5, 0), &o12).as_str(), "9:05");
        let o24 = FaceOptions {
            hour12: false,
            seconds: false,
            meridiem: false,
        };
        assert_eq!(format(&at(2026, 1, 1, 9, 5, 0), &o24).as_str(), "09:05");
    }

    #[test]
    fn seconds_extend_the_run_by_exactly_three_glyphs() {
        let c = at(2026, 1, 1, 10, 42, 7);
        let a = format(
            &c,
            &FaceOptions {
                hour12: true,
                seconds: false,
                meridiem: true,
            },
        );
        let b = format(
            &c,
            &FaceOptions {
                hour12: true,
                seconds: true,
                meridiem: true,
            },
        );
        assert_eq!(b.glyph_count() - a.glyph_count(), 3);
        assert_eq!(b.as_str(), "10:42:07");
    }
}
