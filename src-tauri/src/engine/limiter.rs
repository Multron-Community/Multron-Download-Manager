use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

pub struct Limiter {
    rate: AtomicU64,
    state: Mutex<(f64, Instant)>,
}

impl Limiter {
    pub fn new(bytes_per_sec: u64) -> Self {
        Self {
            rate: AtomicU64::new(bytes_per_sec),
            state: Mutex::new((bytes_per_sec as f64, Instant::now())),
        }
    }

    pub fn set_rate(&self, bytes_per_sec: u64) {
        self.rate.store(bytes_per_sec, Ordering::Relaxed);
        *self.state.lock() = (bytes_per_sec as f64, Instant::now());
    }

    pub async fn acquire(&self, n: usize) {
        let rate = self.rate.load(Ordering::Relaxed);
        if rate == 0 || n == 0 {
            return;
        }
        let wait = {
            let mut st = self.state.lock();
            let now = Instant::now();
            let elapsed = now.duration_since(st.1).as_secs_f64();
            let burst = rate as f64;
            st.0 = (st.0 + elapsed * rate as f64).min(burst);
            st.1 = now;
            st.0 -= n as f64;
            if st.0 < 0.0 {
                Duration::from_secs_f64(-st.0 / rate as f64)
            } else {
                Duration::ZERO
            }
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}
