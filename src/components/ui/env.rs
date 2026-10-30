//! Shared kit plumbing: layout, platform, theme mapping, timers, art URLs.

use crate::api::{NavidromeClient, ServerConfig};
use crate::db::AppSettings;
use dioxus::prelude::*;

/// Width below which the app uses the compact (tab bar) layout.
pub const COMPACT_MAX_WIDTH: f64 = 760.0;

/// Which navigation tree the shell renders. Only one tree is ever mounted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layout {
    /// Phone-sized: tab bar plus floating mini player.
    Compact,
    /// Desktop/tablet: sidebar plus the full player bar.
    Wide,
}

impl Layout {
    pub fn attr(self) -> &'static str {
        match self {
            Layout::Compact => "compact",
            Layout::Wide => "wide",
        }
    }

    pub fn is_compact(self) -> bool {
        self == Layout::Compact
    }
}

/// Context handle for the resize-aware layout signal (provided by the shell).
#[derive(Clone, Copy)]
pub struct LayoutSignal(pub Signal<Layout>);

/// Current layout. Subscribes the caller, so it re-renders on a layout flip
/// (rare) and never on ordinary resizes.
pub fn use_layout() -> Layout {
    match try_use_context::<LayoutSignal>() {
        Some(signal) => (signal.0)(),
        None => default_layout(),
    }
}

/// The layout to assume before the first measurement arrives.
pub fn default_layout() -> Layout {
    #[cfg(target_arch = "wasm32")]
    {
        let width = web_sys::window()
            .and_then(|w| w.inner_width().ok())
            .and_then(|v| v.as_f64())
            .unwrap_or(1024.0);
        if width < COMPACT_MAX_WIDTH {
            Layout::Compact
        } else {
            Layout::Wide
        }
    }
    #[cfg(all(not(target_arch = "wasm32"), any(target_os = "ios", target_os = "android")))]
    {
        Layout::Compact
    }
    #[cfg(all(
        not(target_arch = "wasm32"),
        not(any(target_os = "ios", target_os = "android"))
    ))]
    {
        Layout::Wide
    }
}

const LAYOUT_PROBE_JS: &str = r#"
const report = () => dioxus.send([window.innerWidth || 0, Math.min(screen.width || 0, screen.height || 0)]);
let frame = 0;
window.addEventListener("resize", () => {
  cancelAnimationFrame(frame);
  frame = requestAnimationFrame(report);
});
window.addEventListener("orientationchange", report);
report();
await new Promise(() => {});
"#;

fn layout_for(width: f64, short_side: f64) -> Layout {
    // iPhones stay compact in landscape too; iPads follow the width rule.
    let is_ios_phone = cfg!(target_os = "ios") && short_side > 0.0 && short_side < 600.0;
    if is_ios_phone || (width > 0.0 && width < COMPACT_MAX_WIDTH) {
        Layout::Compact
    } else {
        Layout::Wide
    }
}

/// Creates the layout signal and keeps it in step with the window size.
/// The listener is a resize event (rAF-coalesced), not a poll, and the signal
/// is written only when the layout actually flips.
pub fn use_layout_provider() -> Signal<Layout> {
    let mut layout = use_signal(default_layout);
    use_hook(move || {
        spawn(async move {
            let mut probe = document::eval(LAYOUT_PROBE_JS);
            while let Ok((width, short_side)) = probe.recv::<(f64, f64)>().await {
                let next = layout_for(width, short_side);
                if *layout.peek() != next {
                    layout.set(next);
                }
            }
        });
    });
    layout
}

/// `data-platform` value for this build.
pub fn platform_name() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "web"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "android") {
        "android"
    } else {
        "linux"
    }
}

/// The 96px app mark as a `data:` URI, for `<img src>`. Inlined because
/// desktop builds don't resolve `asset!` paths for images (they showed broken).
pub fn app_mark_src() -> &'static str {
    static MARK: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MARK.get_or_init(|| {
        use base64::Engine;
        let png = include_bytes!("../../../assets/favicon-96x96.png");
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        )
    })
}

/// Maps a stored theme name (including the 11 retired ones) to a Stage theme.
pub fn stage_theme(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "graphite" | "fluent" | "material" => "graphite",
        "tape" | "y2k" => "tape",
        // stage, hig, fruit, aqua, aero, spot, navi, rusty, dark, "" and anything unknown
        _ => "stage",
    }
}

/// `data-appearance` value: "system", "light" or "dark".
///
/// Reads `AppSettings::appearance`; anything unknown (or an old settings file
/// without the field) follows the system. This is the only place the shell asks.
pub fn appearance_attr(settings: &AppSettings) -> &'static str {
    match settings.appearance.trim().to_ascii_lowercase().as_str() {
        "light" => "light",
        "dark" => "dark",
        _ => "system",
    }
}

/// Builds a cover-art URL for `cover_art` on the server `server_id`, at the
/// given pixel size. Returns None when the server or cover is unknown.
pub fn cover_art_url(
    servers: &[ServerConfig],
    server_id: &str,
    cover_art: Option<&str>,
    size_px: u32,
) -> Option<String> {
    let cover_art = cover_art.filter(|value| !value.trim().is_empty())?;
    let server = servers.iter().find(|server| server.id == server_id)?;
    let url = NavidromeClient::new(server.clone()).get_cover_art_url(cover_art, size_px);
    if url.is_empty() {
        None
    } else {
        Some(url)
    }
}

/// Cross-platform timer for UI delays (long-press, pulses).
pub async fn sleep_ms(ms: u64) {
    #[cfg(target_arch = "wasm32")]
    {
        gloo_timers::future::TimeoutFuture::new(ms as u32).await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }
}

/// `m:ss` / `h:mm:ss` for a number of seconds.
pub fn format_clock(seconds: f64) -> String {
    let total = if seconds.is_finite() && seconds > 0.0 {
        seconds.floor() as u64
    } else {
        0
    };
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// True for internet-radio entries (no seeking, no queue actions).
pub fn is_radio_song(song: &crate::api::Song) -> bool {
    song.server_name == "Radio"
}
