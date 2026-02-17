use std::time::Instant;

#[inline]
pub fn log_perf(scope: &str, started_at: Instant, details: &str) {
    let elapsed_ms = started_at.elapsed().as_millis();
    if details.trim().is_empty() {
        eprintln!("[perf] {scope} took {elapsed_ms}ms");
    } else {
        eprintln!("[perf] {scope} took {elapsed_ms}ms | {details}");
    }
}
