pub struct Time;

impl Time {
    pub fn milliseconds(unscaled_time: u64, timescale: u32) -> u64 {
        let milliseconds = u128::from(unscaled_time) * 1_000 / u128::from(timescale);
        u64::try_from(milliseconds).unwrap_or(u64::MAX)
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
}
