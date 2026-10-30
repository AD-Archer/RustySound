//! Plumbing shared by the Library views: playback in context, list marks
//! (now playing, downloaded, starred), debounced search, offset paging and
//! the loading / empty / error states.

use crate::api::{Playlist, QueueSourceKind, ServerConfig, Song};
use crate::components::audio_manager::{
    apply_collection_shuffle_mode, assign_collection_queue_meta, normalize_manual_queue_songs,
};
use crate::components::ui::env::sleep_ms;
use crate::components::ui::{EmptyState, Button, ButtonVariant, Skeleton, TrackMenuController};
use crate::components::{IsPlayingSignal, ShuffleEnabledSignal};
use crate::db::AppSettings;
use dioxus::prelude::*;
use std::collections::HashSet;

/// `server::id`, the identity of a song across servers.
pub fn song_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
}

/// "32 min", "1 hr 12 min". `seconds` is Subsonic seconds.
pub fn format_total(seconds: u64) -> String {
    let minutes = (seconds + 30) / 60;
    if minutes >= 60 {
        let (h, m) = (minutes / 60, minutes % 60);
        if m == 0 {
            format!("{h} hr")
        } else {
            format!("{h} hr {m} min")
        }
    } else {
        format!("{} min", minutes.max(if seconds > 0 { 1 } else { 0 }))
    }
}

/// "8 songs, 32 min".
pub fn songs_summary(count: usize, seconds: u64) -> String {
    let noun = if count == 1 { "song" } else { "songs" };
    if seconds == 0 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}, {}", format_total(seconds))
    }
}

/// Playlists imported by Navidrome from .m3u files can't be edited.
pub fn is_auto_imported(playlist: &Playlist) -> bool {
    playlist
        .comment
        .as_deref()
        .map(|c| c.to_lowercase().contains("auto-imported"))
        .unwrap_or(false)
}

pub fn active_servers(servers: &[ServerConfig]) -> Vec<ServerConfig> {
    servers.iter().filter(|s| s.active).cloned().collect()
}

/// The configured servers as a memo: readers re-run only when the list
/// really changes, not every time the shell rewrites the signal.
pub fn use_servers() -> Memo<Vec<ServerConfig>> {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    use_memo(move || servers())
}

pub fn offline_now(settings: &AppSettings) -> bool {
    settings.offline_mode || crate::cache_service::is_offline_mode()
}

// ---- Playback ---------------------------------------------------------------

/// The playback signals a list needs to start music.
#[derive(Clone, Copy)]
pub struct Player {
    pub queue: Signal<Vec<Song>>,
    pub queue_index: Signal<usize>,
    pub now_playing: Signal<Option<Song>>,
    pub is_playing: Signal<bool>,
    pub shuffle: Signal<bool>,
    pub settings: Signal<AppSettings>,
}

pub fn use_player() -> Player {
    Player {
        queue: use_context::<Signal<Vec<Song>>>(),
        queue_index: use_context::<Signal<usize>>(),
        now_playing: use_context::<Signal<Option<Song>>>(),
        is_playing: use_context::<IsPlayingSignal>().0,
        shuffle: use_context::<ShuffleEnabledSignal>().0,
        settings: use_context::<Signal<AppSettings>>(),
    }
}

pub const OFFLINE_NOTHING: &str =
    "You're offline, and none of these songs are downloaded. Connect to play them.";

impl Player {
    /// Offline, only downloaded songs play. Returns the playable songs and
    /// where `start` landed among them.
    fn playable(self, songs: Vec<Song>, start: usize) -> Result<(Vec<Song>, usize), String> {
        if !offline_now(&self.settings.peek()) {
            let start = start.min(songs.len().saturating_sub(1));
            return Ok((songs, start));
        }
        let wanted = songs.get(start).map(song_key);
        let kept: Vec<Song> = songs
            .into_iter()
            .filter(crate::offline_audio::is_song_downloaded)
            .collect();
        if kept.is_empty() {
            return Err(OFFLINE_NOTHING.to_string());
        }
        let start = wanted
            .and_then(|key| kept.iter().position(|s| song_key(s) == key))
            .unwrap_or(0);
        Ok((kept, start))
    }

    fn start(mut self, songs: Vec<Song>, index: usize) {
        let first = songs.get(index).cloned();
        self.queue.set(songs);
        self.queue_index.set(index);
        self.now_playing.set(first);
        self.is_playing.set(true);
    }

    /// Replaces the queue with `songs` and plays `start`. With a `source` the
    /// queue remembers its album/playlist, so the player's shuffle can
    /// reorder and restore it. Respects the current shuffle mode.
    pub fn play(
        self,
        songs: Vec<Song>,
        start: usize,
        source: Option<(QueueSourceKind, String)>,
    ) -> Result<(), String> {
        if songs.is_empty() {
            return Err("Nothing to play.".to_string());
        }
        let (songs, start) = self.playable(songs, start)?;
        let songs = match source {
            Some((kind, id)) => assign_collection_queue_meta(songs, kind, id),
            None => normalize_manual_queue_songs(songs),
        };
        self.start(songs, start);
        if *self.shuffle.peek() {
            apply_collection_shuffle_mode(self.queue, self.queue_index, self.now_playing, true);
        }
        Ok(())
    }

    /// Plays `songs` in a random order and switches shuffle on. Turning
    /// shuffle off later restores the original order.
    pub fn shuffle_play(
        mut self,
        songs: Vec<Song>,
        source: Option<(QueueSourceKind, String)>,
    ) -> Result<(), String> {
        use rand::seq::SliceRandom;
        if songs.is_empty() {
            return Err("Nothing to play.".to_string());
        }
        let (songs, _) = self.playable(songs, 0)?;
        let mut songs = match source {
            Some((kind, id)) => assign_collection_queue_meta(songs, kind, id),
            None => normalize_manual_queue_songs(songs),
        };
        songs.shuffle(&mut rand::thread_rng());
        if !*self.shuffle.peek() {
            self.shuffle.set(true);
        }
        self.start(songs, 0);
        Ok(())
    }

    /// Adds songs after the current one (`next`) or at the end.
    pub fn enqueue(mut self, songs: Vec<Song>, next: bool) {
        let songs = normalize_manual_queue_songs(songs);
        if songs.is_empty() {
            return;
        }
        if self.now_playing.peek().is_none() && self.queue.peek().is_empty() {
            self.start(songs, 0);
            return;
        }
        let at = (*self.queue_index.peek() + 1).min(self.queue.peek().len());
        self.queue.with_mut(|items| {
            if next {
                let tail = items.split_off(at);
                items.extend(songs);
                items.extend(tail);
            } else {
                items.extend(songs);
            }
        });
    }
}

/// Key of the song now playing, and whether it is audibly playing. Lists
/// re-render on a track change, never on the position tick.
pub fn use_now_playing() -> (Memo<Option<String>>, Memo<bool>) {
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<IsPlayingSignal>().0;
    let key = use_memo(move || now_playing.read().as_ref().map(song_key));
    let live = use_memo(move || is_playing());
    (key, live)
}

// ---- List marks -------------------------------------------------------------

/// Keys of the songs in `songs` that are downloaded. Computed in an effect
/// (never in render) and patched when the track menu downloads a song.
pub fn use_downloaded_keys(songs: Signal<Vec<Song>>) -> Signal<HashSet<String>> {
    let mut keys = use_signal(HashSet::<String>::new);
    let menu = use_context::<TrackMenuController>();
    use_effect(move || {
        let list = songs.read().clone();
        if cfg!(target_arch = "wasm32") || list.is_empty() {
            return;
        }
        spawn(async move {
            // Yield first so the list paints before the file checks run.
            sleep_ms(0).await;
            let found: HashSet<String> = list
                .iter()
                .filter(|song| crate::offline_audio::is_song_downloaded(song))
                .map(song_key)
                .collect();
            if *keys.peek() != found {
                keys.set(found);
            }
        });
    });
    use_effect(move || {
        let Some(change) = (menu.last_change)() else {
            return;
        };
        if change.downloaded == Some(true) {
            let key = format!("{}::{}", change.server_id, change.song_id);
            if !keys.peek().contains(&key) {
                keys.write().insert(key);
            }
        }
    });
    keys
}

/// Applies favourite and rating changes made from the track menu to `songs`,
/// so the marks update without a refetch.
pub fn use_song_patches(mut songs: Signal<Vec<Song>>) {
    let menu = use_context::<TrackMenuController>();
    use_effect(move || {
        let Some(change) = (menu.last_change)() else {
            return;
        };
        if change.starred.is_none() && change.rating.is_none() {
            return;
        }
        let needs = songs.peek().iter().any(|s| change.matches(s));
        if !needs {
            return;
        }
        songs.with_mut(|list| {
            for song in list.iter_mut().filter(|s| change.matches(s)) {
                if let Some(on) = change.starred {
                    song.starred = on.then(|| "local".to_string());
                }
                if let Some(value) = change.rating {
                    song.user_rating = (value > 0).then_some(value);
                }
            }
        });
    });
}

// ---- Search -----------------------------------------------------------------

/// Debounced copy of `input` (trimmed). Fires from one character.
pub fn use_debounced(input: Signal<String>, ms: u64) -> Signal<String> {
    let mut output = use_signal(|| input.peek().trim().to_string());
    let mut generation = use_signal(|| 0u64);
    use_effect(move || {
        let value = input().trim().to_string();
        let next = *generation.peek() + 1;
        generation.set(next);
        if value.is_empty() {
            if !output.peek().is_empty() {
                output.set(String::new());
            }
            return;
        }
        spawn(async move {
            sleep_ms(ms).await;
            if *generation.peek() == next && *output.peek() != value {
                output.set(value);
            }
        });
    });
    output
}

// ---- Paging -----------------------------------------------------------------

/// Offset-paging state for a list that appends pages.
pub struct Pager<T: 'static> {
    pub items: Signal<Vec<T>>,
    /// Rows requested so far per server (the next page starts here).
    pub offset: Signal<u32>,
    pub loading: Signal<bool>,
    pub done: Signal<bool>,
    pub error: Signal<Option<String>>,
    /// First page has arrived (or failed) for the current query.
    pub ready: Signal<bool>,
    /// Bumped on every reset; stale responses are dropped.
    pub generation: Signal<u64>,
}

// Signals are Copy whatever they hold; derive would demand `T: Copy`.
impl<T: 'static> Clone for Pager<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: 'static> Copy for Pager<T> {}

pub fn use_pager<T: 'static>() -> Pager<T> {
    Pager {
        items: use_signal(Vec::new),
        offset: use_signal(|| 0u32),
        loading: use_signal(|| false),
        done: use_signal(|| false),
        error: use_signal(|| None),
        ready: use_signal(|| false),
        generation: use_signal(|| 0u64),
    }
}

impl<T: Clone + 'static> Pager<T> {
    /// Clears the list for a new query. Returns the new generation.
    pub fn reset(mut self) -> u64 {
        let next = *self.generation.peek() + 1;
        self.generation.set(next);
        self.items.set(Vec::new());
        self.offset.set(0);
        self.loading.set(false);
        self.done.set(false);
        self.error.set(None);
        self.ready.set(false);
        next
    }

    /// Starts loading the next page unless one is in flight or the list is
    /// complete. `fetch(offset)` returns the page and whether more exist; on
    /// success the offset moves on by `page_size`.
    pub fn load_more<F, Fut>(mut self, page_size: u32, fetch: F)
    where
        F: FnOnce(u32) -> Fut + 'static,
        Fut: std::future::Future<Output = Result<(Vec<T>, bool), String>> + 'static,
    {
        if *self.loading.peek() || *self.done.peek() {
            return;
        }
        let generation = *self.generation.peek();
        let offset = *self.offset.peek();
        self.loading.set(true);
        self.error.set(None);
        spawn(async move {
            let result = fetch(offset).await;
            if *self.generation.peek() != generation {
                return;
            }
            match result {
                Ok((page, more)) => {
                    self.offset.set(offset + page_size);
                    if !page.is_empty() {
                        self.items.with_mut(|items| items.extend(page));
                    }
                    if !more {
                        self.done.set(true);
                    }
                }
                Err(err) => self.error.set(Some(err)),
            }
            self.loading.set(false);
            if !*self.ready.peek() {
                self.ready.set(true);
            }
        });
    }
}

/// Invisible marker near the end of a paged list. It sits `lead_px` above
/// the list's end and calls `on_near_end` when it scrolls into view. Give it
/// a `key` that changes per page so it re-observes after each append.
#[component]
pub fn LoadMore(on_near_end: EventHandler<()>, #[props(default = 900)] lead_px: u32) -> Element {
    rsx! {
        div {
            class: "rs-library-sentinel",
            style: "margin-top:-{lead_px}px;height:{lead_px}px",
            aria_hidden: "true",
            onvisible: move |evt| {
                if evt.is_intersecting().unwrap_or(false) {
                    on_near_end.call(());
                }
            },
        }
    }
}

/// Footer under a paged list: spinner while loading, Retry after an error.
#[component]
pub fn PageFoot(loading: bool, error: Option<String>, on_retry: EventHandler<()>) -> Element {
    if let Some(err) = error {
        return rsx! {
            div { class: "rs-library-foot rs-library-foot--error", role: "status",
                span { "Couldn't load more. {err}" }
                Button {
                    label: "Retry".to_string(),
                    variant: ButtonVariant::Plain,
                    onclick: move |_| on_retry.call(()),
                }
            }
        };
    }
    if loading {
        return rsx! {
            div { class: "rs-library-foot", role: "status", aria_label: "Loading more",
                span { class: "rs-spinner rs-spinner--page" }
            }
        };
    }
    rsx! {}
}

// ---- States -----------------------------------------------------------------

/// Error with a Retry button; the message mentions being offline when we are.
#[component]
pub fn ErrorState(
    title: String,
    error: String,
    on_retry: EventHandler<()>,
    #[props(default)] testid: Option<String>,
) -> Element {
    let settings = use_context::<Signal<AppSettings>>();
    let offline = offline_now(&settings.read());
    let message = if offline {
        "You're offline. Downloaded music still plays; reconnect to browse the rest.".to_string()
    } else {
        format!("The server said: {error}")
    };
    rsx! {
        EmptyState {
            icon: if offline { "offline".to_string() } else { "server".to_string() },
            title,
            message,
            testid,
            Button {
                label: "Retry".to_string(),
                icon: "refresh-cw".to_string(),
                variant: ButtonVariant::Tinted,
                onclick: move |_| on_retry.call(()),
            }
        }
    }
}

/// "No servers" empty state with a way to Settings.
#[component]
pub fn NoServers() -> Element {
    let navigation = use_context::<crate::components::Navigation>();
    rsx! {
        EmptyState {
            icon: "server".to_string(),
            title: "No server connected".to_string(),
            message: "Add your Navidrome or Subsonic server to browse your library.".to_string(),
            Button {
                label: "Open Settings".to_string(),
                onclick: move |_| navigation.navigate_to(crate::components::AppView::SettingsView {}),
            }
        }
    }
}

/// Skeleton for a cover grid.
#[component]
pub fn GridSkeleton(#[props(default = 8)] count: usize) -> Element {
    rsx! {
        div { class: "rs-library-covers", aria_busy: "true", aria_label: "Loading",
            for index in 0..count {
                div { key: "{index}", class: "rs-library-tile",
                    div { class: "rs-skel rs-library-tile__skel" }
                    div { class: "rs-skel", style: "width:70%;height:12px;margin-top:8px" }
                    div { class: "rs-skel", style: "width:45%;height:10px;margin-top:6px" }
                }
            }
        }
    }
}

/// Skeleton for a track list.
#[component]
pub fn ListSkeleton(#[props(default = 8)] rows: usize, #[props(default)] round: bool) -> Element {
    rsx! {
        Skeleton { rows, round }
    }
}
