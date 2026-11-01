//! The clock and a sleep that work on every target: tokio and the system
//! clock natively, browser timers and `Date.now()` on the web.

/// Wall-clock milliseconds since the Unix epoch. Wall time (not monotonic),
/// so timers stay right after the device sleeps; on the web it is
/// `Date.now()`, which Playwright's clock controls.
pub fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as f64)
            .unwrap_or(0.0)
    }
}

/// Waits `ms` milliseconds.
pub async fn sleep_ms(ms: u64) {
    #[cfg(target_arch = "wasm32")]
    {
        gloo_timers::future::TimeoutFuture::new(ms.min(u64::from(u32::MAX)) as u32).await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_after_2024_and_whole_milliseconds() {
        let now = now_ms();
        assert!(now > 1_704_067_200_000.0, "{now}");
        assert_eq!(now.fract(), 0.0);
    }

    #[test]
    fn sleep_waits_at_least_the_given_time() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let started = std::time::Instant::now();
        runtime.block_on(sleep_ms(20));
        assert!(started.elapsed() >= std::time::Duration::from_millis(20));
    }
}
