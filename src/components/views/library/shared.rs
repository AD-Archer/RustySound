//! Plumbing shared by the Library views: list marks (now playing,
//! downloaded, starred), debounced search and offset paging. Playback goes
//! through `PlayerCtx` (`crate::components::use_player`); the page states
//! (`LoadMore`, `PageFoot`, `ErrorState`, `NoServers`, `GridSkeleton`) are
//! the kit's (`ui/states.rs`).

use crate::api::{item_key, Capabilities, MusicClient, Playlist, ServerConfig, Song};
use crate::components::song_details::actions::apply_song_change;
use crate::components::ui::TrackMenuController;
use crate::components::IsPlayingSignal;
use crate::time::sleep_ms;
use dioxus::prelude::*;
use std::collections::HashSet;

/// Playlists imported by Navidrome from .m3u files can't be edited.
pub fn is_auto_imported(playlist: &Playlist) -> bool {
    playlist
        .comment
        .as_deref()
        .map(|c| c.to_lowercase().contains("auto-imported"))
        .unwrap_or(false)
}

/// The user can change this playlist here: it is theirs (Navidrome refuses
/// edits to other users' public playlists), it wasn't auto-imported from a
/// playlist file (read-only on the server), and its server is configured,
/// active and can edit playlists (Jellyfin playlists are read-only here).
pub fn playlist_editable(playlist: &Playlist, server: Option<&ServerConfig>) -> bool {
    let Some(server) = server else {
        return false;
    };
    let own = playlist
        .owner
        .as_deref()
        .map(|owner| owner.trim().is_empty() || owner.eq_ignore_ascii_case(&server.username))
        .unwrap_or(true);
    server.active && own && !is_auto_imported(playlist) && Capabilities::of(server).playlist_edits
}

/// Bumped when a playlist is created, renamed or deleted here, so the lists
/// that show playlists (the sidebar's) load them again.
pub static PLAYLISTS_REVISION: GlobalSignal<u64> = Signal::global(|| 0);

pub fn playlists_changed() {
    *PLAYLISTS_REVISION.write() += 1;
}

pub fn active_servers(servers: &[ServerConfig]) -> Vec<ServerConfig> {
    servers.iter().filter(|s| s.active).cloned().collect()
}

/// The playlists on `servers`, asked side by side (each client caches its
/// server's list). An error only when no server answered.
pub async fn load_playlists(servers: Vec<ServerConfig>) -> Result<Vec<Playlist>, String> {
    let answers = futures_util::future::join_all(
        servers
            .into_iter()
            .map(|server| async move { MusicClient::new(server).get_playlists().await }),
    )
    .await;
    let mut all = Vec::new();
    let mut last_error = None;
    for answer in answers {
        match answer {
            Ok(list) => all.extend(list),
            Err(err) => last_error = Some(err),
        }
    }
    match (all.is_empty(), last_error) {
        (true, Some(err)) => Err(err),
        _ => Ok(all),
    }
}

/// Whether there's anything to browse: an active server, or music on the
/// device. Reactive (reads the local library status).
pub fn has_sources(servers: &[ServerConfig]) -> bool {
    servers.iter().any(|s| s.active) || crate::local_music::has_music()
}

/// Key of the song now playing, and whether it is audibly playing. Lists
/// re-render on a track change, never on the position tick.
pub fn use_now_playing() -> (Memo<Option<String>>, Memo<bool>) {
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<IsPlayingSignal>().0;
    let key = use_memo(move || now_playing.read().as_ref().map(Song::key));
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
        // Ids only (no whole-list clone). One listing of the download folder,
        // off the main thread: this reruns after every page a paged list
        // appends, and checking each song with file calls on the main thread
        // stalled scrolling for longer the further down the list you were.
        let ids: Vec<(String, String)> = songs
            .read()
            .iter()
            .map(|song| (song.server_id.clone(), song.id.clone()))
            .collect();
        if cfg!(target_arch = "wasm32") || ids.is_empty() {
            return;
        }
        spawn(async move {
            // Yield first so the list paints before the folder is read.
            sleep_ms(0).await;
            #[cfg(not(target_arch = "wasm32"))]
            let found: HashSet<String> = tokio::task::spawn_blocking(move || {
                crate::offline_audio::downloaded_song_keys(&ids)
            })
            .await
            .unwrap_or_default();
            #[cfg(target_arch = "wasm32")]
            let found: HashSet<String> = {
                let _ = ids;
                HashSet::new()
            };
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
            let key = item_key(&change.server_id, &change.song_id);
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
                apply_song_change(song, &change);
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

// ---- Favourites -------------------------------------------------------------

/// Stars or unstars an album or artist (`kind`: "album", "artist") where it
/// lives. Optimistic: `starred` flips at once and flips back, with the
/// reason in `status`, when the call fails.
pub fn toggle_star(
    servers: &[ServerConfig],
    server_id: &str,
    kind: &'static str,
    id: String,
    mut starred: Signal<bool>,
    mut status: Signal<Option<String>>,
) {
    let Some(client) = MusicClient::for_source(servers, server_id) else {
        status.set(Some(format!("This {kind}'s server isn't connected.")));
        return;
    };
    let should = !*starred.peek();
    starred.set(should);
    spawn(async move {
        let result = if should {
            client.star(&id, kind).await
        } else {
            client.unstar(&id, kind).await
        };
        if let Err(err) = result {
            starred.set(!should);
            status.set(Some(format!("Couldn't update Favorites: {err}")));
        }
    });
}
