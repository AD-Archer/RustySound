//! Downloads: what is saved on this device (native builds only).
//!
//! The download index, collection index and memberships are read once per
//! refresh on a blocking thread into a snapshot signal; render never touches
//! the filesystem. A refresh happens on mount, after delete / clear / Run Now
//! / Refresh, and when a running job finishes. Active downloads are polled
//! (an in-memory list, no filesystem) only while a job is running.

#[cfg(not(target_arch = "wasm32"))]
pub use native::DownloadsView;

/// The web build streams only: one clear empty state.
#[cfg(target_arch = "wasm32")]
#[dioxus::prelude::component]
pub fn DownloadsView() -> dioxus::prelude::Element {
    use crate::components::ui::EmptyState;
    use dioxus::prelude::*;
    rsx! {
        div { class: "rs-view rs-view--narrow", "data-testid": "downloads",
            div { class: "rs-page-head",
                h1 { class: "rs-large-title", "Downloads" }
            }
            EmptyState {
                icon: "download".to_string(),
                title: "Downloads need the app".to_string(),
                message: "Offline downloads are saved on your device by the RustySound app for macOS, iOS, Windows, Linux and Android. In the browser, music streams from your server.".to_string(),
                testid: "downloads-web".to_string(),
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::super::favorites::MoreSentinel;
    use crate::api::{NavidromeClient, ServerConfig, Song};
    use crate::components::audio_manager::normalize_manual_queue_songs;
    use crate::components::ui::{
        use_long_press, ActionMenu, Artwork, EmptyState, IconButton, InsetList, ListInset,
        ListRow, MenuAction, PillButton, PillVariant, SectionHeader, Segmented, Sheet, Skeleton,
        TrackMenuController, VirtualList,
    };
    use crate::components::{
        AddIntent, AddMenuController, AppView, Icon, IsPlayingSignal, Navigation,
    };
    use crate::db::{save_settings, AppSettings};
    use crate::diagnostics::{log_perf, PerfTimer};
    use crate::offline_audio::{
        clear_downloads, list_active_downloads, list_downloaded_collection_memberships,
        list_downloaded_collections, list_downloaded_entries, refresh_downloaded_cache,
        remove_downloaded_album, remove_downloaded_collection, remove_downloaded_song,
        run_auto_download_pass, sync_downloaded_collection_members,
        sync_downloaded_collection_metadata, ActiveDownloadEntry, DownloadCollectionEntry,
        DownloadIndexEntry,
    };
    use dioxus::prelude::*;
    use rand::seq::SliceRandom;
    use std::collections::HashMap;
    use std::rc::Rc;

    const POLL_MS: u64 = 1500;
    const SONGS_WINDOW_AFTER: usize = 80;
    const TRACK_ROW_PX: f64 = 58.0;
    const COLLECTION_PAGE: usize = 60;

    // ---- Snapshot (built off the UI thread) ---------------------------------

    /// One album or playlist with its progress, ready to render.
    #[derive(Clone, PartialEq)]
    pub(super) struct CollectionView {
        pub entry: DownloadCollectionEntry,
        pub downloaded: usize,
        pub total: usize,
        pub cover: Option<String>,
    }

    type CollectionKey = (String, String, String);

    /// Everything the view shows, read from disk in one pass.
    #[derive(Default)]
    pub(super) struct DownloadsSnapshot {
        /// Newest first.
        pub entries: Vec<DownloadIndexEntry>,
        pub lookup: HashMap<(String, String), usize>,
        pub memberships: HashMap<CollectionKey, Vec<String>>,
        pub albums: Vec<CollectionView>,
        pub playlists: Vec<CollectionView>,
        pub total_bytes: u64,
    }

    /// Reads the indexes once. `list_downloaded_entries` stats every file (and
    /// drops entries whose file is gone); it is called exactly once here.
    pub(super) fn read_snapshot() -> DownloadsSnapshot {
        let entries = list_downloaded_entries();
        let collections = list_downloaded_collections();
        let memberships: HashMap<CollectionKey, Vec<String>> =
            list_downloaded_collection_memberships()
                .into_iter()
                .map(|m| ((m.kind, m.server_id, m.collection_id), m.song_ids))
                .collect();
        let lookup: HashMap<(String, String), usize> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| ((e.server_id.clone(), e.song_id.clone()), i))
            .collect();
        let total_bytes = entries.iter().map(|e| e.size_bytes).sum();

        let mut album_covers = HashMap::<(String, String), String>::new();
        for entry in &entries {
            let cover = entry
                .cover_art_id
                .clone()
                .filter(|c| !c.trim().is_empty())
                .or_else(|| entry.album_id.clone());
            if let (Some(album_id), Some(cover)) = (entry.album_id.clone(), cover) {
                album_covers
                    .entry((entry.server_id.clone(), album_id))
                    .or_insert(cover);
            }
        }

        let view = |entry: DownloadCollectionEntry| {
            let key = (
                entry.kind.clone(),
                entry.server_id.clone(),
                entry.collection_id.clone(),
            );
            let member_ids = memberships.get(&key);
            let counted = member_ids.map(|ids| {
                ids.iter()
                    .filter(|id| lookup.contains_key(&(entry.server_id.clone(), id.trim().to_string())))
                    .count()
            });
            let downloaded = counted.unwrap_or_else(|| entry.effective_downloaded_song_count());
            let total = entry.effective_total_song_count().max(downloaded);
            let cover = if entry.kind == "album" {
                album_covers
                    .get(&(entry.server_id.clone(), entry.collection_id.clone()))
                    .cloned()
                    .or_else(|| {
                        (!entry.collection_id.starts_with("name:"))
                            .then(|| entry.collection_id.clone())
                    })
            } else {
                member_ids
                    .and_then(|ids| {
                        ids.iter().find_map(|id| {
                            lookup
                                .get(&(entry.server_id.clone(), id.trim().to_string()))
                                .and_then(|i| {
                                    let e = &entries[*i];
                                    e.cover_art_id.clone().or_else(|| e.album_id.clone())
                                })
                        })
                    })
            };
            CollectionView {
                downloaded: downloaded.min(total),
                total,
                cover,
                entry,
            }
        };

        let mut albums: Vec<DownloadCollectionEntry> = collections
            .iter()
            .filter(|c| c.kind == "album")
            .cloned()
            .collect();
        if albums.is_empty() {
            albums = infer_downloaded_albums(&entries);
        }
        let albums = albums.into_iter().map(view).collect();
        let playlists = collections
            .into_iter()
            .filter(|c| c.kind == "playlist")
            .map(view)
            .collect();

        DownloadsSnapshot {
            entries,
            lookup,
            memberships,
            albums,
            playlists,
            total_bytes,
        }
    }

    fn infer_downloaded_albums(entries: &[DownloadIndexEntry]) -> Vec<DownloadCollectionEntry> {
        let mut map = HashMap::<(String, String), DownloadCollectionEntry>::new();
        for entry in entries {
            let Some(album_name) = entry.album.clone().filter(|name| !name.trim().is_empty())
            else {
                continue;
            };
            let album_key = entry
                .album_id
                .clone()
                .unwrap_or_else(|| format!("name:{album_name}"));
            map.entry((entry.server_id.clone(), album_key.clone()))
                .and_modify(|c| {
                    c.song_count += 1;
                    c.total_song_count += 1;
                    c.downloaded_song_count += 1;
                    c.updated_at_ms = c.updated_at_ms.max(entry.updated_at_ms);
                })
                .or_insert(DownloadCollectionEntry {
                    kind: "album".to_string(),
                    server_id: entry.server_id.clone(),
                    collection_id: album_key,
                    name: album_name,
                    auto_download_tracked: false,
                    song_count: 1,
                    total_song_count: 1,
                    downloaded_song_count: 1,
                    updated_at_ms: entry.updated_at_ms,
                });
        }
        let mut values: Vec<_> = map.into_values().collect();
        values.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
        values
    }

    /// Songs of a collection, in collection order when the membership is known.
    fn collection_entries(snap: &DownloadsSnapshot, c: &DownloadCollectionEntry) -> Vec<usize> {
        let key = (c.kind.clone(), c.server_id.clone(), c.collection_id.clone());
        if let Some(ids) = snap.memberships.get(&key) {
            let mut seen = std::collections::HashSet::new();
            return ids
                .iter()
                .filter(|id| seen.insert(id.trim().to_string()))
                .filter_map(|id| {
                    snap.lookup
                        .get(&(c.server_id.clone(), id.trim().to_string()))
                        .copied()
                })
                .collect();
        }
        if c.kind != "album" {
            return Vec::new();
        }
        let by_name = c.collection_id.starts_with("name:");
        snap.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.server_id == c.server_id
                    && if by_name {
                        e.album.as_deref().map(str::trim) == Some(c.name.as_str())
                    } else {
                        e.album_id.as_deref() == Some(c.collection_id.as_str())
                    }
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub(super) fn format_size(bytes: u64) -> String {
        let mb = bytes as f64 / (1024.0 * 1024.0);
        if mb < 1024.0 {
            format!("{mb:.1} MB")
        } else {
            format!("{:.2} GB", mb / 1024.0)
        }
    }

    fn to_song(entry: &DownloadIndexEntry, servers: &[ServerConfig]) -> Song {
        let server_name = servers
            .iter()
            .find(|s| s.id == entry.server_id)
            .map(|s| s.name.clone())
            .or_else(|| entry.server_name.clone().filter(|n| !n.trim().is_empty()))
            .unwrap_or_else(|| "Offline".to_string());
        Song {
            id: entry.song_id.clone(),
            title: entry.title.clone(),
            album: entry.album.clone(),
            album_id: entry.album_id.clone(),
            artist: entry.artist.clone(),
            cover_art: entry.cover_art_id.clone().or_else(|| entry.album_id.clone()),
            duration: 0,
            server_id: entry.server_id.clone(),
            server_name,
            ..Song::default()
        }
    }

    // ---- Store shared by the view's parts -----------------------------------

    #[derive(Clone, PartialEq)]
    enum PendingDelete {
        Song(DownloadIndexEntry),
        Collection(DownloadCollectionEntry),
        ClearAll,
    }

    #[derive(Clone, Copy, PartialEq)]
    enum SongSort {
        Newest,
        Title,
        Artist,
        Album,
        Size,
    }

    #[derive(Clone, Copy, PartialEq)]
    enum CollectionSort {
        Title,
        Recent,
        Oldest,
    }

    #[derive(Clone, Copy)]
    struct DlStore {
        snapshot: Signal<Option<Rc<DownloadsSnapshot>>>,
        active: Signal<Vec<ActiveDownloadEntry>>,
        busy: Signal<bool>,
        polling: Signal<bool>,
        status: Signal<Option<String>>,
        pending: Signal<Option<PendingDelete>>,
        song_menu: Signal<Option<(DownloadIndexEntry, Option<(f64, f64)>)>>,
        sheet: Signal<Option<DownloadCollectionEntry>>,
        loads: Signal<u32>,
    }

    impl DlStore {
        /// Re-reads the indexes on a blocking thread, then publishes them.
        fn reload(self) {
            let (mut snapshot, mut loads) = (self.snapshot, self.loads);
            spawn(async move {
                let started = PerfTimer::now();
                let snap = tokio::task::spawn_blocking(read_snapshot)
                    .await
                    .unwrap_or_default();
                log_perf(
                    "downloads.snapshot",
                    started,
                    &format!("songs={}", snap.entries.len()),
                );
                snapshot.set(Some(Rc::new(snap)));
                loads.with_mut(|n| *n += 1);
                self.ensure_polling();
            });
        }

        /// Polls the in-memory active list while a job runs (or an action is
        /// busy); reloads the snapshot when downloads finish. Stops when idle.
        fn ensure_polling(self) {
            let (mut active, mut polling) = (self.active, self.polling);
            let now = list_active_downloads();
            if now != *active.peek() {
                active.set(now.clone());
            }
            if *polling.peek() || (now.is_empty() && !*self.busy.peek()) {
                return;
            }
            polling.set(true);
            spawn(async move {
                loop {
                    crate::components::ui::env::sleep_ms(POLL_MS).await;
                    let now = list_active_downloads();
                    let before = active.peek().len();
                    if now != *active.peek() {
                        active.set(now.clone());
                    }
                    if now.len() < before {
                        // Something finished: refresh counts and lists.
                        let snap = tokio::task::spawn_blocking(read_snapshot)
                            .await
                            .unwrap_or_default();
                        let mut snapshot = self.snapshot;
                        snapshot.set(Some(Rc::new(snap)));
                    }
                    if now.is_empty() && !*self.busy.peek() {
                        break;
                    }
                }
                polling.set(false);
            });
        }

        fn say(self, message: impl Into<String>) {
            let mut status = self.status;
            status.set(Some(message.into()));
        }
    }

    // ---- The view ------------------------------------------------------------

    #[component]
    pub fn DownloadsView() -> Element {
        let servers = use_context::<Signal<Vec<ServerConfig>>>();
        let navigation = use_context::<Navigation>();
        let mut app_settings = use_context::<Signal<AppSettings>>();
        let track_menu = use_context::<TrackMenuController>();

        let store = use_context_provider(|| DlStore {
            snapshot: Signal::new(None),
            active: Signal::new(Vec::new()),
            busy: Signal::new(false),
            polling: Signal::new(false),
            status: Signal::new(None),
            pending: Signal::new(None),
            song_menu: Signal::new(None),
            sheet: Signal::new(None),
            loads: Signal::new(0),
        });
        let mut tab = use_signal(|| 0usize);
        let mut header_menu = use_signal(|| None::<(f64, f64)>);

        // First load, then a one-off collection metadata sync.
        use_hook(move || {
            store.reload();
            let servers_snapshot = servers.peek().clone();
            spawn(async move {
                if sync_downloaded_collection_metadata(&servers_snapshot).await > 0 {
                    store.reload();
                }
            });
        });

        // A download started from a track menu elsewhere: pick it up. The
        // change that was already there when the view opened is ignored.
        let seen_change = use_hook(|| track_menu.last_change.peek().clone());
        use_effect(move || {
            let latest = (track_menu.last_change)();
            if latest == seen_change {
                return;
            }
            if let Some(change) = latest {
                if change.downloaded.is_some() {
                    store.reload();
                }
            }
        });

        let mut toggle = move |auto: bool| {
            let mut settings = app_settings();
            if auto {
                settings.auto_downloads_enabled = !settings.auto_downloads_enabled;
            } else {
                settings.downloads_enabled = !settings.downloads_enabled;
            }
            app_settings.set(settings.clone());
            spawn(async move {
                let _ = save_settings(settings).await;
            });
        };

        let run_action = move |key: String| {
            if *store.busy.peek() && key != "settings" && key != "clear" {
                return;
            }
            let mut busy = store.busy;
            match key.as_str() {
                "run" => {
                    let active: Vec<ServerConfig> =
                        servers.peek().iter().filter(|s| s.active).cloned().collect();
                    if active.is_empty() {
                        store.say("No active servers available.");
                        return;
                    }
                    let settings = app_settings.peek().clone();
                    busy.set(true);
                    store.say("Running auto-download…");
                    store.ensure_polling();
                    spawn(async move {
                        match run_auto_download_pass(&active, &settings).await {
                            Ok(r) => store.say(format!(
                                "Auto-download finished: {} new, {} skipped, {} failed, {} purged.",
                                r.downloaded, r.skipped, r.failed, r.purged
                            )),
                            Err(err) => store.say(format!("Auto-download failed: {err}")),
                        }
                        busy.set(false);
                        store.reload();
                    });
                }
                "refresh" => {
                    let servers_snapshot = servers.peek().clone();
                    busy.set(true);
                    store.say("Refreshing downloaded albums and playlists…");
                    spawn(async move {
                        let changed = sync_downloaded_collection_metadata(&servers_snapshot).await;
                        store.say(if changed > 0 {
                            format!("Refresh complete: {changed} collection(s) updated.")
                        } else {
                            "Refresh complete: no changes found.".to_string()
                        });
                        busy.set(false);
                        store.reload();
                    });
                }
                "cache" => {
                    let servers_snapshot = servers.peek().clone();
                    if servers_snapshot.is_empty() {
                        store.say("No servers configured.");
                        return;
                    }
                    let settings = app_settings.peek().clone();
                    busy.set(true);
                    store.say("Refreshing lyrics and artwork for downloads…");
                    spawn(async move {
                        match refresh_downloaded_cache(&servers_snapshot, &settings).await {
                            Ok(r) => store.say(format!(
                                "Refreshed {} songs: {} lyrics saved, {} artwork updated.",
                                r.scanned, r.lyrics_warmed, r.artwork_refreshed
                            )),
                            Err(err) => store.say(format!("Cache refresh failed: {err}")),
                        }
                        busy.set(false);
                        store.reload();
                    });
                }
                "clear" => {
                    let mut pending = store.pending;
                    pending.set(Some(PendingDelete::ClearAll));
                }
                "settings" => navigation.navigate_to(AppView::SettingsView {}),
                _ => {}
            }
        };

        let settings = app_settings.read();
        let (downloads_on, auto_on) = (settings.downloads_enabled, settings.auto_downloads_enabled);
        let (limit_mb, limit_count) = (
            settings.download_limit_mb.max(1),
            settings.download_limit_count.max(1),
        );
        drop(settings);
        let busy = (store.busy)();
        let snapshot = (store.snapshot)();
        let active = (store.active)();
        let status = (store.status)();

        rsx! {
            div { class: "rs-view rs-view--narrow", "data-testid": "downloads",
                div { class: "rs-page-head",
                    h1 { class: "rs-large-title", "Downloads" }
                    div { class: "rs-personal-head-actions",
                        IconButton {
                            icon: "more-horizontal".to_string(),
                            label: "Download options".to_string(),
                            testid: "downloads-menu".to_string(),
                            onclick: move |evt: MouseEvent| {
                                let p = evt.client_coordinates();
                                header_menu.set(Some((p.x - 250.0, p.y + 14.0)));
                            },
                        }
                    }
                }

                match snapshot.clone() {
                    None => rsx! { Skeleton { width: "100%".to_string(), height: "96px".to_string() } },
                    Some(snap) => rsx! {
                        StorageCard {
                            total_bytes: snap.total_bytes,
                            songs: snap.entries.len(),
                            albums: snap.albums.len(),
                            playlists: snap.playlists.len(),
                            limit_mb,
                            limit_count,
                        }
                    },
                }

                InsetList { inset: ListInset::Text, label: "Download settings".to_string(),
                    SwitchRow {
                        title: "Downloads".to_string(),
                        subtitle: "Allow saving songs for offline listening".to_string(),
                        on: downloads_on,
                        testid: "downloads-toggle".to_string(),
                        on_toggle: move |_| toggle(false),
                    }
                    SwitchRow {
                        title: "Auto-Download".to_string(),
                        subtitle: "Fetch favorite songs and recent albums and playlists".to_string(),
                        on: auto_on,
                        testid: "auto-download-toggle".to_string(),
                        on_toggle: move |_| toggle(true),
                    }
                }
                p {
                    class: "rs-list-foot rs-personal-status",
                    role: "status",
                    aria_live: "polite",
                    if let Some(message) = status { "{message}" }
                }

                if !active.is_empty() {
                    SectionHeader { title: format!("Downloading · {}", active.len()) }
                    InsetList { label: "Active downloads".to_string(), testid: "downloads-active".to_string(),
                        for entry in active.iter().take(30) {
                            ListRow {
                                key: "{entry.server_id}:{entry.song_id}",
                                title: entry.title.clone(),
                                subtitle: entry.artist.clone().unwrap_or_else(|| "Unknown artist".to_string()),
                                compact: true,
                                leading: rsx! {
                                    span { class: "rs-row__lead", span { class: "rs-spinner rs-spinner--page" } }
                                },
                            }
                        }
                    }
                }

                SectionHeader { title: "On This Device".to_string() }
                div { class: "rs-personal-bar",
                    Segmented {
                        options: vec!["Songs".to_string(), "Albums".to_string(), "Playlists".to_string()],
                        selected: tab(),
                        label: "Downloaded items".to_string(),
                        testid: "downloads-tabs".to_string(),
                        on_select: move |i| tab.set(i),
                    }
                }

                match (snapshot, tab()) {
                    (None, _) => rsx! { Skeleton { rows: 6 } },
                    (Some(_), 0) => rsx! { SongsTab {} },
                    (Some(_), 1) => rsx! { CollectionsTab { key: "albums-{tab}", playlists: false } },
                    (Some(_), _) => rsx! { CollectionsTab { key: "playlists-{tab}", playlists: true } },
                }
            }

            if let Some(anchor) = header_menu() {
                ActionMenu {
                    title: "Downloads".to_string(),
                    anchor,
                    testid: "downloads-actions".to_string(),
                    actions: vec![
                        MenuAction::new("run", "Run Auto-Download Now", "download").disabled(busy),
                        MenuAction::new("refresh", "Refresh Albums & Playlists", "refresh-cw").disabled(busy),
                        MenuAction::new("cache", "Refresh Lyrics & Artwork", "lyrics").disabled(busy),
                        MenuAction::new("settings", "Download Settings", "settings"),
                        MenuAction::new("clear", "Clear All Downloads…", "trash").danger(),
                    ],
                    on_close: move |_| header_menu.set(None),
                    on_action: run_action,
                }
            }
            // Later in the DOM paints on top: sheet, then its menus, then confirm.
            CollectionSheetHost {}
            SongMenuHost {}
            ConfirmHost {}
        }
    }

    /// Rc wrapper so the snapshot can be a prop (compared by pointer).
    #[derive(Clone)]
    struct SnapRef(Rc<DownloadsSnapshot>);

    impl PartialEq for SnapRef {
        fn eq(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.0, &other.0)
        }
    }

    #[component]
    fn StorageCard(
        total_bytes: u64,
        songs: usize,
        albums: usize,
        playlists: usize,
        limit_mb: u32,
        limit_count: u32,
    ) -> Element {
        let used_mb = total_bytes as f64 / (1024.0 * 1024.0);
        let percent = ((used_mb / limit_mb as f64) * 100.0).clamp(0.0, 100.0);
        let limit = format_size(limit_mb as u64 * 1024 * 1024);
        let used = format_size(total_bytes);
        let plural = |n: usize, one: &str, many: &str| {
            if n == 1 {
                format!("1 {one}")
            } else {
                format!("{n} {many}")
            }
        };
        let limit_songs = plural(limit_count as usize, "song", "songs");
        rsx! {
            section { class: "rs-personal-card", "data-testid": "downloads-storage", aria_label: "Storage",
                div { class: "rs-personal-card__top",
                    span { class: "rs-personal-card__label", "Storage" }
                    span { class: "rs-personal-card__value", "{used} of {limit}" }
                }
                div {
                    class: "rs-personal-meter",
                    role: "meter",
                    aria_label: "Storage used",
                    aria_valuemin: "0",
                    aria_valuemax: "100",
                    aria_valuenow: "{percent:.0}",
                    span { style: "width:{percent:.1}%" }
                }
                div { class: "rs-personal-counts",
                    span { b { "{songs}" } if songs == 1 { " song" } else { " songs" } }
                    span { b { "{albums}" } if albums == 1 { " album" } else { " albums" } }
                    span { b { "{playlists}" } if playlists == 1 { " playlist" } else { " playlists" } }
                    span { "Limit {limit_songs}" }
                }
            }
        }
    }

    #[component]
    fn SwitchRow(
        title: String,
        subtitle: String,
        on: bool,
        testid: String,
        on_toggle: EventHandler<()>,
    ) -> Element {
        rsx! {
            li {
                button {
                    r#type: "button",
                    class: "rs-row",
                    role: "switch",
                    aria_checked: if on { "true" } else { "false" },
                    "data-testid": "{testid}",
                    onclick: move |_| on_toggle.call(()),
                    span { class: "rs-row__meta",
                        span { class: "rs-row__title", span { "{title}" } }
                        span { class: "rs-row__sub", style: "display:block", "{subtitle}" }
                    }
                    span {
                        class: "rs-personal-switch",
                        class: if on { "rs-personal-switch--on" },
                        aria_hidden: "true",
                    }
                }
            }
        }
    }

    // ---- Songs ---------------------------------------------------------------

    fn field(value: &Option<String>) -> String {
        value
            .as_deref()
            .map(|v| v.trim().to_ascii_lowercase())
            .unwrap_or_default()
    }

    #[component]
    fn SongsTab() -> Element {
        let store = use_context::<DlStore>();
        let servers = use_context::<Signal<Vec<ServerConfig>>>();
        let ctl = use_queue_ctl();
        let mut query = use_signal(String::new);
        let mut sort = use_signal(|| SongSort::Title);
        let mut sort_menu = use_signal(|| None::<(f64, f64)>);

        // Filtered, sorted indexes into the snapshot (no entry clones).
        let order = use_memo(move || {
            let Some(snap) = (store.snapshot)() else {
                return Rc::new(Vec::new());
            };
            let q = query().trim().to_ascii_lowercase();
            let es = &snap.entries;
            let mut idx: Vec<usize> = (0..es.len())
                .filter(|&i| {
                    q.is_empty()
                        || es[i].title.to_ascii_lowercase().contains(&q)
                        || field(&es[i].artist).contains(&q)
                        || field(&es[i].album).contains(&q)
                })
                .collect();
            match sort() {
                SongSort::Newest => {}
                SongSort::Title => idx.sort_by_cached_key(|&i| es[i].title.to_ascii_lowercase()),
                SongSort::Artist => idx.sort_by_cached_key(|&i| field(&es[i].artist)),
                SongSort::Album => idx.sort_by_cached_key(|&i| field(&es[i].album)),
                SongSort::Size => idx.sort_by(|&a, &b| es[b].size_bytes.cmp(&es[a].size_bytes)),
            }
            Rc::new(idx)
        });

        let play_from = move |position: usize| {
            let Some(snap) = store.snapshot.peek().clone() else {
                return;
            };
            let order = order.peek().clone();
            let servers = servers.peek().clone();
            let songs: Vec<Song> = order
                .iter()
                .map(|&i| to_song(&snap.entries[i], &servers))
                .collect();
            start_queue(ctl, songs, position, false);
        };

        let total = store
            .snapshot
            .read()
            .as_ref()
            .map(|s| s.entries.len())
            .unwrap_or(0);
        let count = order.read().len();
        let render_row = use_callback(move |position: usize| {
            let entry = {
                let order = order.read();
                let snap = store.snapshot.read();
                match (order.get(position), snap.as_ref()) {
                    (Some(&i), Some(snap)) => snap.entries.get(i).cloned(),
                    _ => None,
                }
            };
            let Some(entry) = entry else {
                return rsx! {};
            };
            rsx! {
                DownloadRow {
                    key: "{entry.server_id}:{entry.song_id}",
                    entry,
                    index: position,
                    on_play: move |_| play_from(position),
                }
            }
        });

        if total == 0 {
            return rsx! {
                EmptyState {
                    icon: "download".to_string(),
                    title: "No downloaded songs".to_string(),
                    message: "Download a song, album or playlist from its menu and it plays from this device, even offline.".to_string(),
                    testid: "downloads-empty".to_string(),
                }
            };
        }

        let current = sort();
        rsx! {
            SearchAndSort {
                query: query(),
                placeholder: "Search downloaded songs".to_string(),
                on_query: move |v| query.set(v),
                on_sort: move |p| sort_menu.set(Some(p)),
            }
            if count == 0 {
                p { class: "rs-list-foot", "No downloaded songs match “{query}”." }
            } else if count > SONGS_WINDOW_AFTER {
                VirtualList {
                    item_count: count,
                    row_height: TRACK_ROW_PX,
                    render_row,
                    class: "rs-list".to_string(),
                    testid: "downloads-songs".to_string(),
                }
            } else {
                InsetList { label: "Downloaded songs".to_string(), testid: "downloads-songs".to_string(),
                    for position in 0..count {
                        {render_row.call(position)}
                    }
                }
            }
            p { class: "rs-list-foot", if count == total { "{total} songs" } else { "{count} of {total} songs" } }
            if let Some(anchor) = sort_menu() {
                ActionMenu {
                    title: "Sort Songs".to_string(),
                    anchor,
                    testid: "downloads-sort".to_string(),
                    actions: vec![
                        MenuAction::new("title", "Title", "").checked(current == SongSort::Title),
                        MenuAction::new("artist", "Artist", "").checked(current == SongSort::Artist),
                        MenuAction::new("album", "Album", "").checked(current == SongSort::Album),
                        MenuAction::new("newest", "Recently Downloaded", "").checked(current == SongSort::Newest),
                        MenuAction::new("size", "Size", "").checked(current == SongSort::Size),
                    ],
                    on_close: move |_| sort_menu.set(None),
                    on_action: move |key: String| sort.set(match key.as_str() {
                        "artist" => SongSort::Artist,
                        "album" => SongSort::Album,
                        "newest" => SongSort::Newest,
                        "size" => SongSort::Size,
                        _ => SongSort::Title,
                    }),
                }
            }
        }
    }

    /// The playback signals the downloads view writes.
    #[derive(Clone, Copy)]
    struct QueueCtl {
        queue: Signal<Vec<Song>>,
        queue_index: Signal<usize>,
        now_playing: Signal<Option<Song>>,
        is_playing: Signal<bool>,
    }

    fn use_queue_ctl() -> QueueCtl {
        QueueCtl {
            queue: use_context(),
            queue_index: use_context(),
            now_playing: use_context(),
            is_playing: use_context::<IsPlayingSignal>().0,
        }
    }

    /// Replaces the queue with `songs`, starting at `start` (or shuffled).
    fn start_queue(ctl: QueueCtl, mut songs: Vec<Song>, start: usize, shuffle: bool) {
        let QueueCtl {
            mut queue,
            mut queue_index,
            mut now_playing,
            mut is_playing,
        } = ctl;
        if songs.is_empty() {
            return;
        }
        let mut start = start.min(songs.len() - 1);
        if shuffle {
            songs.shuffle(&mut rand::thread_rng());
            start = 0;
        }
        now_playing.set(songs.get(start).cloned());
        queue.set(songs);
        queue_index.set(start);
        is_playing.set(true);
    }

    #[component]
    fn SearchAndSort(
        query: String,
        placeholder: String,
        on_query: EventHandler<String>,
        on_sort: EventHandler<(f64, f64)>,
    ) -> Element {
        rsx! {
            div { class: "rs-personal-tools", style: "margin-bottom:var(--sp-5)",
                label { class: "rs-field",
                    Icon { name: "search".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                    input {
                        r#type: "search",
                        placeholder: "{placeholder}",
                        aria_label: "{placeholder}",
                        "data-testid": "downloads-search",
                        value: "{query}",
                        oninput: move |evt| on_query.call(evt.value()),
                    }
                }
                IconButton {
                    icon: "list".to_string(),
                    label: "Sort".to_string(),
                    testid: "downloads-sort-btn".to_string(),
                    onclick: move |evt: MouseEvent| {
                        let p = evt.client_coordinates();
                        on_sort.call((p.x - 240.0, p.y + 14.0));
                    },
                }
            }
        }
    }

    /// A downloaded song: tap plays; "…", right-click or long-press open the
    /// download menu (queue actions, album, delete).
    #[component]
    fn DownloadRow(entry: DownloadIndexEntry, index: usize, on_play: EventHandler<()>) -> Element {
        let store = use_context::<DlStore>();
        let press = use_long_press();
        let open_menu = {
            let entry = entry.clone();
            move |anchor: Option<(f64, f64)>| {
                let mut menu = store.song_menu;
                menu.set(Some((entry.clone(), anchor)));
            }
        };
        let sub = [entry.artist.clone(), entry.album.clone()]
            .into_iter()
            .flatten()
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        let size = format_size(entry.size_bytes);
        rsx! {
            li { "data-index": "{index}",
                div {
                    class: "rs-trow rs-trow--art",
                    role: "button",
                    tabindex: "0",
                    aria_label: "{entry.title}",
                    "data-testid": "download-row",
                    onclick: move |_| {
                        if press.take_fired() {
                            return;
                        }
                        on_play.call(());
                    },
                    onkeydown: {
                        let open_menu = open_menu.clone();
                        move |evt: KeyboardEvent| {
                            let key = evt.key();
                            if key == Key::Enter || key == Key::Character(" ".to_string()) {
                                evt.prevent_default();
                                on_play.call(());
                            } else if key == Key::ContextMenu || (key == Key::F10 && evt.modifiers().shift()) {
                                evt.prevent_default();
                                open_menu(None);
                            }
                        }
                    },
                    oncontextmenu: {
                        let open_menu = open_menu.clone();
                        move |evt: MouseEvent| {
                            evt.prevent_default();
                            press.cancel();
                            let p = evt.client_coordinates();
                            open_menu(Some((p.x, p.y)));
                        }
                    },
                    onpointerdown: {
                        let open_menu = open_menu.clone();
                        move |evt: PointerEvent| {
                            let open_menu = open_menu.clone();
                            press.down(&evt, move |p| open_menu(Some(p)));
                        }
                    },
                    onpointermove: move |evt| press.moved(&evt),
                    onpointerup: move |_| press.cancel(),
                    onpointercancel: move |_| press.cancel(),
                    Artwork {
                        cover_art: entry.cover_art_id.clone().or_else(|| entry.album_id.clone()),
                        server_id: entry.server_id.clone(),
                        size_px: 44,
                    }
                    div { class: "rs-trow__meta",
                        div { class: "rs-trow__title", "{entry.title}" }
                        if !sub.is_empty() {
                            div { class: "rs-trow__sub", "{sub}" }
                        }
                    }
                    span { class: "rs-trow__marks",
                        span { class: "rs-trow__dl", role: "img", aria_label: "Downloaded",
                            Icon { name: "downloaded".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                        }
                    }
                    span { class: "rs-trow__dur", "{size}" }
                    button {
                        r#type: "button",
                        class: "rs-trow__more",
                        aria_label: "More for {entry.title}",
                        aria_haspopup: "menu",
                        "data-testid": "download-row-more",
                        onpointerdown: move |evt| evt.stop_propagation(),
                        onclick: {
                            let open_menu = open_menu.clone();
                            move |evt: MouseEvent| {
                                evt.stop_propagation();
                                let p = evt.client_coordinates();
                                open_menu(Some((p.x - 250.0, p.y + 12.0)));
                            }
                        },
                        Icon { name: "more-horizontal".to_string(), class: "rs-ic rs-ic--md".to_string() }
                    }
                }
            }
        }
    }

    #[component]
    fn SongMenuHost() -> Element {
        let store = use_context::<DlStore>();
        let servers = use_context::<Signal<Vec<ServerConfig>>>();
        let navigation = use_context::<Navigation>();
        let mut add_menu = use_context::<AddMenuController>();
        let ctl = use_queue_ctl();
        let mut song_menu = store.song_menu;
        let Some((entry, anchor)) = song_menu() else {
            return rsx! {};
        };
        let has_album = entry.album_id.is_some();
        let on_action = {
            let entry = entry.clone();
            move |key: String| {
                let song = to_song(&entry, &servers.peek());
                match key.as_str() {
                    "next" | "queue" => enqueue(ctl, song, key == "next"),
                    "playlist" => add_menu.open(AddIntent::from_song(song)),
                    "album" => {
                        if let Some(album_id) = entry.album_id.clone() {
                            navigation.navigate_to(AppView::AlbumDetailView {
                                album_id,
                                server_id: entry.server_id.clone(),
                            });
                        }
                    }
                    "delete" => {
                        let mut pending = store.pending;
                        pending.set(Some(PendingDelete::Song(entry.clone())));
                    }
                    _ => {}
                }
            }
        };
        rsx! {
            ActionMenu {
                title: entry.title.clone(),
                anchor,
                testid: "download-menu".to_string(),
                actions: vec![
                    MenuAction::new("next", "Play Next", "play-next"),
                    MenuAction::new("queue", "Add to Queue", "add-to-queue"),
                    MenuAction::new("playlist", "Add to Playlist…", "playlist"),
                    MenuAction::new("album", "Go to Album", "disc").disabled(!has_album),
                    MenuAction::new("delete", "Delete Download…", "trash").danger(),
                ],
                on_close: move |_| song_menu.set(None),
                on_action,
            }
        }
    }

    fn enqueue(ctl: QueueCtl, song: Song, next: bool) {
        let QueueCtl {
            mut queue,
            mut queue_index,
            mut now_playing,
            mut is_playing,
        } = ctl;
        let songs = normalize_manual_queue_songs(vec![song]);
        if now_playing.peek().is_none() && queue.peek().is_empty() {
            now_playing.set(songs.first().cloned());
            queue.set(songs);
            queue_index.set(0);
            is_playing.set(true);
            return;
        }
        let at = (*queue_index.peek() + 1).min(queue.peek().len());
        queue.with_mut(|items| {
            if next {
                let tail = items.split_off(at);
                items.extend(songs);
                items.extend(tail);
            } else {
                items.extend(songs);
            }
        });
    }

    // ---- Albums and playlists -----------------------------------------------

    #[component]
    fn CollectionsTab(playlists: bool) -> Element {
        let store = use_context::<DlStore>();
        let Some(snap) = (store.snapshot)() else {
            return rsx! {};
        };
        let mut query = use_signal(String::new);
        let mut sort = use_signal(|| CollectionSort::Title);
        let mut sort_menu = use_signal(|| None::<(f64, f64)>);
        let mut limit = use_signal(|| COLLECTION_PAGE);

        let source = if playlists { &snap.playlists } else { &snap.albums };
        let q = query().trim().to_ascii_lowercase();
        let mut items: Vec<&CollectionView> = source
            .iter()
            .filter(|c| q.is_empty() || c.entry.name.to_ascii_lowercase().contains(&q))
            .collect();
        match sort() {
            CollectionSort::Title => items.sort_by_cached_key(|c| c.entry.name.to_ascii_lowercase()),
            CollectionSort::Recent => items.sort_by(|a, b| b.entry.updated_at_ms.cmp(&a.entry.updated_at_ms)),
            CollectionSort::Oldest => items.sort_by(|a, b| a.entry.updated_at_ms.cmp(&b.entry.updated_at_ms)),
        }
        let total = items.len();
        let shown = limit().min(total);
        let rows: Vec<CollectionView> = items.into_iter().take(shown).cloned().collect();
        let noun = if playlists { "playlists" } else { "albums" };
        let current = sort();

        if source.is_empty() {
            return rsx! {
                EmptyState {
                    icon: if playlists { "playlist".to_string() } else { "album".to_string() },
                    title: format!("No downloaded {noun}"),
                    message: format!("Download {} from its page to keep it on this device.", if playlists { "a playlist" } else { "an album" }),
                }
            };
        }

        rsx! {
            SearchAndSort {
                query: query(),
                placeholder: format!("Search downloaded {noun}"),
                on_query: move |v| {
                    query.set(v);
                    limit.set(COLLECTION_PAGE);
                },
                on_sort: move |p| sort_menu.set(Some(p)),
            }
            if rows.is_empty() {
                p { class: "rs-list-foot", "No downloaded {noun} match “{query}”." }
            } else {
                InsetList { label: format!("Downloaded {noun}"), testid: format!("downloads-{noun}"),
                    for c in rows {
                        ListRow {
                            key: "{c.entry.server_id}:{c.entry.collection_id}",
                            title: c.entry.name.clone(),
                            subtitle: if c.downloaded >= c.total {
                                format!("{} songs", c.total)
                            } else {
                                format!("{} of {} songs", c.downloaded, c.total)
                            },
                            chevron: true,
                            testid: "download-collection".to_string(),
                            leading: rsx! {
                                Artwork {
                                    cover_art: c.cover.clone(),
                                    server_id: c.entry.server_id.clone(),
                                    size_px: 44,
                                    placeholder: if playlists { "playlist".to_string() } else { "album".to_string() },
                                }
                            },
                            onclick: {
                                let entry = c.entry.clone();
                                move |_| {
                                    let mut sheet = store.sheet;
                                    sheet.set(Some(entry.clone()));
                                }
                            },
                        }
                    }
                }
                if shown < total {
                    MoreSentinel {
                        key: "{shown}",
                        on_visible: move |_| limit.with_mut(|n| *n += COLLECTION_PAGE),
                    }
                }
            }
            if let Some(anchor) = sort_menu() {
                ActionMenu {
                    title: format!("Sort {}", if playlists { "Playlists" } else { "Albums" }),
                    anchor,
                    actions: vec![
                        MenuAction::new("title", "Name", "").checked(current == CollectionSort::Title),
                        MenuAction::new("recent", "Recently Downloaded", "").checked(current == CollectionSort::Recent),
                        MenuAction::new("oldest", "Oldest First", "").checked(current == CollectionSort::Oldest),
                    ],
                    on_close: move |_| sort_menu.set(None),
                    on_action: move |key: String| sort.set(match key.as_str() {
                        "recent" => CollectionSort::Recent,
                        "oldest" => CollectionSort::Oldest,
                        _ => CollectionSort::Title,
                    }),
                }
            }
        }
    }

    #[component]
    fn CollectionSheetHost() -> Element {
        let store = use_context::<DlStore>();
        let mut sheet = store.sheet;
        let Some(collection) = sheet() else {
            return rsx! {};
        };
        let Some(snap) = (store.snapshot)() else {
            return rsx! {};
        };
        rsx! {
            CollectionSheet {
                key: "{collection.kind}:{collection.server_id}:{collection.collection_id}",
                collection,
                snap: SnapRef(snap),
                on_close: move |_| sheet.set(None),
            }
        }
    }

    #[component]
    fn CollectionSheet(
        collection: DownloadCollectionEntry,
        snap: SnapRef,
        on_close: EventHandler<()>,
    ) -> Element {
        let store = use_context::<DlStore>();
        let servers = use_context::<Signal<Vec<ServerConfig>>>();
        let navigation = use_context::<Navigation>();
        let mut add_menu = use_context::<AddMenuController>();
        let ctl = use_queue_ctl();
        let is_playlist = collection.kind == "playlist";

        // Playlists downloaded before memberships were tracked: fetch once.
        use_hook({
            let collection = collection.clone();
            let known = snap.0.memberships.contains_key(&(
                collection.kind.clone(),
                collection.server_id.clone(),
                collection.collection_id.clone(),
            ));
            move || {
                if !is_playlist || known {
                    return;
                }
                let server = servers.peek().iter().find(|s| s.id == collection.server_id).cloned();
                let Some(server) = server else { return };
                spawn(async move {
                    let client = NavidromeClient::new(server);
                    if let Ok((_, songs)) = client.get_playlist(&collection.collection_id).await {
                        sync_downloaded_collection_members(
                            "playlist",
                            &collection.server_id,
                            &collection.collection_id,
                            &songs,
                        );
                        store.reload();
                    }
                });
            }
        });

        let indexes = collection_entries(&snap.0, &collection);
        let view = (if is_playlist { &snap.0.playlists } else { &snap.0.albums })
            .iter()
            .find(|c| {
                c.entry.server_id == collection.server_id
                    && c.entry.collection_id == collection.collection_id
            })
            .cloned();
        let (downloaded, total, cover) = view
            .map(|v| (v.downloaded, v.total, v.cover))
            .unwrap_or((indexes.len(), indexes.len(), None));
        let summary = if downloaded >= total {
            format!("{} · {total} songs downloaded", if is_playlist { "Playlist" } else { "Album" })
        } else {
            format!(
                "{} · {downloaded} of {total} songs downloaded",
                if is_playlist { "Playlist" } else { "Album" }
            )
        };
        let songs: Rc<Vec<Song>> = Rc::new(
            indexes
                .iter()
                .map(|&i| to_song(&snap.0.entries[i], &servers.peek()))
                .collect(),
        );
        let entries: Vec<DownloadIndexEntry> =
            indexes.iter().map(|&i| snap.0.entries[i].clone()).collect();
        let can_open = !(collection.kind == "album" && collection.collection_id.starts_with("name:"));

        let play = {
            let songs = songs.clone();
            move |start: usize, shuffle: bool| start_queue(ctl, (*songs).clone(), start, shuffle)
        };

        rsx! {
            Sheet {
                title: collection.name.clone(),
                testid: "download-collection-sheet".to_string(),
                on_close: move |_| on_close.call(()),
                div { class: "rs-personal-sheet-head",
                    Artwork {
                        cover_art: cover,
                        server_id: collection.server_id.clone(),
                        size_px: 72,
                        placeholder: if is_playlist { "playlist".to_string() } else { "album".to_string() },
                    }
                    div { class: "rs-personal-sheet-head__meta",
                        strong { "{collection.name}" }
                        "{summary}"
                    }
                }
                div { class: "rs-personal-sheet-pills",
                    PillButton {
                        label: "Play".to_string(),
                        icon: "play".to_string(),
                        block: true,
                        disabled: songs.is_empty(),
                        testid: "download-collection-play".to_string(),
                        onclick: {
                            let play = play.clone();
                            move |_| play(0, false)
                        },
                    }
                    PillButton {
                        label: "Shuffle".to_string(),
                        icon: "shuffle".to_string(),
                        variant: PillVariant::Quiet,
                        block: true,
                        disabled: songs.is_empty(),
                        onclick: {
                            let play = play.clone();
                            move |_| play(0, true)
                        },
                    }
                }
                InsetList { inset: ListInset::Icon, label: "Actions".to_string(),
                    ListRow {
                        title: "Add to Playlist…".to_string(),
                        icon: "playlist".to_string(),
                        compact: true,
                        onclick: {
                            let songs = songs.clone();
                            let name = collection.name.clone();
                            move |_| {
                                on_close.call(());
                                add_menu.open(AddIntent::from_songs(name.clone(), (*songs).clone()));
                            }
                        },
                    }
                    if can_open {
                        ListRow {
                            title: if is_playlist { "Go to Playlist".to_string() } else { "Go to Album".to_string() },
                            icon: if is_playlist { "playlist".to_string() } else { "disc".to_string() },
                            compact: true,
                            onclick: {
                                let c = collection.clone();
                                move |_| {
                                    on_close.call(());
                                    navigation.navigate_to(if is_playlist {
                                        AppView::PlaylistDetailView {
                                            playlist_id: c.collection_id.clone(),
                                            server_id: c.server_id.clone(),
                                        }
                                    } else {
                                        AppView::AlbumDetailView {
                                            album_id: c.collection_id.clone(),
                                            server_id: c.server_id.clone(),
                                        }
                                    });
                                }
                            },
                        }
                    }
                    li {
                        button {
                            r#type: "button",
                            class: "rs-row rs-row--compact rs-personal-action--danger",
                            "data-testid": "download-collection-delete",
                            onclick: {
                                let c = collection.clone();
                                move |_| {
                                    let mut pending = store.pending;
                                    pending.set(Some(PendingDelete::Collection(c.clone())));
                                }
                            },
                            span { class: "rs-row__lead rs-personal-action--danger",
                                Icon { name: "trash".to_string(), class: "rs-ic rs-ic--md".to_string() }
                            }
                            span { class: "rs-row__meta", span { class: "rs-row__title", span { "Delete Download…" } } }
                        }
                    }
                }
                SectionHeader { title: "Songs".to_string() }
                if entries.is_empty() {
                    p { class: "rs-list-foot", "No songs from this collection are on this device yet." }
                } else {
                    InsetList { label: "Songs".to_string(),
                        for (position, entry) in entries.into_iter().enumerate() {
                            DownloadRow {
                                key: "{entry.server_id}:{entry.song_id}",
                                entry,
                                index: position,
                                on_play: {
                                    let play = play.clone();
                                    move |_| play(position, false)
                                },
                            }
                        }
                    }
                }
            }
        }
    }

    // ---- Confirmation -------------------------------------------------------

    #[component]
    fn ConfirmHost() -> Element {
        let store = use_context::<DlStore>();
        let mut pending = store.pending;
        let Some(action) = pending() else {
            return rsx! {};
        };
        let (title, message) = match &action {
            PendingDelete::Song(e) => (
                "Delete Download?".to_string(),
                format!("“{}” will be removed from this device. It stays on your server.", e.title),
            ),
            PendingDelete::Collection(c) => (
                format!("Delete {}?", if c.kind == "album" { "Album" } else { "Playlist" }),
                format!("The downloaded songs of “{}” will be removed from this device.", c.name),
            ),
            PendingDelete::ClearAll => (
                "Clear All Downloads?".to_string(),
                "Every downloaded song, album and playlist will be removed from this device.".to_string(),
            ),
        };
        let confirm = move |_key: String| {
            let mut sheet = store.sheet;
            match action.clone() {
                PendingDelete::Song(e) => {
                    let _ = remove_downloaded_song(&e.server_id, &e.song_id);
                    store.say(format!("Removed “{}”.", e.title));
                }
                PendingDelete::Collection(c) => {
                    if c.kind == "album" {
                        let removed = remove_downloaded_album(&c.server_id, &c.collection_id, &c.name);
                        store.say(format!("Removed {removed} songs of “{}”.", c.name));
                    } else {
                        let _ = remove_downloaded_collection(&c.kind, &c.server_id, &c.collection_id);
                        store.say(format!("Removed playlist “{}” from downloads.", c.name));
                    }
                    sheet.set(None);
                }
                PendingDelete::ClearAll => {
                    let removed = clear_downloads();
                    sheet.set(None);
                    store.say(format!("Removed {removed} downloaded songs."));
                }
            }
            store.reload();
        };
        rsx! {
            ActionMenu {
                title,
                message,
                testid: "downloads-confirm".to_string(),
                actions: vec![MenuAction::new("delete", "Delete", "trash").danger()],
                on_close: move |_| pending.set(None),
                on_action: confirm,
            }
        }
    }

    #[cfg(test)]
    mod perf {
        //! `cargo test --features desktop --no-default-features downloads_refresh_cost -- --ignored --nocapture`
        //! Builds a fake index of 500 songs under a temporary HOME and times one
        //! refresh: the old render path (download_stats + list_downloaded_entries
        //! + collections + memberships, per render) against the new snapshot.
        use super::*;
        use crate::offline_audio::download_stats;
        use std::time::Instant;

        #[test]
        #[ignore]
        fn downloads_refresh_cost() {
            let home = std::env::temp_dir().join(format!("rs-dl-perf-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).unwrap();
            std::env::set_var("HOME", &home);
            std::env::set_var("XDG_CACHE_HOME", home.join(".cache"));
            let cache = dirs::cache_dir().unwrap().join("rustysound").join("audio_cache");
            std::fs::create_dir_all(&cache).unwrap();
            let mut index = Vec::new();
            for i in 0..500u32 {
                let ext = if i % 3 == 0 { "flac" } else { "mp3" };
                std::fs::write(cache.join(format!("srv__song{i}.{ext}")), b"x").unwrap();
                index.push(DownloadIndexEntry {
                    server_id: "srv".into(),
                    song_id: format!("song{i}"),
                    title: format!("Song {i}"),
                    artist: Some(format!("Artist {}", i % 40)),
                    album: Some(format!("Album {}", i % 50)),
                    album_id: Some(format!("al{}", i % 50)),
                    size_bytes: 8_000_000,
                    updated_at_ms: i as u64,
                    ..Default::default()
                });
            }
            std::fs::write(cache.join("download_index.json"), serde_json::to_string(&index).unwrap())
                .unwrap();

            let runs = 20;
            let t = Instant::now();
            for _ in 0..runs {
                let stats = download_stats();
                let entries = list_downloaded_entries();
                let _ = list_active_downloads();
                let _ = list_downloaded_collections();
                let _ = list_downloaded_collection_memberships();
                assert_eq!(stats.song_count, 500);
                assert_eq!(entries.len(), 500);
            }
            let old_ms = t.elapsed().as_secs_f64() * 1000.0 / runs as f64;
            let t = Instant::now();
            for _ in 0..runs {
                let snap = read_snapshot();
                assert_eq!(snap.entries.len(), 500);
            }
            let new_ms = t.elapsed().as_secs_f64() * 1000.0 / runs as f64;
            println!("old render path: {old_ms:.2} ms per render (on the UI thread)");
            println!("new snapshot:    {new_ms:.2} ms per refresh (blocking thread)");
            let _ = std::fs::remove_dir_all(&home);
        }
    }
}
