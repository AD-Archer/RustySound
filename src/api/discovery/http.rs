//! HTTP plumbing for discovery: a shared client, timeouts that work on wasm and
//! native, and a request budget so one Discover load can't fan out unbounded.

use futures_util::future::{select, Either};
use once_cell::sync::Lazy;
use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Timeout for one external (Last.fm / iTunes / ListenBrainz) request.
pub const EXTERNAL_TIMEOUT_MS: u64 = 6_000;
/// Timeout for one request to the user's own server.
pub const SERVER_TIMEOUT_MS: u64 = 10_000;

static EXTERNAL_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

/// Sleep that works on both wasm and native.
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

/// Run `fut`, giving up after `ms` milliseconds (`None` on timeout).
pub async fn with_timeout<F: Future>(ms: u64, fut: F) -> Option<F::Output> {
    let fut = Box::pin(fut);
    let timer = Box::pin(sleep_ms(ms));
    match select(fut, timer).await {
        Either::Left((output, _)) => Some(output),
        Either::Right(_) => None,
    }
}

/// Run a server call with [`SERVER_TIMEOUT_MS`], folding timeouts into `Err`.
pub async fn server_call<T, F>(fut: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    with_timeout(SERVER_TIMEOUT_MS, fut)
        .await
        .unwrap_or_else(|| Err("Server request timed out".to_string()))
}

/// A shared, decrementing request allowance. Cloning shares the same counter.
#[derive(Debug, Clone)]
pub struct Budget {
    remaining: Arc<AtomicU32>,
    used: Arc<AtomicU32>,
}

impl Budget {
    pub fn new(limit: u32) -> Self {
        Self {
            remaining: Arc::new(AtomicU32::new(limit)),
            used: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Reserve one request. Returns false when the budget is spent.
    pub fn take(&self) -> bool {
        let taken = self
            .remaining
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| left.checked_sub(1))
            .is_ok();
        if taken {
            self.used.fetch_add(1, Ordering::SeqCst);
        }
        taken
    }

    /// Requests reserved so far.
    pub fn used(&self) -> u32 {
        self.used.load(Ordering::SeqCst)
    }
}

/// GET a JSON document from an external API, within the budget and timeout.
///
/// Non-2xx responses still return their JSON body when it parses (Last.fm puts
/// `{"error":10,"message":…}` in a 403). HTTP 204 returns `Value::Null`.
/// Errors never include the URL (it may contain an API key).
pub async fn get_external_json(url: &str, budget: &Budget) -> Result<serde_json::Value, String> {
    if crate::cache_service::is_offline_mode() {
        return Err("Offline mode".to_string());
    }
    if !budget.take() {
        return Err("Request budget spent".to_string());
    }
    let request = async {
        #[allow(unused_mut)]
        let mut builder = EXTERNAL_CLIENT.get(url);
        #[cfg(not(target_arch = "wasm32"))]
        {
            builder = builder.header("User-Agent", "RustySound (Discover)");
        }
        let response = builder.send().await.map_err(|_| "Network error".to_string())?;
        let status = response.status();
        if status.as_u16() == 204 {
            return Ok(serde_json::Value::Null);
        }
        let text = response
            .text()
            .await
            .map_err(|_| "Could not read response".to_string())?;
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) => Ok(value),
            Err(_) if !status.is_success() => Err(format!("HTTP {}", status.as_u16())),
            Err(_) => Err("Invalid JSON".to_string()),
        }
    };
    with_timeout(EXTERNAL_TIMEOUT_MS, request)
        .await
        .unwrap_or_else(|| Err("Timed out".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_caps_requests() {
        let budget = Budget::new(2);
        let shared = budget.clone();
        assert!(budget.take());
        assert!(shared.take());
        assert!(!budget.take());
        assert_eq!(budget.used(), 2);
    }

    #[test]
    fn timeout_fires_on_native() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let slow = runtime.block_on(with_timeout(10, sleep_ms(5_000)));
        assert!(slow.is_none());
        let fast = runtime.block_on(with_timeout(1_000, async { 7 }));
        assert_eq!(fast, Some(7));
    }
}
