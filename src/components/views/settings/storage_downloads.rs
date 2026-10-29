//! Settings → Storage & Downloads, part 2: downloads, auto-download and limits.

use super::shared::{
    format_mb, use_settings_ctx, ActionRow, Footnote, GroupTitle, NumberRow, SelectRow, SwitchRow,
    UsageMeter,
};
use super::storage::off_ui_thread;
use crate::api::ServerConfig;
use crate::components::ui::{PillButton, PillVariant, Sheet};
use crate::components::{AppView, Navigation};
use crate::db::ArtworkDownloadPreference;
use crate::offline_audio::{
    clear_downloads, download_stats, prune_temporary_queue_prefetch_downloads,
    refresh_downloaded_cache, run_auto_download_pass,
};
use dioxus::prelude::*;

fn artwork_pref_key(pref: ArtworkDownloadPreference) -> &'static str {
    match pref {
        ArtworkDownloadPreference::ServerOnly => "server_only",
        ArtworkDownloadPreference::Id3Only => "id3_only",
        ArtworkDownloadPreference::PreferServer => "prefer_server",
        ArtworkDownloadPreference::PreferId3 => "prefer_id3",
    }
}

fn parse_artwork_pref(value: &str) -> ArtworkDownloadPreference {
    match value {
        "server_only" => ArtworkDownloadPreference::ServerOnly,
        "id3_only" => ArtworkDownloadPreference::Id3Only,
        "prefer_id3" => ArtworkDownloadPreference::PreferId3,
        _ => ArtworkDownloadPreference::PreferServer,
    }
}

fn options(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Job {
    Idle,
    AutoDownload,
    Refresh,
}

#[component]
pub(super) fn DownloadsGroup() -> Element {
    let ctx = use_settings_ctx();
    let navigation = use_context::<Navigation>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    // (songs, bytes), read off the UI thread.
    let mut stats = use_signal(|| None::<(usize, u64)>);
    let mut refresh = use_signal(|| 0u64);
    let mut job = use_signal(|| Job::Idle);
    let mut status = use_signal(|| None::<String>);
    let mut confirm_clear = use_signal(|| false);

    use_effect(move || {
        let _ = refresh();
        spawn(async move {
            let snapshot = off_ui_thread(|| {
                let s = download_stats();
                (s.song_count, s.total_size_bytes)
            })
            .await;
            stats.set(Some(snapshot));
        });
    });

    let settings = ctx.settings.read().clone();
    let favorite_limit = match settings.auto_download_tier {
        3 => 150,
        2 => 100,
        _ => 50,
    };
    let (usage_label, usage_percent) = match stats() {
        Some((songs, bytes)) => {
            let limit_bytes = settings.download_limit_mb.max(1) as f64 * 1024.0 * 1024.0;
            (
                format!(
                    "{songs} songs · {} of {} MB",
                    format_mb(bytes),
                    settings.download_limit_mb
                ),
                bytes as f64 / limit_bytes * 100.0,
            )
        }
        None => ("Measuring…".to_string(), 0.0),
    };
    let busy = job() != Job::Idle;

    let run_auto = move |_| {
        if job() != Job::Idle {
            return;
        }
        let active: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
        if active.is_empty() {
            status.set(Some("Turn a server on first.".to_string()));
            return;
        }
        let snapshot = ctx.settings.peek().clone();
        job.set(Job::AutoDownload);
        status.set(Some("Running auto-download…".to_string()));
        spawn(async move {
            status.set(Some(match run_auto_download_pass(&active, &snapshot).await {
                Ok(r) => format!(
                    "Auto-download finished: {} new, {} skipped, {} failed, {} removed.",
                    r.downloaded, r.skipped, r.failed, r.purged
                ),
                Err(error) => format!("Auto-download failed: {error}"),
            }));
            job.set(Job::Idle);
            refresh.with_mut(|n| *n += 1);
        });
    };

    let run_refresh = move |_| {
        if job() != Job::Idle {
            return;
        }
        let all = servers();
        if all.is_empty() {
            status.set(Some("Add a server first.".to_string()));
            return;
        }
        let snapshot = ctx.settings.peek().clone();
        job.set(Job::Refresh);
        status.set(Some("Refreshing lyrics and artwork for downloads…".to_string()));
        spawn(async move {
            status.set(Some(match refresh_downloaded_cache(&all, &snapshot).await {
                Ok(r) => {
                    let missing = if r.missing_servers > 0 {
                        format!(" {} downloads belong to servers that are gone.", r.missing_servers)
                    } else {
                        String::new()
                    };
                    format!(
                        "Refreshed {} downloads: {} lyrics ({} tried), {} artwork.{missing}",
                        r.scanned, r.lyrics_warmed, r.lyrics_attempted, r.artwork_refreshed
                    )
                }
                Err(error) => format!("Refresh failed: {error}"),
            }));
            job.set(Job::Idle);
            refresh.with_mut(|n| *n += 1);
        });
    };

    rsx! {
        GroupTitle { title: "Downloads".to_string() }
        if cfg!(target_arch = "wasm32") {
            Footnote { "Downloads work in the Mac, iPhone and desktop apps. These settings sync when you use one." }
        }
        ul { class: "rs-list rs-list--text",
            SwitchRow {
                id: "downloads-enabled".to_string(),
                title: "Downloads".to_string(),
                subtitle: "Allow saving songs and lyrics for offline listening.".to_string(),
                checked: settings.downloads_enabled,
                on_toggle: move |on| ctx.update(|s| s.downloads_enabled = on),
            }
            li {
                div { class: "rs-row rs-row--compact rs-settings-usage",
                    span { class: "rs-row__meta",
                        span { class: "rs-settings-row__title", "Downloaded" }
                        UsageMeter { label: "Download storage used".to_string(), percent: usage_percent }
                    }
                    span { class: "rs-row__detail", "data-testid": "downloads-usage", "{usage_label}" }
                }
            }
            ActionRow {
                label: "Open Downloads".to_string(),
                icon: "download".to_string(),
                onclick: move |_| navigation.navigate_to(AppView::DownloadsView {}),
            }
        }

        GroupTitle { title: "Auto-Download".to_string() }
        ul { class: "rs-list rs-list--text",
            SwitchRow {
                id: "auto-downloads".to_string(),
                title: "Auto-download".to_string(),
                subtitle: "Fetch favourite songs and recent albums and playlists in the background.".to_string(),
                checked: settings.auto_downloads_enabled,
                on_toggle: move |on: bool| {
                    ctx.update(|s| s.auto_downloads_enabled = on);
                    if !on {
                        let removed = prune_temporary_queue_prefetch_downloads(5);
                        if removed > 0 {
                            status.set(Some(format!("Removed {removed} temporary queue downloads.")));
                            refresh.with_mut(|n| *n += 1);
                        }
                    }
                },
            }
            SelectRow {
                id: "auto-download-tier".to_string(),
                title: "Favourites".to_string(),
                subtitle: format!("Keeps up to {favorite_limit} favourite songs."),
                value: settings.auto_download_tier.to_string(),
                options: options(&[("1", "50 songs"), ("2", "100 songs"), ("3", "150 songs")]),
                on_change: move |v: String| {
                    if let Ok(tier) = v.parse::<u8>() {
                        ctx.update(|s| s.auto_download_tier = tier.clamp(1, 3));
                    }
                },
            }
            NumberRow {
                id: "auto-download-albums".to_string(),
                title: "Recent albums".to_string(),
                value: settings.auto_download_album_count as i64,
                min: 0,
                max: 25,
                on_commit: move |v: i64| ctx.update(|s| s.auto_download_album_count = v as u32),
            }
            NumberRow {
                id: "auto-download-playlists".to_string(),
                title: "Recent playlists".to_string(),
                value: settings.auto_download_playlist_count as i64,
                min: 0,
                max: 25,
                on_commit: move |v: i64| ctx.update(|s| s.auto_download_playlist_count = v as u32),
            }
            SelectRow {
                id: "artwork-pref".to_string(),
                title: "Artwork source".to_string(),
                subtitle: "Big artwork warm-ups can hit server rate limits if repeated often.".to_string(),
                value: artwork_pref_key(settings.artwork_download_preference).to_string(),
                options: options(&[
                    ("prefer_server", "Prefer server"),
                    ("prefer_id3", "Prefer embedded (ID3)"),
                    ("server_only", "Server only"),
                    ("id3_only", "Embedded only"),
                ]),
                on_change: move |v: String| ctx.update(|s| s.artwork_download_preference = parse_artwork_pref(&v)),
            }
        }

        GroupTitle { title: "Limits".to_string() }
        ul { class: "rs-list rs-list--text",
            NumberRow {
                id: "download-limit-count".to_string(),
                title: "Most songs".to_string(),
                value: settings.download_limit_count as i64,
                min: 25,
                max: 20_000,
                on_commit: move |v: i64| ctx.update(|s| s.download_limit_count = v as u32),
            }
            NumberRow {
                id: "download-limit-mb".to_string(),
                title: "Most space".to_string(),
                value: settings.download_limit_mb as i64,
                min: 256,
                max: 131_072,
                step: 256,
                unit: "MB".to_string(),
                on_commit: move |v: i64| ctx.update(|s| s.download_limit_mb = v as u32),
            }
            ActionRow {
                label: "Use Recommended".to_string(),
                detail: "15 + 15 · 6 GB".to_string(),
                onclick: move |_| ctx.update(|s| {
                    s.downloads_enabled = true;
                    s.auto_downloads_enabled = true;
                    s.auto_download_album_count = 15;
                    s.auto_download_playlist_count = 15;
                    s.download_limit_count = 5000;
                    s.download_limit_mb = 6000;
                }),
            }
        }

        ul { class: "rs-list rs-list--text rs-settings-gap",
            ActionRow {
                label: "Run Auto-Download Now".to_string(),
                icon: "download".to_string(),
                busy: job() == Job::AutoDownload,
                disabled: busy,
                testid: "auto-download-run".to_string(),
                onclick: run_auto,
            }
            ActionRow {
                label: "Refresh Downloaded Lyrics & Artwork".to_string(),
                icon: "refresh-cw".to_string(),
                busy: job() == Job::Refresh,
                disabled: busy,
                onclick: run_refresh,
            }
            ActionRow {
                label: "Clear Downloads…".to_string(),
                danger: true,
                disabled: busy,
                testid: "downloads-clear".to_string(),
                onclick: move |_| confirm_clear.set(true),
            }
        }
        if let Some(message) = status() {
            p { class: "rs-list-foot rs-settings-foot", role: "status", "{message}" }
        }

        if confirm_clear() {
            Sheet {
                title: "Clear Downloads?".to_string(),
                testid: "downloads-clear-confirm".to_string(),
                on_close: move |_| confirm_clear.set(false),
                p { class: "rs-settings-confirm",
                    "Every downloaded song is deleted from this device. You'll need a connection to play them again."
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
                            confirm_clear.set(false);
                            spawn(async move {
                                let removed = off_ui_thread(clear_downloads).await;
                                status.set(Some(format!("Removed {removed} downloaded songs.")));
                                refresh.with_mut(|n| *n += 1);
                            });
                        },
                        "Clear Downloads"
                    }
                }
            }
        }
    }
}
