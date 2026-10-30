//! Search results: top hit, Artists (round thumbs), Albums, Songs (TrackRows),
//! each section previewed with a "See All".

use super::scoring::TopHit;
use crate::api::{Album, Artist, SearchResult, Song};
use crate::components::audio_manager::normalize_manual_queue_songs;
use crate::components::ui::{Artwork, InsetList, ListInset, ListRow, SectionHeader, TrackRow};
use crate::components::{AppView, Icon, IsPlayingSignal, Navigation};
use dioxus::prelude::*;

/// Songs shown at most, even after "See All" (a search is not a library view).
const SONG_CAP: usize = 200;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Kind {
    Artists,
    Albums,
    Songs,
}

fn preview_len(kind: Kind, wide: bool) -> usize {
    match (kind, wide) {
        (Kind::Artists, false) => 6,
        (Kind::Artists, true) => 8,
        (Kind::Albums, false) => 4,
        (Kind::Albums, true) => 6,
        (Kind::Songs, false) => 5,
        (Kind::Songs, true) => 8,
    }
}

/// Replaces the queue with one song and plays it (as the old Search did).
fn use_play_song() -> impl FnMut(Song) + Copy {
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<IsPlayingSignal>().0;
    move |song: Song| {
        let single = normalize_manual_queue_songs(vec![song]);
        queue.set(single.clone());
        queue_index.set(0);
        now_playing.set(single.first().cloned());
        is_playing.set(true);
    }
}

#[component]
pub(super) fn Results(
    results: SearchResult,
    top: Option<TopHit>,
    wide: bool,
    on_open: EventHandler<()>,
) -> Element {
    let navigation = use_context::<Navigation>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let mut play = use_play_song();
    let mut expanded = use_signal(|| None::<Kind>);

    let shown = |kind: Kind, total: usize| -> usize {
        if expanded() == Some(kind) {
            total.min(SONG_CAP)
        } else {
            total.min(preview_len(kind, wide))
        }
    };
    let see_all = move |kind: Kind, total: usize| -> (Option<String>, Option<EventHandler<MouseEvent>>) {
        if total <= preview_len(kind, wide) {
            return (None, None);
        }
        let open = expanded() == Some(kind);
        let label = if open { "Show Less".to_string() } else { "See All".to_string() };
        (
            Some(label),
            Some(EventHandler::new(move |_| {
                expanded.set(if open { None } else { Some(kind) });
            })),
        )
    };

    let open_artist = move |artist: Artist| {
        on_open.call(());
        navigation.navigate_to(AppView::ArtistDetailView {
            artist_id: artist.id,
            server_id: artist.server_id,
        });
    };
    let open_album = move |album: Album| {
        on_open.call(());
        navigation.navigate_to(AppView::AlbumDetailView {
            album_id: album.id,
            server_id: album.server_id,
        });
    };

    let playing_key = now_playing
        .read()
        .as_ref()
        .map(|s| format!("{}:{}", s.server_id, s.id));
    let artists = &results.artists[..shown(Kind::Artists, results.artists.len())];
    let albums = &results.albums[..shown(Kind::Albums, results.albums.len())];
    let songs = &results.songs[..shown(Kind::Songs, results.songs.len())];
    let (artists_action, artists_handler) = see_all(Kind::Artists, results.artists.len());
    let (albums_action, albums_handler) = see_all(Kind::Albums, results.albums.len());
    let (songs_action, songs_handler) = see_all(Kind::Songs, results.songs.len());

    rsx! {
        if let Some(hit) = top {
            SectionHeader { title: "Top Hit".to_string() }
            TopHitCard {
                hit: hit.clone(),
                on_activate: move |_| match hit.clone() {
                    TopHit::Artist(artist) => open_artist(artist),
                    TopHit::Album(album) => open_album(album),
                    TopHit::Song(song) => {
                        on_open.call(());
                        play(song);
                    }
                },
            }
        }

        if !artists.is_empty() {
            SectionHeader {
                title: "Artists".to_string(),
                action: artists_action,
                on_action: artists_handler,
                testid: "search-artists-all".to_string(),
            }
            ul { class: "rs-search-artists", aria_label: "Artists",
                for artist in artists.iter().cloned() {
                    li { key: "{artist.server_id}:{artist.id}",
                        ArtistCard {
                            artist: artist.clone(),
                            onclick: move |_| open_artist(artist.clone()),
                        }
                    }
                }
            }
        }

        if !albums.is_empty() {
            SectionHeader {
                title: "Albums".to_string(),
                action: albums_action,
                on_action: albums_handler,
                testid: "search-albums-all".to_string(),
            }
            InsetList { label: "Albums".to_string(),
                for album in albums.iter().cloned() {
                    ListRow {
                        key: "{album.server_id}:{album.id}",
                        title: album.name.clone(),
                        subtitle: match album.year {
                            Some(year) => format!("{} · {year}", album.artist),
                            None => album.artist.clone(),
                        },
                        chevron: true,
                        leading: rsx! {
                            Artwork {
                                cover_art: album.cover_art.clone(),
                                server_id: album.server_id.clone(),
                                size_px: 44,
                                placeholder: "album".to_string(),
                            }
                        },
                        onclick: move |_| open_album(album.clone()),
                    }
                }
            }
        }

        if !songs.is_empty() {
            SectionHeader {
                title: "Songs".to_string(),
                action: songs_action,
                on_action: songs_handler,
                testid: "search-songs-all".to_string(),
            }
            InsetList { inset: ListInset::Art, label: "Songs".to_string(),
                for (index, song) in songs.iter().cloned().enumerate() {
                    TrackRow {
                        key: "{song.server_id}:{song.id}",
                        playing: playing_key.as_deref() == Some(format!("{}:{}", song.server_id, song.id).as_str()),
                        index,
                        song: song.clone(),
                        on_play: move |_| {
                            on_open.call(());
                            play(song.clone());
                        },
                    }
                }
            }
            if expanded() == Some(Kind::Songs) && results.songs.len() > SONG_CAP {
                p { class: "rs-list-foot", "Showing the best {SONG_CAP} songs. Add a word to narrow it down." }
            }
        }
    }
}

#[component]
fn TopHitCard(hit: TopHit, on_activate: EventHandler<MouseEvent>) -> Element {
    let (title, kind, cover, server_id, round, icon) = match &hit {
        TopHit::Artist(a) => (
            a.name.clone(),
            if a.album_count == 1 { "Artist · 1 album".to_string() } else { format!("Artist · {} albums", a.album_count) },
            a.cover_art.clone(),
            a.server_id.clone(),
            true,
            "artist",
        ),
        TopHit::Album(a) => (
            a.name.clone(),
            format!("Album · {}", a.artist),
            a.cover_art.clone(),
            a.server_id.clone(),
            false,
            "album",
        ),
        TopHit::Song(s) => (
            s.title.clone(),
            format!("Song · {}", s.artist.clone().unwrap_or_default()),
            s.cover_art.clone().or_else(|| s.album_id.clone()),
            s.server_id.clone(),
            false,
            "music",
        ),
    };
    let is_song = matches!(hit, TopHit::Song(_));
    rsx! {
        button {
            r#type: "button",
            class: "rs-search-top",
            "data-testid": "search-top-hit",
            onclick: move |evt| on_activate.call(evt),
            Artwork {
                cover_art: cover,
                server_id,
                size_px: 88,
                round,
                placeholder: icon.to_string(),
            }
            span { class: "rs-search-top__meta",
                span { class: "rs-search-top__title", "{title}" }
                span { class: "rs-search-top__kind", "{kind}" }
            }
            span { class: "rs-search-top__go", aria_hidden: "true",
                if is_song {
                    Icon { name: "play".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                } else {
                    Icon { name: "chevron-right".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                }
            }
        }
    }
}

/// A round artist thumb with name and album count. Also used by the Artists
/// and Favorites views (kept at `views::search::ArtistCard`).
#[component]
pub fn ArtistCard(artist: Artist, onclick: EventHandler<MouseEvent>) -> Element {
    let count = if artist.album_count == 1 {
        "1 album".to_string()
    } else {
        format!("{} albums", artist.album_count)
    };
    rsx! {
        button {
            r#type: "button",
            class: "rs-search-artist",
            "data-testid": "artist-card",
            onclick: move |evt| onclick.call(evt),
            Artwork {
                cover_art: artist.cover_art.clone(),
                server_id: artist.server_id.clone(),
                size_px: 96,
                fill: true,
                round: true,
                placeholder: "artist".to_string(),
                class: "rs-search-artist__art".to_string(),
            }
            span { class: "rs-search-artist__name", "{artist.name}" }
            span { class: "rs-search-artist__count", "{count}" }
        }
    }
}
