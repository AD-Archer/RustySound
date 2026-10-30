//! Settings → Storage & Downloads, part 1: the cache and Smart Cache Warm-up.
//! Downloads live in `storage_downloads.rs`.

use super::shared::{
    format_mb, use_settings_ctx, ActionRow, Footnote, GroupTitle, NumberRow, SwitchRow, UsageMeter,
};
use super::smart_cache::run_smart_cache;
use super::storage_downloads::DownloadsGroup;
use crate::api::ServerConfig;
use crate::cache_service::{clear_all as clear_cache_storage, stats as current_cache_stats};
use crate::components::ui::{PillButton, PillVariant, Sheet};
use dioxus::prelude::*;

/// (entries, used bytes, max bytes)
type CacheNumbers = (usize, u64, u64);

/// Runs `job` off the UI thread on native builds (filesystem work), inline on web.
pub(super) async fn off_ui_thread<T, F>(job: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static + Default,
{
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::task::spawn_blocking(job).await.unwrap_or_default()
    }
    #[cfg(target_arch = "wasm32")]
    {
        job()
    }
}

#[component]
pub(super) fn StorageSection() -> Element {
    rsx! {
        CacheGroup {}
        DownloadsGroup {}
    }
}

#[component]
fn CacheGroup() -> Element {
    let ctx = use_settings_ctx();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut stats = use_signal(|| None::<CacheNumbers>);
    let mut refresh = use_signal(|| 0u64);
    let mut confirm_clear = use_signal(|| false);
    let warm_busy = use_signal(|| false);
    let warm_progress = use_signal(|| 0u8);
    let warm_status = use_signal(|| None::<String>);

    // Stats are read into a signal after render, refreshed after changes.
    use_effect(move || {
        let _ = refresh();
        let _ = ctx.settings.read().cache_size_mb;
        spawn(async move {
            let s = current_cache_stats();
            stats.set(Some((s.entry_count, s.total_size_bytes as u64, s.max_size_bytes as u64)));
        });
    });

    let settings = ctx.settings.read();
    let enabled = settings.cache_enabled;
    let images = settings.cache_images_enabled;
    let size_mb = settings.cache_size_mb as i64;
    let expiry = settings.cache_expiry_days as i64;
    drop(settings);

    let (usage_label, usage_percent) = match stats() {
        Some((entries, used, max)) => {
            let percent = if max > 0 { used as f64 / max as f64 * 100.0 } else { 0.0 };
            (
                format!("{entries} items · {} of {}", format_mb(used), format_mb(max)),
                percent,
            )
        }
        None => ("Measuring…".to_string(), 0.0),
    };

    let start_warm = move |_| {
        let mut warm_busy = warm_busy;
        let mut warm_status = warm_status;
        if warm_busy() {
            return;
        }
        let active: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
        if active.is_empty() {
            warm_status.set(Some("Turn a server on first: there's nothing to warm.".to_string()));
            return;
        }
        let snapshot = ctx.settings.peek().clone();
        warm_busy.set(true);
        spawn(async move {
            let summary = run_smart_cache(active, snapshot, warm_progress, warm_status).await;
            warm_status.set(Some(summary));
            warm_busy.set(false);
            refresh.with_mut(|n| *n += 1);
        });
    };

    rsx! {
        GroupTitle { title: "Cache".to_string() }
        ul { class: "rs-list rs-list--text",
            SwitchRow {
                id: "cache-enabled".to_string(),
                title: "Cache library data".to_string(),
                subtitle: "Keep songs, artists, playlists, favourites and lyrics on this device.".to_string(),
                checked: enabled,
                on_toggle: move |on| ctx.update_cache(|s| s.cache_enabled = on),
            }
            SwitchRow {
                id: "cache-images".to_string(),
                title: "Cache artwork".to_string(),
                subtitle: "Faster repeat views and fewer artwork requests.".to_string(),
                checked: images,
                on_toggle: move |on| ctx.update_cache(|s| s.cache_images_enabled = on),
            }
            NumberRow {
                id: "cache-size".to_string(),
                title: "Maximum size".to_string(),
                value: size_mb,
                min: 25,
                max: 2048,
                unit: "MB".to_string(),
                on_commit: move |v: i64| ctx.update_cache(|s| s.cache_size_mb = v as u32),
            }
            NumberRow {
                id: "cache-expiry".to_string(),
                title: "Expire after".to_string(),
                subtitle: "Use -1 to keep entries until the cache is full.".to_string(),
                value: expiry,
                min: -1,
                max: 3650,
                unit: "days".to_string(),
                on_commit: move |v: i64| ctx.update_cache(|s| {
                    s.cache_expiry_days = v as i32;
                    s.cache_expiry_in_days = true;
                }),
            }
            li {
                div { class: "rs-row rs-row--compact rs-settings-usage",
                    span { class: "rs-row__meta",
                        span { class: "rs-settings-row__title", "In use" }
                        UsageMeter { label: "Cache usage".to_string(), percent: usage_percent }
                    }
                    span { class: "rs-row__detail", "data-testid": "cache-usage", "{usage_label}" }
                }
            }
            ActionRow {
                label: "Use Recommended".to_string(),
                detail: "100 MB · 30 days".to_string(),
                onclick: move |_| ctx.update_cache(|s| {
                    s.cache_enabled = true;
                    s.cache_images_enabled = true;
                    s.cache_size_mb = 100;
                    s.cache_expiry_days = 30;
                    s.cache_expiry_in_days = true;
                }),
            }
            ActionRow {
                label: "Clear Cache…".to_string(),
                danger: true,
                testid: "cache-clear".to_string(),
                onclick: move |_| confirm_clear.set(true),
            }
        }
        Footnote { "Native apps also prefetch the current and next songs so playback continues through a dropped connection." }

        GroupTitle { title: "Smart Cache Warm-up".to_string() }
        ul { class: "rs-list rs-list--text",
            ActionRow {
                label: if warm_busy() { "Warming…".to_string() } else { "Run Warm-up".to_string() },
                icon: "zap".to_string(),
                busy: warm_busy(),
                testid: "smart-cache-run".to_string(),
                onclick: start_warm,
            }
            if warm_busy() {
                li {
                    div { class: "rs-row rs-row--compact rs-settings-usage",
                        span { class: "rs-row__meta",
                            UsageMeter { label: "Warm-up progress".to_string(), percent: warm_progress() as f64 }
                        }
                        span { class: "rs-row__detail", "{warm_progress}%" }
                    }
                }
            }
        }
        Footnote {
            if let Some(status) = warm_status() {
                "{status}"
            } else {
                "Prefetches albums, songs, playlists, lyrics and artwork for the servers that are on, with pauses between requests. How much it fetches scales with the cache size."
            }
        }

        if confirm_clear() {
            Sheet {
                title: "Clear Cache?".to_string(),
                testid: "cache-clear-confirm".to_string(),
                on_close: move |_| confirm_clear.set(false),
                p { class: "rs-settings-confirm",
                    "Cached library data, lyrics and artwork are removed from this device. Downloads stay. Things load from the server again as you browse."
                }
                div { class: "rs-settings-form__actions",
                    PillButton {
                        label: "Cancel".to_string(),
                        variant: PillVariant::Quiet,
                        onclick: move |_| confirm_clear.set(false),
                    }
                    button {
                        r#type: "button",
                        class: "rs-pill rs-settings-danger-pill",
                        onclick: move |_| {
                            clear_cache_storage();
                            confirm_clear.set(false);
                            refresh.with_mut(|n| *n += 1);
                            ctx.notify("Cache cleared");
                        },
                        "Clear Cache"
                    }
                }
            }
        }
    }
}
