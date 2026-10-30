//! Home's data: one signal per shelf, filled progressively by one load task
//! (stage first, then the cheap snapshot shelves, then the similarity
//! shelves), and the off-server shelf by its own task once the library
//! shelves are in. Shelves read only their own signal, so a shelf landing
//! re-renders that shelf and nothing else.

use crate::api::discovery::{
    load_albums_you_might_like, load_discover_off_server, load_discover_rewind,
    load_home_playlists, load_new_in_library, load_recently_played, load_songs_you_might_like,
    refresh_discover, DiscoverAlbum, DiscoverSong, DiscoveryConfig, HomePlaylist,
    OffServerSection, RewindSection,
};
use super::stage::initial_centre;
use crate::api::ServerConfig;
use crate::db::AppSettings;
use dioxus::core::Task;
use dioxus::prelude::*;

/// Home's loaded data and its controls, provided as context to the shelves.
/// `None` means "loading" (skeleton); `Some(empty)` means "hide the shelf".
#[derive(Clone, Copy)]
pub struct HomeData {
    /// Recently played albums: the Cover Flow stage.
    pub recent: Signal<Option<Vec<DiscoverAlbum>>>,
    pub new_albums: Signal<Option<Vec<DiscoverAlbum>>>,
    pub playlists: Signal<Option<Vec<HomePlaylist>>>,
    pub songs: Signal<Option<Vec<DiscoverSong>>>,
    pub albums_like: Signal<Option<Vec<DiscoverAlbum>>>,
    pub rewind: Signal<Option<RewindSection>>,
    pub off_server: Signal<Option<OffServerSection>>,
    /// Settings → Discovery "show music not on your server".
    pub offserver_enabled: Memo<bool>,
    /// A refresh is in flight (old content stays visible meanwhile).
    pub refreshing: Signal<bool>,
    /// Index of the centred Cover Flow item.
    pub centre: Signal<usize>,
    pub active: Memo<Vec<ServerConfig>>,
    pub offline: Memo<bool>,
    generation: Signal<u64>,
}

impl HomeData {
    /// New picks for today: reseed and reload every shelf, keeping the old
    /// content on screen until the new content lands.
    pub fn refresh(&self) {
        if *self.refreshing.peek() || self.active.peek().is_empty() {
            return;
        }
        refresh_discover(&self.active.peek());
        let mut generation = self.generation;
        *generation.write() += 1;
    }
}

/// Creates Home's data, provides it as context and drives the loads.
pub fn use_home_data(servers: Signal<Vec<ServerConfig>>, settings: Signal<AppSettings>) -> HomeData {
    let active = use_memo(move || {
        servers
            .read()
            .iter()
            .filter(|s| s.active)
            .cloned()
            .collect::<Vec<_>>()
    });
    // Memos notify only on real changes, so unrelated settings writes
    // (volume, the saved-mix map) never reload anything.
    let config = use_memo(move || DiscoveryConfig::from_settings(&settings.read()));
    let offserver_enabled = use_memo(move || config.read().offserver_enabled);
    let offline = use_memo(move || settings.read().offline_mode);

    let mut recent = use_signal(|| None::<Vec<DiscoverAlbum>>);
    let mut new_albums = use_signal(|| None::<Vec<DiscoverAlbum>>);
    let mut playlists = use_signal(|| None::<Vec<HomePlaylist>>);
    let mut songs = use_signal(|| None::<Vec<DiscoverSong>>);
    let mut albums_like = use_signal(|| None::<Vec<DiscoverAlbum>>);
    let mut rewind = use_signal(|| None::<RewindSection>);
    let mut off_server = use_signal(|| None::<OffServerSection>);
    let mut refreshing = use_signal(|| false);
    let mut centre = use_signal(|| 0usize);
    let generation = use_signal(|| 0u64);
    // The library shelves have loaded. The off-server shelf waits for it, so
    // both don't fetch the shared snapshot at once.
    let mut library_ready = use_signal(|| false);

    let data = use_context_provider(|| HomeData {
        recent,
        new_albums,
        playlists,
        songs,
        albums_like,
        rewind,
        off_server,
        offserver_enabled,
        refreshing,
        centre,
        active,
        offline,
        generation,
    });

    // The library shelves: on mount, server changes, offline changes and refresh.
    let mut task = use_signal(|| None::<Task>);
    let mut seen_generation = use_signal(|| 0u64);
    use_effect(move || {
        let servers = active();
        let _ = offline();
        let current = generation();

        if let Some(previous) = *task.peek() {
            previous.cancel();
        }
        let is_refresh = current != *seen_generation.peek();
        seen_generation.set(current);
        library_ready.set(false);
        if !is_refresh {
            recent.set(None);
            new_albums.set(None);
            playlists.set(None);
            songs.set(None);
            albums_like.set(None);
            rewind.set(None);
            off_server.set(None);
        }
        if servers.is_empty() {
            refreshing.set(false);
            return;
        }
        refreshing.set(is_refresh);
        let settings = settings.peek().clone();
        let load = spawn(async move {
            let stage = load_recently_played(&servers, &settings).await;
            let played = stage.len();
            centre.set(initial_centre(played));
            recent.set(Some(stage));
            let new = load_new_in_library(&servers, &settings).await;
            if played == 0 {
                // Nothing played yet: the stage shows the new albums instead.
                centre.set(initial_centre(new.len()));
            }
            new_albums.set(Some(new));
            rewind.set(Some(load_discover_rewind(&servers, &settings).await));
            playlists.set(Some(load_home_playlists(&servers, &settings).await));
            songs.set(Some(load_songs_you_might_like(&servers, &settings).await));
            albums_like.set(Some(load_albums_you_might_like(&servers, &settings).await));
            refreshing.set(false);
            library_ready.set(true);
        });
        task.set(Some(load));
    });

    // The off-server shelf: after the library shelves, and again when the
    // discovery settings (toggle, Last.fm, ListenBrainz) change.
    let mut off_task = use_signal(|| None::<Task>);
    use_effect(move || {
        let ready = library_ready();
        let enabled = config.read().offserver_enabled;
        let _ = offline();
        if let Some(previous) = *off_task.peek() {
            previous.cancel();
        }
        if !ready {
            return;
        }
        if !enabled {
            if off_server.peek().is_some() {
                off_server.set(None);
            }
            return;
        }
        let servers = active.peek().clone();
        let settings = settings.peek().clone();
        off_task.set(Some(spawn(async move {
            off_server.set(Some(load_discover_off_server(&servers, &settings).await));
        })));
    });

    data
}
