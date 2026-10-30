//! Stats: per-server library counts and scan status (with Quick Scan), the
//! server list, and "Most Played" from the listening data the API already
//! exposes (frequent albums, songs by play count).

use crate::api::*;
use crate::components::ui::env::sleep_ms;
use crate::components::ui::{
    Artwork, EmptyState, InsetList, ListInset, ListRow, Button, SectionHeader, Skeleton,
    TrackRow,
};
use crate::components::{AppView, IsPlayingSignal, Navigation};
use dioxus::prelude::*;

const SCAN_POLL_MS: u64 = 2000;
/// Give up following a scan after this many polls (~10 minutes).
const SCAN_POLL_LIMIT: u32 = 300;
const TOP_ALBUMS: usize = 8;
const TOP_SONGS: usize = 10;

#[derive(Clone, PartialEq, Default)]
struct ServerStats {
    songs: Option<u64>,
    albums: Option<u64>,
    artists: Option<usize>,
    playlists: Option<usize>,
    scan: Option<ScanStatus>,
}

async fn load_server_stats(server: ServerConfig) -> ServerStats {
    let client = NavidromeClient::new(server);
    let scan = client.get_scan_status().await.ok();
    let artists = client.get_artists().await.ok();
    let playlists = client.get_playlists().await.ok();
    ServerStats {
        songs: scan.as_ref().and_then(|s| s.count),
        // getArtists lists album artists with their album counts.
        albums: artists
            .as_ref()
            .map(|list| list.iter().map(|a| u64::from(a.album_count)).sum()),
        artists: artists.map(|list| list.len()),
        playlists: playlists.map(|list| list.len()),
        scan,
    }
}

fn number(value: Option<u64>) -> String {
    let Some(value) = value else {
        return "—".to_string();
    };
    let digits = value.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// "just now", "12 minutes ago", "3 hours ago", "4 days ago", or the date.
fn ago(iso: &str) -> Option<String> {
    let then = chrono::DateTime::parse_from_rfc3339(iso).ok()?;
    let secs = (chrono::Utc::now() - then.with_timezone(&chrono::Utc)).num_seconds().max(0);
    Some(match secs {
        0..=59 => "just now".to_string(),
        60..=3599 => match secs / 60 {
            1 => "1 minute ago".to_string(),
            m => format!("{m} minutes ago"),
        },
        3600..=86_399 => match secs / 3600 {
            1 => "1 hour ago".to_string(),
            h => format!("{h} hours ago"),
        },
        86_400..=2_591_999 => match secs / 86_400 {
            1 => "yesterday".to_string(),
            d => format!("{d} days ago"),
        },
        _ => then.format("%b %-d, %Y").to_string(),
    })
}

fn scan_line(scan: &Option<ScanStatus>) -> (String, Option<String>) {
    let Some(scan) = scan else {
        return ("Unavailable".to_string(), None);
    };
    if is_running(&Some(scan.clone())) {
        let detail = scan
            .count
            .map(|n| format!("{} songs so far", number(Some(n))))
            .or_else(|| scan.current_task.clone());
        return ("Scanning…".to_string(), detail);
    }
    let last = scan
        .last_scan
        .as_deref()
        .and_then(ago)
        .map(|when| format!("Last scan {when}"));
    let status = if scan.status == "unknown" { "Unknown".to_string() } else { "Idle".to_string() };
    (status, last)
}

type StatsRows = Signal<Vec<(ServerConfig, Option<ServerStats>)>>;

fn is_running(scan: &Option<ScanStatus>) -> bool {
    scan.as_ref()
        .map(|s| s.scanning == Some(true) || s.status == "running" || s.status == "scanning")
        .unwrap_or(false)
}

fn set_scan(mut stats: StatsRows, server_id: &str, status: ScanStatus) {
    stats.with_mut(|list| {
        if let Some((_, Some(slot))) = list.iter_mut().find(|(s, _)| s.id == server_id) {
            slot.scan = Some(status);
        }
    });
}

/// Polls the scan status only while a scan runs, then reloads the counts.
async fn follow_scan(server: ServerConfig, mut stats: StatsRows) {
    let client = NavidromeClient::new(server.clone());
    for _ in 0..SCAN_POLL_LIMIT {
        sleep_ms(SCAN_POLL_MS).await;
        let Ok(status) = client.get_scan_status().await else { break };
        let running = is_running(&Some(status.clone()));
        set_scan(stats, &server.id, status);
        if !running {
            break;
        }
    }
    let id = server.id.clone();
    let loaded = load_server_stats(server).await;
    stats.with_mut(|list| {
        if let Some(slot) = list.iter_mut().find(|(s, _)| s.id == id) {
            slot.1 = Some(loaded);
        }
    });
}

#[component]
pub fn StatsView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<IsPlayingSignal>().0;

    let mut stats = use_signal(Vec::<(ServerConfig, Option<ServerStats>)>::new);
    let mut top_albums = use_signal(|| None::<Vec<Album>>);
    let mut top_songs = use_signal(|| None::<Vec<Song>>);
    let mut scanning = use_signal(std::collections::HashSet::<String>::new);
    let mut note = use_signal(|| None::<String>);

    use_effect(move || {
        let active: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
        stats.set(active.iter().cloned().map(|s| (s, None)).collect());
        for server in active.clone() {
            spawn(async move {
                let id = server.id.clone();
                let loaded = load_server_stats(server.clone()).await;
                let running = is_running(&loaded.scan);
                stats.with_mut(|list| {
                    if let Some(slot) = list.iter_mut().find(|(s, _)| s.id == id) {
                        slot.1 = Some(loaded);
                    }
                });
                // A scan already in progress: follow it until it ends.
                if running {
                    follow_scan(server, stats).await;
                }
            });
        }
        let album_servers = active.clone();
        spawn(async move {
            let mut albums = Vec::new();
            for server in album_servers {
                if let Ok(mut list) = NavidromeClient::new(server)
                    .get_albums("frequent", TOP_ALBUMS as u32, 0)
                    .await
                {
                    albums.append(&mut list);
                }
            }
            albums.sort_by(|a, b| b.play_count.unwrap_or(0).cmp(&a.play_count.unwrap_or(0)));
            albums.retain(|a| a.play_count.unwrap_or(1) > 0);
            albums.truncate(TOP_ALBUMS);
            top_albums.set(Some(albums));
        });
        spawn(async move {
            let mut songs = Vec::new();
            for server in active {
                if let Ok(mut list) = NavidromeClient::new(server)
                    .get_native_songs(NativeSongSortField::PlayCount, NativeSortOrder::Desc, 0, TOP_SONGS)
                    .await
                {
                    songs.append(&mut list);
                }
            }
            songs.sort_by(|a, b| b.play_count.unwrap_or(0).cmp(&a.play_count.unwrap_or(0)));
            songs.retain(|s| s.play_count.unwrap_or(0) > 0);
            songs.truncate(TOP_SONGS);
            top_songs.set(Some(songs));
        });
    });

    let mut quick_scan = move |server: ServerConfig| {
        if scanning.peek().contains(&server.id) {
            return;
        }
        scanning.with_mut(|set| {
            set.insert(server.id.clone());
        });
        note.set(None);
        spawn(async move {
            let client = NavidromeClient::new(server.clone());
            match client.start_scan().await {
                Ok(status) => {
                    set_scan(stats, &server.id, status);
                    follow_scan(server.clone(), stats).await;
                }
                Err(err) => note.set(Some(format!("Couldn't start a scan on {}: {err}", server.name))),
            }
            scanning.with_mut(|set| {
                set.remove(&server.id);
            });
        });
    };

    let all_servers = servers();
    let rows = stats();
    let busy = scanning();
    let current = now_playing
        .read()
        .as_ref()
        .map(|s| (s.server_id.clone(), s.id.clone()));
    let live = is_playing();
    let multi = rows.len() > 1;

    rsx! {
        div { class: "rs-view rs-view--narrow", "data-testid": "stats",
            div { class: "rs-page-head",
                h1 { class: "rs-large-title", "Stats" }
            }

            if all_servers.is_empty() {
                EmptyState {
                    icon: "stats".to_string(),
                    title: "No servers connected".to_string(),
                    message: "Add a Navidrome server to see what's in your library.".to_string(),
                    Button {
                        label: "Add Server".to_string(),
                        onclick: move |_| navigation.navigate_to(AppView::SettingsView {}),
                    }
                }
            } else {
                if let Some(message) = note() {
                    p { class: "rs-personal-error", role: "alert", "{message}" }
                }
                for (server, loaded) in rows {
                    section { key: "{server.id}", aria_label: "{server.name}",
                    SectionHeader { title: if multi { server.name.clone() } else { "Library".to_string() } }
                    match loaded {
                        None => rsx! { Skeleton { rows: 4 } },
                        Some(s) => {
                            let (scan_status, scan_detail) = scan_line(&s.scan);
                            let is_scanning = busy.contains(&server.id) || is_running(&s.scan);
                            let server_for_scan = server.clone();
                            rsx! {
                                InsetList { inset: ListInset::Icon, label: format!("{} counts", server.name), testid: "stats-counts".to_string(),
                                    ListRow { title: "Songs".to_string(), icon: "music".to_string(), detail: number(s.songs), compact: true }
                                    ListRow { title: "Albums".to_string(), icon: "album".to_string(), detail: number(s.albums), compact: true }
                                    ListRow { title: "Artists".to_string(), icon: "artist".to_string(), detail: number(s.artists.map(|n| n as u64)), compact: true }
                                    ListRow { title: "Playlists".to_string(), icon: "playlist".to_string(), detail: number(s.playlists.map(|n| n as u64)), compact: true }
                                }
                                InsetList { inset: ListInset::Icon, label: format!("{} scan", server.name), testid: "stats-scan".to_string(),
                                    ListRow {
                                        title: "Library Scan".to_string(),
                                        subtitle: scan_detail,
                                        icon: "refresh-cw".to_string(),
                                        detail: scan_status,
                                        compact: true,
                                    }
                                    li {
                                        button {
                                            r#type: "button",
                                            class: "rs-row rs-row--compact rs-personal-action",
                                            disabled: is_scanning,
                                            "data-testid": "stats-quick-scan",
                                            onclick: move |_| quick_scan(server_for_scan.clone()),
                                            span { class: "rs-row__lead",
                                                if is_scanning { span { class: "rs-spinner rs-spinner--page" } } else { crate::components::Icon { name: "zap".to_string(), class: "rs-ic rs-ic--md".to_string() } }
                                            }
                                            span { class: "rs-row__meta",
                                                span { class: "rs-row__title",
                                                    span { if is_scanning { "Scanning…" } else { "Quick Scan" } }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    }
                }

                SectionHeader { title: "Servers".to_string() }
                InsetList { inset: ListInset::Icon, label: "Servers".to_string(),
                    for server in all_servers.iter() {
                        ListRow {
                            key: "{server.id}",
                            title: server.name.clone(),
                            subtitle: server.url.clone(),
                            detail: if server.active { "Active".to_string() } else { "Inactive".to_string() },
                            leading: rsx! {
                                span { class: "rs-row__lead",
                                    span { class: if server.active { "rs-led" } else { "rs-led rs-led--off" } }
                                }
                            },
                        }
                    }
                }

                match (top_albums(), top_songs()) {
                    (None, _) | (_, None) => rsx! {},
                    (Some(albums), Some(songs)) => {
                        let shared = std::rc::Rc::new(songs.clone());
                        rsx! {
                        if !albums.is_empty() {
                            SectionHeader { title: "Most Played Albums".to_string() }
                            InsetList { label: "Most played albums".to_string(), testid: "stats-top-albums".to_string(),
                                for album in albums {
                                    ListRow {
                                        key: "{album.server_id}:{album.id}",
                                        title: album.name.clone(),
                                        subtitle: album.artist.clone(),
                                        detail: album.play_count.map(|n| if n == 1 { "1 play".to_string() } else { format!("{} plays", number(Some(u64::from(n)))) }),
                                        leading: rsx! {
                                            Artwork {
                                                cover_art: album.cover_art.clone(),
                                                server_id: album.server_id.clone(),
                                                size_px: 44,
                                                placeholder: "album".to_string(),
                                            }
                                        },
                                        onclick: {
                                            let album = album.clone();
                                            move |_| navigation.navigate_to(AppView::AlbumDetailView {
                                                album_id: album.id.clone(),
                                                server_id: album.server_id.clone(),
                                            })
                                        },
                                    }
                                }
                            }
                        }
                        if !songs.is_empty() {
                            SectionHeader { title: "Most Played Songs".to_string() }
                            InsetList { label: "Most played songs".to_string(), testid: "stats-top-songs".to_string(),
                                for (index, song) in songs.iter().enumerate() {
                                    {
                                        let playing = current
                                            .as_ref()
                                            .map(|(server, id)| *server == song.server_id && *id == song.id)
                                            .unwrap_or(false);
                                        let plays = song.play_count.unwrap_or(0);
                                        let artist = song.artist.clone().unwrap_or_default();
                                        let subtitle = if artist.is_empty() {
                                            format!("{plays} plays")
                                        } else {
                                            format!("{artist} · {plays} plays")
                                        };
                                        let list = shared.clone();
                                        rsx! {
                                            TrackRow {
                                                key: "{song.server_id}:{song.id}",
                                                song: song.clone(),
                                                subtitle,
                                                index,
                                                playing,
                                                live: playing && live,
                                                on_play: move |_| {
                                                    now_playing.set(list.get(index).cloned());
                                                    queue.set((*list).clone());
                                                    queue_index.set(index);
                                                    is_playing.set(true);
                                                },
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::number;

    #[test]
    fn numbers_get_thousands_separators() {
        assert_eq!(number(Some(0)), "0");
        assert_eq!(number(Some(999)), "999");
        assert_eq!(number(Some(1000)), "1,000");
        assert_eq!(number(Some(1234567)), "1,234,567");
        assert_eq!(number(None), "—");
    }
}
