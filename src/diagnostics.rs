#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug)]
pub struct PerfTimer {
    started_at: std::time::Instant,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug)]
pub struct PerfTimer {
    started_at_ms: f64,
}

impl PerfTimer {
    #[inline]
    pub fn now() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            return Self {
                started_at: std::time::Instant::now(),
            };
        }

        #[cfg(target_arch = "wasm32")]
        {
            Self {
                started_at_ms: js_sys::Date::now(),
            }
        }
    }

    #[inline]
    fn elapsed_ms(self) -> u128 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            return self.started_at.elapsed().as_millis();
        }

        #[cfg(target_arch = "wasm32")]
        {
            (js_sys::Date::now() - self.started_at_ms).max(0.0).round() as u128
        }
    }
}

#[inline]
pub fn log_perf(scope: &str, started_at: PerfTimer, details: &str) {
    let elapsed_ms = started_at.elapsed_ms();
    if details.trim().is_empty() {
        eprintln!("[perf] {scope} took {elapsed_ms}ms");
    } else {
        eprintln!("[perf] {scope} took {elapsed_ms}ms | {details}");
    }
}
