//! The track context menu (long-press, right-click or "…").
//!
//! One `TrackMenuHost` is mounted by the shell; anything can open the menu for
//! a song through the `TrackMenuController` context:
//!
//! ```ignore
//! let mut menu = use_context::<TrackMenuController>();
//! menu.open(song.clone(), Some((x, y)));
//! ```
//!
//! Actions reuse the app's existing paths: the add-to menu (`AddIntent`) for
//! playlists, the server star/rating calls, `prefetch_song_audio` for
//! downloads, `Navigation` for album/artist, and the song-details overlay for
//! lyrics and info. After a favourite, rating or download change the
//! controller publishes a `SongChange` so lists can update their marks.

use crate::api::{NavidromeClient, ServerConfig, Song};
use crate::components::audio_manager::normalize_manual_queue_songs;
use crate::components::ui::env::is_radio_song;
use crate::components::ui::Artwork;
use crate::components::views::artist_links::{parse_artist_names, resolve_artist_id_for_name};
use crate::components::{
    AddIntent, AddMenuController, AppView, Icon, IsPlayingSignal, Navigation,
    SongDetailsController, SongDetailsTab,
};
use crate::db::AppSettings;
use dioxus::prelude::*;

/// What the menu was opened for.
#[derive(Clone, PartialEq, Debug)]
pub struct TrackMenuRequest {
    pub song: Song,
    /// Viewport point to anchor the popover at (wide layout). None centres it.
    pub anchor: Option<(f64, f64)>,
}

/// A change made from the menu that list views may want to reflect.
#[derive(Clone, PartialEq, Debug)]
pub struct SongChange {
    pub server_id: String,
    pub song_id: String,
    pub starred: Option<bool>,
    pub rating: Option<u32>,
    pub downloaded: Option<bool>,
}

impl SongChange {
    fn for_song(song: &Song) -> Self {
        Self {
            server_id: song.server_id.clone(),
            song_id: song.id.clone(),
            starred: None,
            rating: None,
            downloaded: None,
        }
    }

    /// True when this change is about `song`.
    pub fn matches(&self, song: &Song) -> bool {
        self.song_id == song.id && self.server_id == song.server_id
    }
}

/// Context handle for the track menu (provided by the shell).
#[derive(Clone, Copy, PartialEq)]
pub struct TrackMenuController {
    pub request: Signal<Option<TrackMenuRequest>>,
    /// Last favourite/rating/download change made from the menu.
    pub last_change: Signal<Option<SongChange>>,
}

impl TrackMenuController {
    pub fn new(
        request: Signal<Option<TrackMenuRequest>>,
        last_change: Signal<Option<SongChange>>,
    ) -> Self {
        Self {
            request,
            last_change,
        }
    }

    /// Opens the menu for `song`, anchored at a viewport point if given.
    pub fn open(&mut self, song: Song, anchor: Option<(f64, f64)>) {
        self.request.set(Some(TrackMenuRequest { song, anchor }));
    }

    pub fn close(&mut self) {
        if self.request.peek().is_some() {
            self.request.set(None);
        }
    }
}

fn update_song_everywhere(
    mut queue: Signal<Vec<Song>>,
    mut now_playing: Signal<Option<Song>>,
    server_id: &str,
    song_id: &str,
    apply: impl Fn(&mut Song),
) {
    queue.with_mut(|items| {
        for item in items
            .iter_mut()
            .filter(|s| s.id == song_id && s.server_id == server_id)
        {
            apply(item);
        }
    });
    let playing_matches = now_playing
        .peek()
        .as_ref()
        .map(|s| s.id == song_id && s.server_id == server_id)
        .unwrap_or(false);
    if playing_matches {
        now_playing.with_mut(|current| {
            if let Some(song) = current.as_mut() {
                apply(song);
            }
        });
    }
}

/// The menu card itself: tile row (Play Next, Add to Queue, Favorite), then
/// grouped rows. Rendered by `TrackMenuHost`; usable on its own in a sheet.
/// * `song`: the target. `show_head`: show the song "lift" header (sheets).
/// * `popover_style`: when set, the card is a fixed-position popover with
///   this inline style.
#[component]
pub fn TrackMenu(
    song: Song,
    #[props(default)] show_head: bool,
    #[props(default)] popover_style: Option<String>,
) -> Element {
    let mut controller = use_context::<TrackMenuController>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<IsPlayingSignal>().0;
    let add_menu = use_context::<AddMenuController>();
    let song_details = use_context::<SongDetailsController>();
    let navigation = use_context::<Navigation>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let mut rate_open = use_signal(|| false);

    let downloaded = {
        let song = song.clone();
        use_resource(move || {
            let song = song.clone();
            async move { crate::offline_audio::is_song_downloaded(&song) }
        })
    };

    let radio = is_radio_song(&song);
    let starred = song.starred.is_some();
    let rating = song.user_rating.unwrap_or(0).min(5);
    let has_album = song.album_id.is_some();
    let has_artist = song
        .artist
        .as_deref()
        .map(|a| !a.trim().is_empty())
        .unwrap_or(false);
    let can_download = !cfg!(target_arch = "wasm32") && !radio;
    let is_downloaded = downloaded().unwrap_or(false);

    // ---- Queue actions --------------------------------------------------
    let enqueue = {
        let song = song.clone();
        move |next: bool| {
            let songs = normalize_manual_queue_songs(vec![song.clone()]);
            let (mut queue, mut queue_index, mut now_playing, mut is_playing) =
                (queue, queue_index, now_playing, is_playing);
            if now_playing.peek().is_none() && queue.peek().is_empty() {
                let first = songs.first().cloned();
                queue.set(songs);
                queue_index.set(0);
                now_playing.set(first);
                is_playing.set(true);
            } else {
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
            controller.close();
        }
    };
    let play_next = {
        let mut enqueue = enqueue.clone();
        move |_| enqueue(true)
    };
    let add_to_queue = {
        let mut enqueue = enqueue.clone();
        move |_| enqueue(false)
    };

    let start_radio = {
        let song = song.clone();
        move |_| {
            controller.close();
            let seed = song.clone();
            let server = servers
                .peek()
                .iter()
                .find(|s| s.id == seed.server_id)
                .cloned();
            let Some(server) = server else {
                return;
            };
            let (mut queue, mut queue_index, mut now_playing, mut is_playing) =
                (queue, queue_index, now_playing, is_playing);
            spawn(async move {
                let client = NavidromeClient::new(server);
                let mut similar = client.get_similar_songs(&seed.id, 50).await.unwrap_or_default();
                if similar.is_empty() {
                    similar = client.get_random_songs(50).await.unwrap_or_default();
                }
                let mut mix = vec![seed.clone()];
                let mut seen = std::collections::HashSet::new();
                seen.insert(format!("{}::{}", seed.server_id, seed.id));
                for track in similar {
                    if seen.insert(format!("{}::{}", track.server_id, track.id)) {
                        mix.push(track);
                    }
                    if mix.len() >= 50 {
                        break;
                    }
                }
                let mix = normalize_manual_queue_songs(mix);
                let first = mix.first().cloned();
                queue.set(mix);
                queue_index.set(0);
                now_playing.set(first);
                is_playing.set(true);
            });
        }
    };

    // ---- Library actions -------------------------------------------------
    let toggle_favorite = {
        let song = song.clone();
        move |_| {
            controller.close();
            let should_star = song.starred.is_none();
            let server = servers
                .peek()
                .iter()
                .find(|s| s.id == song.server_id)
                .cloned();
            let Some(server) = server else {
                return;
            };
            let song = song.clone();
            let mut last_change = controller.last_change;
            spawn(async move {
                let client = NavidromeClient::new(server);
                let result = if should_star {
                    client.star(&song.id, "song").await
                } else {
                    client.unstar(&song.id, "song").await
                };
                if result.is_ok() {
                    let marker = should_star.then(|| "local".to_string());
                    update_song_everywhere(queue, now_playing, &song.server_id, &song.id, |s| {
                        s.starred = marker.clone();
                    });
                    let mut change = SongChange::for_song(&song);
                    change.starred = Some(should_star);
                    last_change.set(Some(change));
                }
            });
        }
    };

    let set_rating = {
        let song = song.clone();
        move |value: u32| {
            controller.close();
            let server = servers
                .peek()
                .iter()
                .find(|s| s.id == song.server_id)
                .cloned();
            let Some(server) = server else {
                return;
            };
            let song = song.clone();
            let mut last_change = controller.last_change;
            spawn(async move {
                let client = NavidromeClient::new(server);
                if client.set_rating(&song.id, value).await.is_ok() {
                    let stored = (value > 0).then_some(value);
                    update_song_everywhere(queue, now_playing, &song.server_id, &song.id, |s| {
                        s.user_rating = stored;
                    });
                    let mut change = SongChange::for_song(&song);
                    change.rating = Some(value);
                    last_change.set(Some(change));
                }
            });
        }
    };

    let add_to_playlist = {
        let song = song.clone();
        let mut add_menu = add_menu.clone();
        move |_| {
            controller.close();
            add_menu.open(AddIntent::from_song(song.clone()));
        }
    };

    let download = {
        let song = song.clone();
        move |_| {
            controller.close();
            let servers_snapshot = servers.peek().clone();
            if servers_snapshot.is_empty() {
                return;
            }
            let mut settings = app_settings.peek().clone();
            settings.downloads_enabled = true;
            let song = song.clone();
            let mut last_change = controller.last_change;
            spawn(async move {
                if crate::offline_audio::prefetch_song_audio(&song, &servers_snapshot, &settings)
                    .await
                    .is_ok()
                {
                    let mut change = SongChange::for_song(&song);
                    change.downloaded = Some(true);
                    last_change.set(Some(change));
                }
            });
        }
    };

    // ---- Navigation ----------------------------------------------------------
    let go_album = {
        let song = song.clone();
        let mut song_details = song_details.clone();
        move |_| {
            controller.close();
            if let Some(album_id) = song.album_id.clone() {
                song_details.close();
                navigation.navigate_to(AppView::AlbumDetailView {
                    album_id,
                    server_id: song.server_id.clone(),
                });
            }
        }
    };

    let go_artist = {
        let song = song.clone();
        let mut song_details = song_details.clone();
        move |_| {
            controller.close();
            song_details.close();
            let names = parse_artist_names(song.artist.as_deref().unwrap_or_default());
            if names.len() <= 1 {
                if let Some(artist_id) = song.artist_id.clone() {
                    navigation.navigate_to(AppView::ArtistDetailView {
                        artist_id,
                        server_id: song.server_id.clone(),
                    });
                    return;
                }
            }
            let Some(name) = names.into_iter().next() else {
                return;
            };
            let server = servers
                .peek()
                .iter()
                .find(|s| s.id == song.server_id)
                .cloned();
            let Some(server) = server else {
                return;
            };
            let server_id = song.server_id.clone();
            spawn(async move {
                if let Some(artist_id) = resolve_artist_id_for_name(server, name).await {
                    navigation.navigate_to(AppView::ArtistDetailView {
                        artist_id,
                        server_id,
                    });
                }
            });
        }
    };

    let open_details = {
        let song = song.clone();
        let song_details = song_details.clone();
        move |tab: SongDetailsTab| {
            controller.close();
            let mut song_details = song_details.clone();
            song_details.open(song.clone());
            song_details.set_tab(tab);
        }
    };
    let show_lyrics = {
        let mut open_details = open_details.clone();
        move |_| open_details(SongDetailsTab::Lyrics)
    };
    let show_info = {
        let mut open_details = open_details.clone();
        move |_| open_details(SongDetailsTab::Details)
    };

    let subtitle = song.artist.clone().unwrap_or_default();
    let is_popover = popover_style.is_some();

    rsx! {
        div {
            class: "rs-menu",
            class: if is_popover { "rs-menu--popover" },
            style: popover_style.clone(),
            role: "menu",
            aria_label: "{song.title}",
            tabindex: "-1",
            "data-testid": "track-menu",
            onmounted: move |evt| async move {
                let _ = evt.set_focus(true).await;
            },
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    evt.prevent_default();
                    controller.close();
                }
            },
            if show_head {
                div { class: "rs-menu__head",
                    Artwork {
                        cover_art: song.cover_art.clone(),
                        server_id: song.server_id.clone(),
                        size_px: 44,
                    }
                    div { class: "rs-menu__head-meta",
                        div { class: "rs-menu__head-title", "{song.title}" }
                        if !subtitle.is_empty() {
                            div { class: "rs-menu__head-sub", "{subtitle}" }
                        }
                    }
                }
                div { class: "rs-menu__gap", aria_hidden: "true" }
            }
            div { class: "rs-menu__tiles",
                button {
                    r#type: "button",
                    class: "rs-menu__tile",
                    role: "menuitem",
                    disabled: radio,
                    "data-testid": "track-menu-play-next",
                    onclick: play_next,
                    Icon { name: "play-next".to_string(), class: "rs-ic".to_string() }
                    "Play Next"
                }
                button {
                    r#type: "button",
                    class: "rs-menu__tile",
                    role: "menuitem",
                    disabled: radio,
                    "data-testid": "track-menu-add-to-queue",
                    onclick: add_to_queue,
                    Icon { name: "add-to-queue".to_string(), class: "rs-ic".to_string() }
                    "Add to Queue"
                }
                button {
                    r#type: "button",
                    class: "rs-menu__tile",
                    role: "menuitemcheckbox",
                    aria_checked: if starred { "true" } else { "false" },
                    disabled: radio,
                    "data-testid": "track-menu-favorite",
                    onclick: toggle_favorite,
                    Icon {
                        name: if starred { "heart-filled".to_string() } else { "heart".to_string() },
                        class: "rs-ic".to_string(),
                    }
                    if starred { "Favorited" } else { "Favorite" }
                }
            }
            div { class: "rs-menu__gap", aria_hidden: "true" }
            button {
                r#type: "button",
                class: "rs-menu__item",
                role: "menuitem",
                disabled: radio,
                "data-testid": "track-menu-add-to-playlist",
                onclick: add_to_playlist,
                "Add to Playlist…"
                Icon { name: "playlist".to_string(), class: "rs-ic".to_string() }
            }
            button {
                r#type: "button",
                class: "rs-menu__item",
                role: "menuitem",
                disabled: radio,
                "data-testid": "track-menu-start-radio",
                onclick: start_radio,
                "Start Radio from Song"
                Icon { name: "radio".to_string(), class: "rs-ic".to_string() }
            }
            button {
                r#type: "button",
                class: "rs-menu__item",
                role: "menuitem",
                disabled: !has_album,
                "data-testid": "track-menu-go-album",
                onclick: go_album,
                "Go to Album"
                Icon { name: "disc".to_string(), class: "rs-ic".to_string() }
            }
            button {
                r#type: "button",
                class: "rs-menu__item",
                role: "menuitem",
                disabled: !has_artist || radio,
                "data-testid": "track-menu-go-artist",
                onclick: go_artist,
                "Go to Artist"
                Icon { name: "artist".to_string(), class: "rs-ic".to_string() }
            }
            div { class: "rs-menu__gap", aria_hidden: "true" }
            button {
                r#type: "button",
                class: "rs-menu__item",
                role: "menuitem",
                aria_haspopup: "menu",
                aria_expanded: if rate_open() { "true" } else { "false" },
                disabled: radio,
                "data-testid": "track-menu-rate",
                onclick: move |_| rate_open.toggle(),
                if rating > 0 {
                    "Rate ({rating} of 5)"
                } else {
                    "Rate"
                }
                Icon {
                    name: if rate_open() { "chevron-down".to_string() } else { "chevron-right".to_string() },
                    class: "rs-ic rs-ic--sm rs-menu__sub".to_string(),
                }
            }
            if rate_open() {
                div {
                    class: "rs-menu__stars",
                    role: "group",
                    aria_label: "Rating",
                    for value in 1..=5u32 {
                        button {
                            key: "{value}",
                            r#type: "button",
                            class: "rs-menu__star",
                            class: if value <= rating { "rs-menu__star--on" },
                            aria_label: "{value} star",
                            aria_pressed: if value == rating { "true" } else { "false" },
                            onclick: {
                                let mut set_rating = set_rating.clone();
                                move |_| set_rating(value)
                            },
                            Icon {
                                name: if value <= rating { "star-filled".to_string() } else { "star".to_string() },
                                class: "rs-ic rs-ic--md".to_string(),
                            }
                        }
                    }
                    button {
                        r#type: "button",
                        class: "rs-menu__star",
                        aria_label: "Clear rating",
                        onclick: {
                            let mut set_rating = set_rating.clone();
                            move |_| set_rating(0)
                        },
                        Icon { name: "x".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                    }
                }
            }
            if can_download {
                button {
                    r#type: "button",
                    class: "rs-menu__item",
                    role: "menuitem",
                    disabled: is_downloaded,
                    "data-testid": "track-menu-download",
                    onclick: download,
                    if is_downloaded { "Downloaded" } else { "Download" }
                    Icon {
                        name: if is_downloaded { "downloaded".to_string() } else { "download".to_string() },
                        class: "rs-ic".to_string(),
                    }
                }
            }
            button {
                r#type: "button",
                class: "rs-menu__item",
                role: "menuitem",
                disabled: radio,
                "data-testid": "track-menu-lyrics",
                onclick: show_lyrics,
                "Show Lyrics"
                Icon { name: "lyrics".to_string(), class: "rs-ic".to_string() }
            }
            button {
                r#type: "button",
                class: "rs-menu__item",
                role: "menuitem",
                "data-testid": "track-menu-info",
                onclick: show_info,
                "Song Info"
                Icon { name: "info".to_string(), class: "rs-ic".to_string() }
            }
        }
    }
}
