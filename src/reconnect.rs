use std::time::Duration;

#[derive(Default)]
pub struct Retry {
    failures: u32,
}
impl Retry {
    pub fn connected(&mut self) {
        self.failures = 0;
    }
    pub fn next_delay(&mut self, jitter_ms: u64) -> Duration {
        self.failures = (self.failures + 1).min(4);
        Duration::from_millis(250 * (1 << self.failures) + jitter_ms.min(249))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prolonged_outage_stays_bounded_and_success_restores_fast_retry() {
        let mut retry = Retry::default();
        for _ in 0..100 {
            let delay = retry.next_delay(249);
            assert!(delay >= Duration::from_millis(500));
            assert!(delay < Duration::from_secs(5));
        }
        retry.connected();
        assert!(retry.next_delay(249) < Duration::from_secs(1));
    }
}
