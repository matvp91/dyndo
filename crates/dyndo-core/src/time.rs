pub struct Time;

impl Time {
    pub fn milliseconds(unscaled_time: u64, timescale: u32) -> u64 {
        let milliseconds = u128::from(unscaled_time) * 1_000 / u128::from(timescale);
        u64::try_from(milliseconds).unwrap_or(u64::MAX)
    }

    pub fn milliseconds_ceil(unscaled_time: u64, timescale: u32) -> u64 {
        let milliseconds = u128::from(unscaled_time) * 1_000;
        let milliseconds = milliseconds.div_ceil(u128::from(timescale));
        u64::try_from(milliseconds).unwrap_or(u64::MAX)
    }

    pub fn milliseconds_rounded(unscaled_time: u64, timescale: u32) -> u64 {
        let timescale = u128::from(timescale);
        let milliseconds = (u128::from(unscaled_time) * 1_000 + timescale / 2) / timescale;
        u64::try_from(milliseconds).unwrap_or(u64::MAX)
    }

    pub fn seconds_rounded(unscaled_time: u64, timescale: u32) -> u64 {
        let timescale = u128::from(timescale);
        let seconds = (u128::from(unscaled_time) + timescale / 2) / timescale;
        u64::try_from(seconds).unwrap_or(u64::MAX)
    }

    pub fn ticks_from_milliseconds(milliseconds: u32, timescale: u64) -> u64 {
        let ticks = u128::from(milliseconds) * u128::from(timescale) / 1_000;
        u64::try_from(ticks).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::Time;

    #[test]
    fn milliseconds_truncates_fractional_milliseconds() {
        assert_eq!(Time::milliseconds(1, 90_000), 0);
    }

    #[test]
    fn milliseconds_converts_using_the_timescale() {
        assert_eq!(Time::milliseconds(90_000, 90_000), 1_000);
    }

    #[test]
    fn milliseconds_saturates_when_the_result_does_not_fit() {
        assert_eq!(Time::milliseconds(u64::MAX, 1), u64::MAX);
    }

    #[test]
    fn milliseconds_ceil_rounds_fractional_milliseconds_up() {
        assert_eq!(Time::milliseconds_ceil(1, 90_000), 1);
    }

    #[test]
    fn milliseconds_rounded_rounds_to_the_nearest_millisecond() {
        assert_eq!(Time::milliseconds_rounded(45, 90_000), 1);
    }

    #[test]
    fn seconds_rounded_rounds_to_the_nearest_second() {
        assert_eq!(Time::seconds_rounded(1_500, 1_000), 2);
    }

    #[test]
    fn ticks_from_milliseconds_uses_the_target_timescale() {
        assert_eq!(Time::ticks_from_milliseconds(500, 90_000), 45_000);
    }
}
