//! Monotonic deltas and wall-clock reads, and nothing else.

use crate::sys;

/// Millisecond clock for frame deltas. `QueryPerformanceCounter` because `GetTickCount` advances on
/// a 10–16 ms quantum, and a 240 Hz accumulator driven by it would step unevenly and show up as
/// judder in the swing.
#[derive(Clone, Copy)]
pub struct PerfClock {
    freq: i64,
    last: i64,
}

impl PerfClock {
    #[must_use]
    pub fn new() -> Self {
        let mut freq = 0i64;
        let mut now = 0i64;
        unsafe {
            if sys::QueryPerformanceFrequency(&mut freq) == 0 || freq <= 0 {
                freq = 1000;
            }
            sys::QueryPerformanceCounter(&mut now);
        }
        Self { freq, last: now }
    }

    /// Seconds since the previous call. Also the "dt" for the tick that just happened.
    pub fn tick(&mut self) -> f64 {
        let mut now = 0i64;
        unsafe {
            sys::QueryPerformanceCounter(&mut now);
        }
        let dt = (now - self.last) as f64 / self.freq as f64;
        self.last = now;
        if dt.is_finite() && dt > 0.0 {
            dt.min(1.0)
        } else {
            0.0
        }
    }
}

impl Default for PerfClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Local wall time for the face. Two APIs on purpose:
///
/// *   `GetLocalTime` yields the *displayed* civil fields, so the face never disagrees with the
///     clock in the corner of the screen, DST transitions included — which is the only promise a
///     desktop clock is judged on.
/// *   `GetSystemTimeAsFileTime` yields the exact instant, used for the seconds-alignment timer and
///     for a stopwatch, where fields are the wrong representation.
#[must_use]
pub fn local_fields() -> (u32, u32, u32, u32, u32, u32) {
    let mut st = sys::SYSTEMTIME::default();
    unsafe {
        sys::GetLocalTime(&mut st);
    }
    (
        st.year as u32,
        st.month as u32,
        st.day as u32,
        st.hour as u32,
        st.minute as u32,
        st.second as u32,
    )
}

/// 100 ns intervals since 1601-01-01, the raw FILETIME value.
#[must_use]
pub fn filetime_100ns() -> i64 {
    let mut ft = sys::FILETIME { low: 0, high: 0 };
    unsafe {
        sys::GetSystemTimeAsFileTime(&mut ft);
    }
    ((ft.high as i64) << 32) | ft.low as i64
}

/// Unix milliseconds, UTC.
#[must_use]
pub fn unix_ms() -> i64 {
    // 116444736000000000 = 100 ns ticks between 1601-01-01 and 1970-01-01.
    (filetime_100ns() - 116_444_736_000_000_000) / 10_000
}

/// Local offset from UTC in seconds, DST included.
///
/// `GetTimeZoneInformation` reports `bias` in minutes and *subtracts* it to reach UTC, so the sign
/// here is the negation of the raw field. Getting this backwards is the classic one-off-hours bug in
/// hand-rolled clocks, and it is why the conversion lives in one named function rather than inline.
#[must_use]
pub fn local_offset_secs() -> i32 {
    let mut tzi = sys::TIME_ZONE_INFORMATION::default();
    let rc = unsafe { sys::GetTimeZoneInformation(&mut tzi) };
    let mut bias = tzi.bias;
    // TIME_ZONE_ID_DAYLIGHT == 2: daylight bias is additive on top of the standard bias.
    if rc == 2 {
        bias += tzi.daylight_bias;
    }
    -bias * 60
}

pub const TIME_ZONE_ID_DAYLIGHT: u32 = 2;
