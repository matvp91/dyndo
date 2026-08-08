//! Milliseconds and a track's own clock.
//!
//! A track counts time in timescale units while manifests and requests speak
//! milliseconds, and no single rounding is right for both. Which way a conversion goes
//! decides whether a buffer is advertised too small or a duration points past the end
//! of the media, so every caller names the direction it needs rather than inheriting
//! one.
//!
//! Comparisons stay out of here. Asking which edge a boundary falls on is exact —
//! see [`boundary_utils`](crate::boundary_utils) — and converting first would let a
//! boundary round onto an edge that sits before it.
//!
//! A timescale of zero is refused when a track is probed, so nothing here divides by
//! it.

pub struct ClockUtils;

impl ClockUtils {
    /// `raw` in milliseconds, rounded down.
    ///
    /// For a value read as a limit, where claiming less time than exists is the safe
    /// error: a presentation's duration must not point a player past its media.
    pub fn millis_floor(raw: u64, timescale: u32) -> u64 {
        millis(u128::from(raw) * 1000 / u128::from(timescale))
    }

    /// `raw` in milliseconds, rounded up.
    ///
    /// For a value read as a guarantee, where claiming more time than exists is the
    /// safe error: a buffer has to cover the segment it was sized for.
    pub fn millis_ceil(raw: u64, timescale: u32) -> u64 {
        millis((u128::from(raw) * 1000).div_ceil(u128::from(timescale)))
    }

    /// `raw` in the nearest millisecond, halves away from zero.
    ///
    /// For a value read as a measurement rather than a bound, where being wrong in
    /// either direction costs the same.
    pub fn millis_nearest(raw: u64, timescale: u32) -> u64 {
        let timescale = u128::from(timescale);

        millis((u128::from(raw) * 1000 + timescale / 2) / timescale)
    }

    /// `millis` in timescale units, rounded down — the edge or frame at or before the
    /// time asked for, never past it.
    pub fn raw_floor(millis: u64, timescale: u32) -> u64 {
        let raw = u128::from(millis) * u128::from(timescale) / 1000;

        u64::try_from(raw).unwrap_or(u64::MAX)
    }
}

fn millis(scaled: u128) -> u64 {
    u64::try_from(scaled).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A timescale that divides 1000 converts exactly, so every rounding agrees and
    /// the direction only shows up where it does not.
    #[test]
    fn an_exact_conversion_rounds_the_same_either_way() {
        assert_eq!(
            (
                ClockUtils::millis_floor(90_000, 90_000),
                ClockUtils::millis_ceil(90_000, 90_000),
                ClockUtils::millis_nearest(90_000, 90_000)
            ),
            (1_000, 1_000, 1_000)
        );
    }

    #[test]
    fn millis_floor_drops_the_remainder() {
        assert_eq!(ClockUtils::millis_floor(3_001, 3_000), 1_000);
    }

    #[test]
    fn millis_ceil_keeps_the_remainder() {
        assert_eq!(ClockUtils::millis_ceil(3_001, 3_000), 1_001);
    }

    #[test]
    fn millis_nearest_rounds_halves_up() {
        assert_eq!(
            (
                ClockUtils::millis_nearest(1_500, 1_000_000),
                ClockUtils::millis_nearest(1_499, 1_000_000)
            ),
            (2, 1)
        );
    }

    #[test]
    fn raw_floor_lands_at_or_before_the_time_asked_for() {
        assert_eq!(
            (
                ClockUtils::raw_floor(1_000, 90_000),
                ClockUtils::raw_floor(5, 44_100)
            ),
            (90_000, 220)
        );
    }

    /// The conversions widen to 128 bits, so a track no clock could describe saturates
    /// rather than wrapping into a plausible-looking time.
    #[test]
    fn an_unrepresentable_conversion_saturates() {
        assert_eq!(ClockUtils::millis_floor(u64::MAX, 1), u64::MAX);
    }
}
