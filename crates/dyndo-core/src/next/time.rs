pub struct Time;

impl Time {
    pub fn milliseconds(unscaled_time: u64, timescale: u32) -> u64 {
        let milliseconds = u128::from(unscaled_time) * 1_000 / u128::from(timescale);
        u64::try_from(milliseconds).unwrap_or(u64::MAX)
    }
}
