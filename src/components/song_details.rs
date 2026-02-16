use crate::api::{
    fetch_lyrics_with_fallback, format_duration, search_lyrics_candidates, LyricLine, LyricsQuery,
    LyricsResult, LyricsSearchCandidate, NavidromeClient, ServerConfig, Song,
};
use crate::components::{
    seek_to, spawn_shuffle_queue, AddIntent, AddMenuController, AppView, AudioState, Icon,
    Navigation, PlaybackPositionSignal, VolumeSignal,
};
use crate::db::{AppSettings, RepeatMode};
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SongDetailsTab {
    Details,
    Queue,
    Related,
    Lyrics,
}

impl SongDetailsTab {
    fn label(self) -> &'static str {
        match self {
            Self::Details => "Details",
            Self::Queue => "Up Next",
            Self::Related => "Related",
            Self::Lyrics => "Lyrics",
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct SongDetailsState {
    pub is_open: bool,
    pub song: Option<Song>,
    pub active_tab: SongDetailsTab,
}

impl Default for SongDetailsState {
    fn default() -> Self {
        Self {
            is_open: false,
            song: None,
            active_tab: SongDetailsTab::Details,
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct SongDetailsController {
    state: Signal<SongDetailsState>,
}

impl SongDetailsController {
    pub fn new(state: Signal<SongDetailsState>) -> Self {
        Self { state }
    }

    pub fn open(&mut self, song: Song) {
        self.state.with_mut(|state| {
            state.is_open = true;
            state.song = Some(song);
        });
    }

    pub fn close(&mut self) {
        self.state.with_mut(|state| {
            state.is_open = false;
        });
    }

    pub fn set_tab(&mut self, tab: SongDetailsTab) {
        self.state.with_mut(|state| {
            state.active_tab = tab;
        });
    }

    pub fn current(&self) -> SongDetailsState {
        (self.state)()
    }
}

const DESKTOP_TABS: [SongDetailsTab; 3] = [
    SongDetailsTab::Lyrics,
    SongDetailsTab::Queue,
    SongDetailsTab::Related,
];
const MOBILE_TABS: [SongDetailsTab; 4] = [
    SongDetailsTab::Details,
    SongDetailsTab::Lyrics,
    SongDetailsTab::Queue,
    SongDetailsTab::Related,
];

#[component]
pub fn SongDetailsOverlay(controller: SongDetailsController) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let audio_state = use_context::<Signal<AudioState>>();
    let create_queue_busy = use_signal(|| false);
    let lyrics_search_title = use_signal(|| None::<String>);
    let lyrics_query_override = use_signal(|| None::<LyricsQuery>);
    let lyrics_candidate_search_term = use_signal(|| None::<String>);
    let lyrics_candidate_refresh_nonce = use_signal(|| 0u64);
    let lyrics_refresh_nonce = use_signal(|| 0u64);
    let lyrics_auto_retry_for_song = use_signal(|| None::<String>);
    let last_song_key = use_signal(|| None::<String>);

    let state = controller.current();
    let selected_song = state.song.clone();
    let selected_song_key = selected_song
        .as_ref()
        .map(|song| format!("{}:{}", song.server_id, song.id));

    {
        let mut controller = controller.clone();
        let now_playing = now_playing.clone();
        use_effect(move || {
            let state = controller.current();
            if !state.is_open {
                return;
            }

            let Some(now_song) = now_playing() else {
                return;
            };

            let should_follow = state.song.as_ref() != Some(&now_song);

            if should_follow {
                controller.open(now_song);
            }
        });
    }

    {
        let mut lyrics_search_title = lyrics_search_title.clone();
        let mut lyrics_query_override = lyrics_query_override.clone();
        let mut lyrics_candidate_search_term = lyrics_candidate_search_term.clone();
        let mut lyrics_auto_retry_for_song = lyrics_auto_retry_for_song.clone();
        let selected_song_key = selected_song_key.clone();
        let mut last_song_key = last_song_key.clone();
        use_effect(move || {
            if last_song_key() != selected_song_key {
                last_song_key.set(selected_song_key.clone());
                lyrics_auto_retry_for_song.set(None);
                lyrics_search_title.set(None);
                lyrics_query_override.set(None);
                lyrics_candidate_search_term.set(None);
            }
        });
    }

    let related_resource = {
        let controller = controller.clone();
        use_resource(move || {
            let song = controller.current().song;
            let servers_snapshot = servers();
            async move { load_related_songs(song, servers_snapshot).await }
        })
    };

    let lyrics_resource = {
        let controller = controller.clone();
        let app_settings = app_settings.clone();
        let lyrics_query_override = lyrics_query_override.clone();
        let lyrics_refresh_nonce = lyrics_refresh_nonce.clone();
        use_resource(move || {
            let song = controller.current().song;
            let settings = app_settings();
            let query_override = lyrics_query_override();
            let _refresh_nonce = lyrics_refresh_nonce();
            async move {
                let Some(song) = song else {
                    return Err("No song selected.".to_string());
                };
                let query = query_override.unwrap_or_else(|| LyricsQuery::from_song(&song));
                fetch_lyrics_with_fallback(
                    &query,
                    &settings.lyrics_provider_order,
                    settings.lyrics_request_timeout_secs,
                )
                .await
            }
        })
    };

    {
        let selected_song_key = selected_song_key.clone();
        let mut lyrics_auto_retry_for_song = lyrics_auto_retry_for_song.clone();
        let mut lyrics_resource = lyrics_resource.clone();
        let mut lyrics_refresh_nonce = lyrics_refresh_nonce.clone();
        use_effect(move || {
            let latest_lyrics_result = lyrics_resource();
            let Some(song_key) = selected_song_key.clone() else {
                return;
            };

            let Some(Err(_)) = latest_lyrics_result else {
                return;
            };

            if lyrics_auto_retry_for_song() == Some(song_key.clone()) {
                return;
            }

            lyrics_auto_retry_for_song.set(Some(song_key));
            lyrics_refresh_nonce.set(lyrics_refresh_nonce().saturating_add(1));
            lyrics_resource.restart();
        });
    }

    let lyrics_candidates_resource = {
        let controller = controller.clone();
        let app_settings = app_settings.clone();
        let lyrics_candidate_search_term = lyrics_candidate_search_term.clone();
        let lyrics_candidate_refresh_nonce = lyrics_candidate_refresh_nonce.clone();
        use_resource(move || {
            let song = controller.current().song;
            let settings = app_settings();
            let search_term = lyrics_candidate_search_term();
            let _refresh_nonce = lyrics_candidate_refresh_nonce();
            async move {
                let Some(song) = song else {
                    return Ok(Vec::<LyricsSearchCandidate>::new());
                };
                let Some(search_term) = search_term
                    .as_ref()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                else {
                    return Ok(Vec::<LyricsSearchCandidate>::new());
                };
                let mut query = LyricsQuery::from_song(&song);
                query.title = search_term;
                search_lyrics_candidates(
                    &query,
                    &settings.lyrics_provider_order,
                    settings.lyrics_request_timeout_secs,
                )
                .await
            }
        })
    };

    if !state.is_open {
        return rsx! {};
    }

    let Some(song) = selected_song else {
        return rsx! {};
    };

    let settings = app_settings();
    let sync_lyrics = !settings.lyrics_unsynced_mode;

    let desktop_tab = match state.active_tab {
        SongDetailsTab::Details => SongDetailsTab::Lyrics,
        other => other,
    };

    let cover_url = song_cover_url(&song, &servers(), 700);

    let queue_snapshot = queue();
    let current_queue_index = queue_index();
    let up_next = queue_snapshot
        .into_iter()
        .enumerate()
        .filter(|(index, _)| *index > current_queue_index)
        .take(60)
        .collect::<Vec<_>>();

    let current_time = (audio_state().current_time)();
    let offset_seconds = settings.lyrics_offset_ms as f64 / 1000.0;

    let song_title = if song.title.trim().is_empty() {
        "Unknown Song".to_string()
    } else {
        song.title.clone()
    };

    rsx! {
        div {
            class: "fixed inset-0 z-[80] bg-zinc-950",
            div {
                class: "w-full h-full border border-zinc-800/80 bg-zinc-950 overflow-hidden flex flex-col",
                div { class: "flex items-center justify-between px-4 md:px-6 py-4 border-b border-zinc-800/80",
                    div {
                        p { class: "text-xs uppercase tracking-[0.2em] text-zinc-500", "Song Menu" }
                        h2 { class: "text-lg md:text-2xl font-semibold text-white truncate", "{song_title}" }
                    }
                    button {
                        class: "p-2 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800/80 transition-colors",
                        onclick: {
                            let mut controller = controller.clone();
                            move |_| controller.close()
                        },
                        Icon { name: "x".to_string(), class: "w-5 h-5".to_string() }
                    }
                }

                div { class: "hidden md:grid grid-cols-12 flex-1 overflow-hidden",
                    div { class: "col-span-4 border-r border-zinc-800/80 p-6 overflow-y-auto",
                        DetailsPanel {
                            song: song.clone(),
                            cover_url: cover_url.clone(),
                        }
                    }

                    div { class: "col-span-8 flex flex-col min-h-0",
                        div { class: "flex items-center gap-2 px-4 py-3 border-b border-zinc-800/70",
                            for tab in DESKTOP_TABS {
                                button {
                                    class: if tab == desktop_tab {
                                        "px-3 py-1.5 rounded-lg bg-emerald-500/20 border border-emerald-500/40 text-emerald-300 text-sm"
                                    } else {
                                        "px-3 py-1.5 rounded-lg border border-zinc-700/60 text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors text-sm"
                                    },
                                    onclick: {
                                        let mut controller = controller.clone();
                                        move |_| controller.set_tab(tab)
                                    },
                                    "{tab.label()}"
                                }
                            }
                        }

                        div { class: "flex-1 min-h-0 p-4",
                            if desktop_tab == SongDetailsTab::Queue {
                                QueuePanel {
                                    up_next: up_next.clone(),
                                    seed_song: song.clone(),
                                    create_queue_busy,
                                }
                            }
                            if desktop_tab == SongDetailsTab::Related {
                                RelatedPanel {
                                    related: related_resource(),
                                }
                            }
                            if desktop_tab == SongDetailsTab::Lyrics {
                                LyricsPanel {
                                    key: "{song.server_id}:{song.id}:desktop",
                                    panel_dom_key: format!("{}:{}:desktop", song.server_id, song.id),
                                    lyrics: lyrics_resource(),
                                    lyrics_candidates: lyrics_candidates_resource(),
                                    lyrics_candidates_search_term: lyrics_candidate_search_term(),
                                    selected_query_override: lyrics_query_override(),
                                    current_time,
                                    offset_seconds,
                                    sync_lyrics,
                                    on_refresh: {
                                        let mut lyrics_resource = lyrics_resource.clone();
                                        let mut lyrics_refresh_nonce = lyrics_refresh_nonce.clone();
                                        move |_| {
                                            lyrics_refresh_nonce.set(lyrics_refresh_nonce().saturating_add(1));
                                            lyrics_resource.restart();
                                        }
                                    },
                                    default_search_title: song.title.clone(),
                                    manual_search_title: lyrics_search_title(),
                                    on_manual_search: {
                                        let mut lyrics_search_title = lyrics_search_title.clone();
                                        let mut lyrics_query_override = lyrics_query_override.clone();
                                        let mut lyrics_candidate_search_term =
                                            lyrics_candidate_search_term.clone();
                                        let mut lyrics_candidate_refresh_nonce =
                                            lyrics_candidate_refresh_nonce.clone();
                                        move |title: String| {
                                            let normalized = title.trim().to_string();
                                            if normalized.is_empty() {
                                                lyrics_search_title.set(None);
                                                lyrics_query_override.set(None);
                                                lyrics_candidate_search_term.set(None);
                                            } else {
                                                lyrics_search_title.set(Some(normalized.clone()));
                                                lyrics_query_override.set(None);
                                                lyrics_candidate_search_term.set(Some(normalized));
                                                lyrics_candidate_refresh_nonce.set(
                                                    lyrics_candidate_refresh_nonce()
                                                        .saturating_add(1),
                                                );
                                            }
                                        }
                                    },
                                    on_select_lyrics_candidate: {
                                        let mut lyrics_query_override = lyrics_query_override.clone();
                                        let mut lyrics_search_title = lyrics_search_title.clone();
                                        let mut lyrics_resource = lyrics_resource.clone();
                                        let mut lyrics_refresh_nonce = lyrics_refresh_nonce.clone();
                                        move |query: LyricsQuery| {
                                            lyrics_search_title.set(Some(query.title.clone()));
                                            lyrics_query_override.set(Some(query));
                                            lyrics_refresh_nonce
                                                .set(lyrics_refresh_nonce().saturating_add(1));
                                            lyrics_resource.restart();
                                        }
                                    },
                                    on_clear_manual_search: {
                                        let mut lyrics_search_title = lyrics_search_title.clone();
                                        let mut lyrics_query_override = lyrics_query_override.clone();
                                        let mut lyrics_candidate_search_term =
                                            lyrics_candidate_search_term.clone();
                                        let mut lyrics_candidate_refresh_nonce =
                                            lyrics_candidate_refresh_nonce.clone();
                                        move |_| {
                                            lyrics_search_title.set(None);
                                            lyrics_query_override.set(None);
                                            lyrics_candidate_search_term.set(None);
                                            lyrics_candidate_refresh_nonce.set(
                                                lyrics_candidate_refresh_nonce().saturating_add(1),
                                            );
                                        }
                                    },
                                }
                            }
                        }
                    }
                }

                div { class: "md:hidden flex-1 overflow-y-auto",
                    div { class: "px-3 py-3 border-b border-zinc-800/80 overflow-x-auto",
                        div { class: "flex items-center gap-2 min-w-max",
                            for tab in MOBILE_TABS {
                                button {
                                    class: if tab == state.active_tab {
                                        "px-3 py-1.5 rounded-lg bg-emerald-500/20 border border-emerald-500/40 text-emerald-300 text-sm"
                                    } else {
                                        "px-3 py-1.5 rounded-lg border border-zinc-700/60 text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors text-sm"
                                    },
                                    onclick: {
                                        let mut controller = controller.clone();
                                        move |_| controller.set_tab(tab)
                                    },
                                    "{tab.label()}"
                                }
                            }
                        }
                    }

                    div { class: "p-3 pb-5 h-full min-h-0",
                        if state.active_tab == SongDetailsTab::Details {
                            DetailsPanel {
                                song: song.clone(),
                                cover_url: cover_url.clone(),
                            }
                        }
                        if state.active_tab == SongDetailsTab::Queue {
                            QueuePanel {
                                up_next: up_next.clone(),
                                seed_song: song.clone(),
                                create_queue_busy,
                            }
                        }
                        if state.active_tab == SongDetailsTab::Related {
                            RelatedPanel {
                                related: related_resource(),
                            }
                        }
                        if state.active_tab == SongDetailsTab::Lyrics {
                            LyricsPanel {
                                key: "{song.server_id}:{song.id}:mobile",
                                panel_dom_key: format!("{}:{}:mobile", song.server_id, song.id),
                                lyrics: lyrics_resource(),
                                lyrics_candidates: lyrics_candidates_resource(),
                                lyrics_candidates_search_term: lyrics_candidate_search_term(),
                                selected_query_override: lyrics_query_override(),
                                current_time,
                                offset_seconds,
                                sync_lyrics,
                                on_refresh: {
                                    let mut lyrics_resource = lyrics_resource.clone();
                                    let mut lyrics_refresh_nonce = lyrics_refresh_nonce.clone();
                                    move |_| {
                                        lyrics_refresh_nonce.set(lyrics_refresh_nonce().saturating_add(1));
                                        lyrics_resource.restart();
                                    }
                                },
                                default_search_title: song.title.clone(),
                                manual_search_title: lyrics_search_title(),
                                on_manual_search: {
                                    let mut lyrics_search_title = lyrics_search_title.clone();
                                    let mut lyrics_query_override = lyrics_query_override.clone();
                                    let mut lyrics_candidate_search_term =
                                        lyrics_candidate_search_term.clone();
                                    let mut lyrics_candidate_refresh_nonce =
                                        lyrics_candidate_refresh_nonce.clone();
                                    move |title: String| {
                                        let normalized = title.trim().to_string();
                                        if normalized.is_empty() {
                                            lyrics_search_title.set(None);
                                            lyrics_query_override.set(None);
                                            lyrics_candidate_search_term.set(None);
                                        } else {
                                            lyrics_search_title.set(Some(normalized.clone()));
                                            lyrics_query_override.set(None);
                                            lyrics_candidate_search_term.set(Some(normalized));
                                            lyrics_candidate_refresh_nonce.set(
                                                lyrics_candidate_refresh_nonce().saturating_add(1),
                                            );
                                        }
                                    }
                                },
                                on_select_lyrics_candidate: {
                                    let mut lyrics_query_override = lyrics_query_override.clone();
                                    let mut lyrics_search_title = lyrics_search_title.clone();
                                    let mut lyrics_resource = lyrics_resource.clone();
                                    let mut lyrics_refresh_nonce = lyrics_refresh_nonce.clone();
                                    move |query: LyricsQuery| {
                                        lyrics_search_title.set(Some(query.title.clone()));
                                        lyrics_query_override.set(Some(query));
                                        lyrics_refresh_nonce
                                            .set(lyrics_refresh_nonce().saturating_add(1));
                                        lyrics_resource.restart();
                                    }
                                },
                                on_clear_manual_search: {
                                    let mut lyrics_search_title = lyrics_search_title.clone();
                                    let mut lyrics_query_override = lyrics_query_override.clone();
                                    let mut lyrics_candidate_search_term =
                                        lyrics_candidate_search_term.clone();
                                    let mut lyrics_candidate_refresh_nonce =
                                        lyrics_candidate_refresh_nonce.clone();
                                    move |_| {
                                        lyrics_search_title.set(None);
                                        lyrics_query_override.set(None);
                                        lyrics_candidate_search_term.set(None);
                                        lyrics_candidate_refresh_nonce.set(
                                            lyrics_candidate_refresh_nonce().saturating_add(1),
                                        );
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct DetailsPanelProps {
    song: Song,
    cover_url: Option<String>,
}

#[component]
fn DetailsPanel(props: DetailsPanelProps) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let add_menu = use_context::<AddMenuController>();
    let navigation = use_context::<Navigation>();
    let controller = use_context::<SongDetailsController>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<Signal<bool>>();
    let repeat_mode = use_context::<Signal<RepeatMode>>();
    let volume = use_context::<VolumeSignal>().0;
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let audio_state = use_context::<Signal<AudioState>>();

    let now_playing_song = now_playing();
    let queue_snapshot = queue();
    let is_selected_song_now_playing = now_playing_song
        .as_ref()
        .map(|current| current.id == props.song.id && current.server_id == props.song.server_id)
        .unwrap_or(false);
    let is_selected_song_favorited = queue_snapshot
        .iter()
        .find(|entry| entry.id == props.song.id && entry.server_id == props.song.server_id)
        .map(|song| song.starred.is_some())
        .or_else(|| {
            now_playing_song
                .as_ref()
                .filter(|song| song.id == props.song.id && song.server_id == props.song.server_id)
                .map(|song| song.starred.is_some())
        })
        .unwrap_or(props.song.starred.is_some());
    let currently_playing = is_playing();
    let current_repeat_mode = repeat_mode();
    let queue_len = queue_snapshot.len();
    let can_prev = queue_index() > 0;
    let can_next = queue_index().saturating_add(1) < queue_len
        || (current_repeat_mode == RepeatMode::All && queue_len > 0)
        || current_repeat_mode == RepeatMode::Off
        || (current_repeat_mode == RepeatMode::One && now_playing_song.is_some());
    let now_playing_rating = now_playing_song
        .as_ref()
        .and_then(|song| song.user_rating)
        .unwrap_or(0)
        .min(5);
    let current_time = (audio_state().current_time)();
    let duration = (audio_state().duration)();
    let display_duration = if duration > 0.0 {
        duration
    } else {
        now_playing_song
            .as_ref()
            .map(|song| song.duration as f64)
            .unwrap_or(0.0)
    };
    let playback_percent = if display_duration > 0.0 {
        ((current_time / display_duration) * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    let song_artist = props
        .song
        .artist
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Unknown Artist".to_string());
    let song_album = props
        .song
        .album
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Unknown Album".to_string());

    let on_open_artist = {
        let mut controller = controller.clone();
        let navigation = navigation.clone();
        let artist_id = props.song.artist_id.clone();
        let server_id = props.song.server_id.clone();
        move |_| {
            if let Some(artist_id) = artist_id.clone() {
                controller.close();
                navigation.navigate_to(AppView::ArtistDetailView {
                    artist_id,
                    server_id: server_id.clone(),
                });
            }
        }
    };

    let on_open_album = {
        let mut controller = controller.clone();
        let navigation = navigation.clone();
        let album_id = props.song.album_id.clone();
        let server_id = props.song.server_id.clone();
        move |_| {
            if let Some(album_id) = album_id.clone() {
                controller.close();
                navigation.navigate_to(AppView::AlbumDetailView {
                    album_id,
                    server_id: server_id.clone(),
                });
            }
        }
    };

    let on_open_album_cover = {
        let mut controller = controller.clone();
        let navigation = navigation.clone();
        let album_id = props.song.album_id.clone();
        let server_id = props.song.server_id.clone();
        move |_| {
            if let Some(album_id) = album_id.clone() {
                controller.close();
                navigation.navigate_to(AppView::AlbumDetailView {
                    album_id,
                    server_id: server_id.clone(),
                });
            }
        }
    };

    let on_toggle_selected_playback = {
        let song = props.song.clone();
        let mut queue = queue.clone();
        let mut queue_index = queue_index.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let is_selected_song_now_playing = is_selected_song_now_playing;
        move |_| {
            if is_selected_song_now_playing {
                is_playing.set(!is_playing());
                return;
            }
            let song_for_queue = song.clone();
            let mut found_index = None;
            queue.with_mut(|items| {
                if let Some(existing_index) = items.iter().position(|entry| {
                    entry.id == song_for_queue.id && entry.server_id == song_for_queue.server_id
                }) {
                    found_index = Some(existing_index);
                } else {
                    items.push(song_for_queue.clone());
                    found_index = Some(items.len().saturating_sub(1));
                }
            });

            if let Some(index) = found_index {
                queue_index.set(index);
                now_playing.set(Some(song.clone()));
                is_playing.set(true);
            }
        }
    };

    let on_prev_song = {
        let mut queue_index = queue_index.clone();
        let queue = queue.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        move |_| {
            let idx = queue_index();
            if idx == 0 {
                return;
            }
            let next_idx = idx - 1;
            if let Some(song) = queue().get(next_idx).cloned() {
                queue_index.set(next_idx);
                now_playing.set(Some(song));
                is_playing.set(true);
            }
        }
    };

    let on_next_song = {
        let servers = servers.clone();
        let mut queue_index = queue_index.clone();
        let queue = queue.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let repeat_mode = repeat_mode.clone();
        let seed_song = props.song.clone();
        move |_| {
            let was_playing = is_playing();
            let repeat = repeat_mode();
            if repeat == RepeatMode::One {
                seek_to(0.0);
                if was_playing {
                    is_playing.set(true);
                }
                return;
            }
            let idx = queue_index();
            let next_idx = idx.saturating_add(1);
            let queue_list = queue();
            if repeat == RepeatMode::Off
                && (queue_list.is_empty() || idx >= queue_list.len().saturating_sub(1))
            {
                let seed = now_playing().or(Some(seed_song.clone()));
                spawn_shuffle_queue(
                    servers(),
                    queue.clone(),
                    queue_index.clone(),
                    now_playing.clone(),
                    is_playing.clone(),
                    seed,
                    Some(was_playing),
                );
                return;
            }
            if let Some(song) = queue_list.get(next_idx).cloned() {
                queue_index.set(next_idx);
                now_playing.set(Some(song));
                is_playing.set(true);
            } else if repeat == RepeatMode::All && !queue_list.is_empty() {
                if let Some(song) = queue_list.first().cloned() {
                    queue_index.set(0);
                    now_playing.set(Some(song));
                    is_playing.set(true);
                }
            }
        }
    };

    let on_seek_now_playing = {
        let mut playback_position = playback_position.clone();
        let mut audio_state = audio_state.clone();
        move |evt: Event<FormData>| {
            let Ok(value) = evt.value().parse::<f64>() else {
                return;
            };
            let duration = (audio_state().duration)();
            if duration <= 0.0 {
                return;
            }
            let target = (value.clamp(0.0, 100.0) / 100.0) * duration;
            playback_position.set(target);
            audio_state.write().current_time.set(target);
            seek_to(target);
        }
    };

    let on_volume_change = {
        let mut volume = volume.clone();
        move |evt: Event<FormData>| {
            if let Ok(value) = evt.value().parse::<f64>() {
                volume.set((value / 100.0).clamp(0.0, 1.0));
            }
        }
    };

    let on_toggle_song_favorite = {
        let song = props.song.clone();
        let servers = servers.clone();
        let now_playing = now_playing.clone();
        let queue = queue.clone();
        let should_star = !is_selected_song_favorited;
        move |_| {
            toggle_song_favorite(
                song.clone(),
                should_star,
                servers.clone(),
                now_playing.clone(),
                queue.clone(),
            );
        }
    };

    let on_add_to_playlist = {
        let mut add_menu = add_menu.clone();
        let song = props.song.clone();
        move |_| {
            add_menu.open(AddIntent::from_song(song.clone()));
        }
    };
    let on_cycle_loop = {
        let mut repeat_mode = repeat_mode.clone();
        move |_| {
            let next = match repeat_mode() {
                RepeatMode::One => RepeatMode::Off,
                RepeatMode::Off | RepeatMode::All => RepeatMode::One,
            };
            repeat_mode.set(next);
        }
    };

    rsx! {
        div { class: "space-y-5",
            div { class: "flex justify-center",
                if props.song.album_id.is_some() {
                    button {
                        class: "w-full max-w-md aspect-square rounded-2xl border border-zinc-800/80 overflow-hidden bg-zinc-900/60 shadow-2xl hover:ring-2 hover:ring-emerald-500/50 transition-all",
                        onclick: on_open_album_cover,
                        title: "Open album",
                        {
                            match props.cover_url.clone() {
                                Some(url) => rsx! {
                                    img {
                                        src: "{url}",
                                        alt: "{props.song.title}",
                                        class: "w-full h-full object-cover",
                                        loading: "lazy",
                                    }
                                },
                                None => rsx! {
                                    div { class: "w-full h-full bg-gradient-to-br from-zinc-800 to-zinc-900 flex items-center justify-center",
                                        Icon { name: "music".to_string(), class: "w-20 h-20 text-zinc-600".to_string() }
                                    }
                                },
                            }
                        }
                    }
                } else {
                    div { class: "w-full max-w-md aspect-square rounded-2xl border border-zinc-800/80 overflow-hidden bg-zinc-900/60 shadow-2xl",
                        {
                            match props.cover_url.clone() {
                                Some(url) => rsx! {
                                    img {
                                        src: "{url}",
                                        alt: "{props.song.title}",
                                        class: "w-full h-full object-cover",
                                        loading: "lazy",
                                    }
                                },
                                None => rsx! {
                                    div { class: "w-full h-full bg-gradient-to-br from-zinc-800 to-zinc-900 flex items-center justify-center",
                                        Icon { name: "music".to_string(), class: "w-20 h-20 text-zinc-600".to_string() }
                                    }
                                },
                            }
                        }
                    }
                }
            }

            div { class: "space-y-3 text-center",
                h3 { class: "text-xl md:text-2xl font-semibold text-white leading-tight break-words", "{props.song.title}" }
                div { class: "space-y-1 pt-1",
                    p { class: "text-[10px] uppercase tracking-[0.18em] text-zinc-500", "Artist" }
                    if props.song.artist_id.is_some() {
                        button {
                            class: "text-sm text-emerald-300 hover:text-emerald-200 transition-colors whitespace-normal break-words leading-snug",
                            onclick: on_open_artist,
                            "{song_artist}"
                        }
                    } else {
                        p { class: "text-sm text-zinc-300 whitespace-normal break-words leading-snug", "{song_artist}" }
                    }
                }
                div { class: "space-y-1 pt-3 border-t border-zinc-800/70",
                    p { class: "text-[10px] uppercase tracking-[0.18em] text-zinc-500", "Album" }
                    if props.song.album_id.is_some() {
                        button {
                            class: "text-sm text-zinc-300 hover:text-white transition-colors whitespace-normal break-words leading-snug",
                            onclick: on_open_album,
                            "{song_album}"
                        }
                    } else {
                        p { class: "text-sm text-zinc-400 whitespace-normal break-words leading-snug", "{song_album}" }
                    }
                }
            }

            div { class: "grid grid-cols-3 gap-2 text-center",
                div { class: "rounded-xl border border-zinc-800/80 bg-zinc-900/50 p-3",
                    p { class: "text-[10px] uppercase tracking-wider text-zinc-500", "Duration" }
                    p { class: "text-sm text-zinc-200 mt-1", "{format_duration(props.song.duration)}" }
                }
                div { class: "rounded-xl border border-zinc-800/80 bg-zinc-900/50 p-3",
                    p { class: "text-[10px] uppercase tracking-wider text-zinc-500", "Server" }
                    p { class: "text-sm text-zinc-200 mt-1 truncate", "{props.song.server_name}" }
                }
                div { class: "rounded-xl border border-zinc-800/80 bg-zinc-900/50 p-3",
                    p { class: "text-[10px] uppercase tracking-wider text-zinc-500", "Track" }
                    p { class: "text-sm text-zinc-200 mt-1", "{props.song.track.unwrap_or(0)}" }
                }
            }

            div { class: "rounded-2xl border border-zinc-800/80 bg-zinc-900/50 p-3 space-y-3",
                div { class: "flex items-center justify-between gap-2",
                    p { class: "text-sm font-medium text-white", "Now Playing Controls" }
                    if is_selected_song_now_playing {
                        span { class: "text-[10px] uppercase tracking-wider text-emerald-300", "This Song Is Playing" }
                    } else if now_playing_song.is_some() {
                        span { class: "text-[10px] uppercase tracking-wider text-zinc-500", "Playing Another Song" }
                    } else {
                        span { class: "text-[10px] uppercase tracking-wider text-zinc-500", "Idle" }
                    }
                }

                if let Some(current_song) = now_playing_song.clone() {
                    div { class: "space-y-1",
                        p { class: "text-sm text-zinc-100 truncate", "{current_song.title}" }
                        p { class: "text-xs text-zinc-500 truncate",
                            "{current_song.artist.clone().unwrap_or_default()}"
                        }
                    }

                    div { class: "space-y-1",
                        div { class: "flex items-center justify-between text-xs text-zinc-500",
                            span { "{format_duration(current_time as u32)}" }
                            span { "{format_duration(display_duration.max(0.0) as u32)}" }
                        }
                        input {
                            r#type: "range",
                            min: "0",
                            max: "100",
                            value: playback_percent.round() as i32,
                            disabled: display_duration <= 0.0,
                            class: "w-full h-1.5 bg-zinc-800 rounded-full appearance-none cursor-pointer accent-emerald-500 disabled:opacity-40 disabled:cursor-not-allowed",
                            oninput: on_seek_now_playing,
                            onchange: on_seek_now_playing,
                        }
                    }

                    div { class: "space-y-2",
                        div { class: "flex items-center gap-2",
                            Icon {
                                name: if volume() > 0.5 { "volume-2".to_string() } else if volume() > 0.0 { "volume-1".to_string() } else { "volume-x".to_string() },
                                class: "w-4 h-4 text-zinc-400".to_string(),
                            }
                            input {
                                r#type: "range",
                                min: "0",
                                max: "100",
                                value: (volume() * 100.0).round() as i32,
                                class: "flex-1 h-1.5 bg-zinc-800 rounded-full appearance-none cursor-pointer accent-zinc-400",
                                oninput: on_volume_change,
                                onchange: on_volume_change,
                            }
                            span { class: "text-xs text-zinc-500 w-10 text-right",
                                "{(volume() * 100.0).round() as i32}%"
                            }
                        }

                        div { class: "flex items-center justify-between gap-3",
                            div { class: "flex items-center gap-2",
                                button {
                                    class: if is_selected_song_favorited {
                                        "p-2 rounded-full border border-emerald-500/50 text-emerald-300 hover:text-emerald-200 transition-colors"
                                    } else {
                                        "p-2 rounded-full border border-zinc-700 text-zinc-400 hover:text-white transition-colors"
                                    },
                                    onclick: on_toggle_song_favorite,
                                    title: if is_selected_song_favorited { "Unfavorite song" } else { "Favorite song" },
                                    Icon {
                                        name: if is_selected_song_favorited { "heart-filled".to_string() } else { "heart".to_string() },
                                        class: "w-4 h-4".to_string(),
                                    }
                                }
                                button {
                                    class: "p-2 rounded-full border border-zinc-700 text-zinc-400 hover:text-white transition-colors",
                                    onclick: on_add_to_playlist,
                                    title: "Add to playlist",
                                    Icon { name: "playlist".to_string(), class: "w-4 h-4".to_string() }
                                }
                                button {
                                    class: if current_repeat_mode == RepeatMode::One {
                                        "p-2 rounded-full border border-emerald-500/50 text-emerald-300 hover:text-emerald-200 transition-colors"
                                    } else {
                                        "p-2 rounded-full border border-zinc-700 text-zinc-400 hover:text-white transition-colors"
                                    },
                                    onclick: on_cycle_loop,
                                    title: if current_repeat_mode == RepeatMode::One { "Loop one (on)" } else { "Loop one (off)" },
                                    Icon {
                                        name: if current_repeat_mode == RepeatMode::One { "repeat-1".to_string() } else { "repeat".to_string() },
                                        class: "w-4 h-4".to_string(),
                                    }
                                }
                            }

                            div { class: "flex items-center gap-1",
                                for value in 1u32..=5u32 {
                                    button {
                                        class: if value <= now_playing_rating {
                                            "text-amber-400 hover:text-amber-300 transition-colors"
                                        } else {
                                            "text-zinc-500 hover:text-zinc-300 transition-colors"
                                        },
                                        onclick: {
                                            let servers = servers.clone();
                                            let now_playing = now_playing.clone();
                                            let queue = queue.clone();
                                            move |_| {
                                                set_now_playing_rating(
                                                    servers.clone(),
                                                    now_playing.clone(),
                                                    queue.clone(),
                                                    value,
                                                );
                                            }
                                        },
                                        Icon {
                                            name: if value <= now_playing_rating { "star-filled".to_string() } else { "star".to_string() },
                                            class: "w-4 h-4".to_string(),
                                        }
                                    }
                                }
                                button {
                                    class: "ml-1 text-[11px] text-zinc-500 hover:text-zinc-300 transition-colors",
                                    onclick: {
                                        let servers = servers.clone();
                                        let now_playing = now_playing.clone();
                                        let queue = queue.clone();
                                        move |_| {
                                            set_now_playing_rating(
                                                servers.clone(),
                                                now_playing.clone(),
                                                queue.clone(),
                                                0,
                                            );
                                        }
                                    },
                                    "Clear"
                                }
                            }
                        }
                    }
                } else {
                    p { class: "text-sm text-zinc-500", "No song is currently playing. Start playback to unlock progress and rating controls." }
                }
            }

            div { class: "grid grid-cols-3 gap-3",
                button {
                    class: if can_prev {
                        "h-11 rounded-xl border border-zinc-700 text-zinc-300 hover:text-white hover:border-zinc-500 transition-colors flex items-center justify-center"
                    } else {
                        "h-11 rounded-xl border border-zinc-800 text-zinc-600 cursor-not-allowed flex items-center justify-center"
                    },
                    disabled: !can_prev,
                    onclick: on_prev_song,
                    Icon { name: "prev".to_string(), class: "w-5 h-5".to_string() }
                }
                button {
                    class: "h-11 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-white transition-colors flex items-center justify-center",
                    onclick: on_toggle_selected_playback,
                    Icon {
                        name: if is_selected_song_now_playing && currently_playing { "pause".to_string() } else { "play".to_string() },
                        class: "w-5 h-5".to_string(),
                    }
                }
                button {
                    class: if can_next {
                        "h-11 rounded-xl border border-zinc-700 text-zinc-300 hover:text-white hover:border-zinc-500 transition-colors flex items-center justify-center"
                    } else {
                        "h-11 rounded-xl border border-zinc-800 text-zinc-600 cursor-not-allowed flex items-center justify-center"
                    },
                    disabled: !can_next,
                    onclick: on_next_song,
                    Icon { name: "next".to_string(), class: "w-5 h-5".to_string() }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct QueuePanelProps {
    up_next: Vec<(usize, Song)>,
    seed_song: Song,
    create_queue_busy: Signal<bool>,
}

#[component]
fn QueuePanel(props: QueuePanelProps) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<Signal<bool>>();
    let controller = use_context::<SongDetailsController>();
    let drag_source_index = use_signal(|| None::<usize>);

    let on_create_queue = {
        let seed_song = props.seed_song.clone();
        let mut create_queue_busy = props.create_queue_busy.clone();
        let servers = servers.clone();
        let mut queue = queue.clone();
        let mut queue_index = queue_index.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        move |_| {
            if create_queue_busy() {
                return;
            }
            create_queue_busy.set(true);
            let seed_song = seed_song.clone();
            let servers_snapshot = servers();
            spawn(async move {
                let generated = build_queue_from_seed(seed_song.clone(), servers_snapshot).await;
                if generated.is_empty() {
                    queue.set(vec![seed_song.clone()]);
                    queue_index.set(0);
                    now_playing.set(Some(seed_song));
                } else {
                    queue.set(generated.clone());
                    queue_index.set(0);
                    now_playing.set(generated.first().cloned());
                }
                is_playing.set(true);
                create_queue_busy.set(false);
            });
        }
    };
    let servers_snapshot = servers();

    if props.up_next.is_empty() {
        return rsx! {
            div { class: "h-full flex flex-col items-center justify-center text-center px-4 gap-3",
                p { class: "text-zinc-400 text-sm", "Queue is empty for this track." }
                p { class: "text-zinc-500 text-xs", "Generate a queue from similar songs and keep this song as the seed." }
                button {
                    class: if (props.create_queue_busy)() {
                        "px-4 py-2 rounded-xl bg-zinc-700 text-zinc-300 text-sm cursor-not-allowed"
                    } else {
                        "px-4 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-white text-sm transition-colors"
                    },
                    disabled: (props.create_queue_busy)(),
                    onclick: on_create_queue,
                    if (props.create_queue_busy)() {
                        "Creating queue..."
                    } else {
                        "Create Queue From This Song"
                    }
                }
            }
        };
    }

    rsx! {
        div { class: "h-full overflow-y-auto pr-1 space-y-2",
            for (index, entry) in props.up_next.iter() {
                div {
                    key: "{entry.server_id}:{entry.id}:{index}",
                    draggable: true,
                    class: if drag_source_index() == Some(*index) {
                        "w-full rounded-xl border border-emerald-500/50 bg-emerald-500/10 opacity-70"
                    } else {
                        "w-full rounded-xl border border-zinc-800/80"
                    },
                    ondragstart: {
                        let mut drag_source_index = drag_source_index.clone();
                        let source_index = *index;
                        move |_| {
                            drag_source_index.set(Some(source_index));
                        }
                    },
                    ondragend: {
                        let mut drag_source_index = drag_source_index.clone();
                        move |_| {
                            drag_source_index.set(None);
                        }
                    },
                    ondragover: move |evt| {
                        evt.prevent_default();
                    },
                    ondrop: {
                        let mut drag_source_index = drag_source_index.clone();
                        let queue = queue.clone();
                        let queue_index = queue_index.clone();
                        let now_playing = now_playing.clone();
                        let target_index = *index;
                        move |evt| {
                            evt.prevent_default();
                            let Some(source_index) = drag_source_index() else {
                                return;
                            };
                            drag_source_index.set(None);
                            reorder_queue_entry(
                                queue.clone(),
                                queue_index.clone(),
                                now_playing.clone(),
                                source_index,
                                target_index,
                            );
                        }
                    },
                    div { class: "flex items-center gap-2 p-3",
                        button {
                            class: "flex-1 text-left flex items-center gap-3 hover:bg-emerald-500/5 rounded-lg px-1 py-1 transition-colors min-w-0",
                            onclick: {
                                let selected_song = entry.clone();
                                let mut queue_index = queue_index.clone();
                                let mut now_playing = now_playing.clone();
                                let mut is_playing = is_playing.clone();
                                let mut controller = controller.clone();
                                let index = *index;
                                move |_| {
                                    queue_index.set(index);
                                    now_playing.set(Some(selected_song.clone()));
                                    is_playing.set(true);
                                    controller.open(selected_song.clone());
                                }
                            },
                            span { class: "w-6 text-xs text-zinc-500 text-right font-mono flex-shrink-0",
                                "{index + 1}"
                            }
                            {
                                match song_cover_url(entry, &servers_snapshot, 96) {
                                    Some(url) => rsx! {
                                        img {
                                            src: "{url}",
                                            alt: "{entry.title}",
                                            class: "w-10 h-10 rounded-md object-cover border border-zinc-800/80 flex-shrink-0",
                                            loading: "lazy",
                                        }
                                    },
                                    None => rsx! {
                                        div { class: "w-10 h-10 rounded-md bg-zinc-800 flex items-center justify-center text-zinc-500 border border-zinc-800/80 flex-shrink-0",
                                            Icon { name: "music".to_string(), class: "w-4 h-4".to_string() }
                                        }
                                    },
                                }
                            }
                            div { class: "min-w-0 flex-1",
                                p { class: "text-sm text-white truncate", "{entry.title}" }
                                p { class: "text-xs text-zinc-500 truncate",
                                    "{entry.artist.clone().unwrap_or_default()}"
                                }
                            }
                            span { class: "text-xs text-zinc-500 font-mono flex-shrink-0",
                                "{format_duration(entry.duration)}"
                            }
                        }
                        button {
                            class: "p-2 rounded-lg border border-zinc-800/80 text-zinc-500 hover:text-red-400 hover:border-red-500/40 transition-colors",
                            title: "Remove from queue",
                            onclick: {
                                let queue = queue.clone();
                                let queue_index = queue_index.clone();
                                let now_playing = now_playing.clone();
                                let is_playing = is_playing.clone();
                                let remove_index = *index;
                                move |evt: MouseEvent| {
                                    evt.stop_propagation();
                                    remove_queue_entry(
                                        queue.clone(),
                                        queue_index.clone(),
                                        now_playing.clone(),
                                        is_playing.clone(),
                                        remove_index,
                                    );
                                }
                            },
                            Icon { name: "x".to_string(), class: "w-4 h-4".to_string() }
                        }
                        div {
                            class: "px-1 text-zinc-600 cursor-grab active:cursor-grabbing select-none",
                            title: "Drag to reorder",
                            Icon { name: "bars".to_string(), class: "w-4 h-4".to_string() }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RelatedPanelProps {
    related: Option<Vec<Song>>,
}

#[component]
fn RelatedPanel(props: RelatedPanelProps) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<Signal<bool>>();
    let controller = use_context::<SongDetailsController>();

    let Some(related) = props.related.clone() else {
        return rsx! {
            div { class: "h-full flex items-center justify-center text-zinc-500 text-sm gap-2",
                Icon { name: "loader".to_string(), class: "w-4 h-4".to_string() }
                "Loading related songs..."
            }
        };
    };

    if related.is_empty() {
        return rsx! {
            div { class: "h-full flex items-center justify-center text-zinc-500 text-sm",
                "No related songs found for this track."
            }
        };
    }
    let servers_snapshot = servers();

    rsx! {
        div { class: "h-full overflow-y-auto pr-1 space-y-2",
            for related_song in related {
                button {
                    class: "w-full text-left flex items-center gap-3 p-3 rounded-xl border border-zinc-800/80 hover:border-emerald-500/40 hover:bg-emerald-500/5 transition-colors",
                    onclick: {
                        let related_song = related_song.clone();
                        let mut queue = queue.clone();
                        let mut queue_index = queue_index.clone();
                        let mut now_playing = now_playing.clone();
                        let mut is_playing = is_playing.clone();
                        let mut controller = controller.clone();
                        move |_| {
                            let song_for_queue = related_song.clone();
                            let mut found_index = None;
                            queue.with_mut(|items| {
                                if let Some(existing_index) = items.iter().position(|entry| {
                                    entry.id == song_for_queue.id
                                        && entry.server_id == song_for_queue.server_id
                                }) {
                                    found_index = Some(existing_index);
                                } else {
                                    items.push(song_for_queue.clone());
                                    found_index = Some(items.len().saturating_sub(1));
                                }
                            });
                            if let Some(index) = found_index {
                                queue_index.set(index);
                                now_playing.set(Some(related_song.clone()));
                                is_playing.set(true);
                                controller.open(related_song.clone());
                            }
                        }
                    },
                    {
                        match song_cover_url(&related_song, &servers_snapshot, 96) {
                            Some(url) => rsx! {
                                img {
                                    src: "{url}",
                                    alt: "{related_song.title}",
                                    class: "w-10 h-10 rounded-md object-cover border border-zinc-800/80 flex-shrink-0",
                                    loading: "lazy",
                                }
                            },
                            None => rsx! {
                                div { class: "w-10 h-10 rounded-md bg-zinc-800 flex items-center justify-center text-zinc-500 border border-zinc-800/80 flex-shrink-0",
                                    Icon { name: "music".to_string(), class: "w-4 h-4".to_string() }
                                }
                            },
                        }
                    }
                    div { class: "min-w-0 flex-1",
                        p { class: "text-sm text-white truncate", "{related_song.title}" }
                        p { class: "text-xs text-zinc-500 truncate",
                            "{related_song.artist.clone().unwrap_or_default()}"
                        }
                    }
                    span { class: "text-xs text-zinc-500 font-mono", "{format_duration(related_song.duration)}" }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct LyricsPanelProps {
    panel_dom_key: String,
    lyrics: Option<Result<LyricsResult, String>>,
    lyrics_candidates: Option<Result<Vec<LyricsSearchCandidate>, String>>,
    lyrics_candidates_search_term: Option<String>,
    selected_query_override: Option<LyricsQuery>,
    current_time: f64,
    offset_seconds: f64,
    sync_lyrics: bool,
    on_refresh: EventHandler<MouseEvent>,
    default_search_title: String,
    manual_search_title: Option<String>,
    on_manual_search: EventHandler<String>,
    on_select_lyrics_candidate: EventHandler<LyricsQuery>,
    on_clear_manual_search: EventHandler<MouseEvent>,
}

#[component]
fn LyricsPanel(props: LyricsPanelProps) -> Element {
    let navigation = use_context::<Navigation>();
    let controller = use_context::<SongDetailsController>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let audio_state = use_context::<Signal<AudioState>>();
    let search_panel_open = use_signal(|| false);
    let programmatic_scroll_until_ms = use_signal(|| 0.0_f64);
    let manual_scroll_hold_until_ms = use_signal(|| 0.0_f64);
    let last_centered_index = use_signal(|| None::<usize>);

    let on_open_settings = {
        let navigation = navigation.clone();
        let mut controller = controller.clone();
        move |_| {
            controller.close();
            navigation.navigate_to(AppView::SettingsView {});
        }
    };

    let mut search_input = use_signal(|| {
        props
            .manual_search_title
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| props.default_search_title.clone())
    });

    let on_toggle_search_panel = {
        let mut search_panel_open = search_panel_open.clone();
        move |_| {
            search_panel_open.set(!search_panel_open());
        }
    };

    let on_search_submit = {
        let search_input = search_input.clone();
        let on_manual_search = props.on_manual_search.clone();
        move |_| {
            on_manual_search.call(search_input().trim().to_string());
        }
    };

    let on_use_current_song = {
        let mut search_input = search_input.clone();
        let default_search_title = props.default_search_title.clone();
        let on_clear_manual_search = props.on_clear_manual_search.clone();
        move |evt: MouseEvent| {
            search_input.set(default_search_title.clone());
            on_clear_manual_search.call(evt);
        }
    };

    let mut on_pick_candidate = {
        let on_select_lyrics_candidate = props.on_select_lyrics_candidate.clone();
        let mut search_panel_open = search_panel_open.clone();
        move |query: LyricsQuery| {
            on_select_lyrics_candidate.call(query);
            search_panel_open.set(false);
        }
    };

    {
        let mut search_input = search_input.clone();
        let manual_search_title = props.manual_search_title.clone();
        let default_search_title = props.default_search_title.clone();
        use_effect(move || {
            let next_value = manual_search_title
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| default_search_title.clone());
            if search_input() != next_value {
                search_input.set(next_value);
            }
        });
    }

    let mut on_seek_line = {
        let mut playback_position = playback_position.clone();
        let mut audio_state = audio_state.clone();
        let offset_seconds = props.offset_seconds;
        let sync_lyrics = props.sync_lyrics;
        move |line: LyricLine| {
            if !sync_lyrics {
                return;
            }
            let target = (line.timestamp_seconds - offset_seconds).max(0.0);
            playback_position.set(target);
            audio_state.write().current_time.set(target);
            seek_to(target);
        }
    };

    let last_successful_lyrics = use_signal(|| None::<LyricsResult>);
    {
        let mut last_successful_lyrics = last_successful_lyrics.clone();
        let latest_lyrics = props.lyrics.clone();
        use_effect(move || {
            if let Some(Ok(lyrics)) = latest_lyrics.as_ref() {
                if last_successful_lyrics().as_ref() != Some(lyrics) {
                    last_successful_lyrics.set(Some(lyrics.clone()));
                }
            }
        });
    }

    let fetch_error = match props.lyrics.clone() {
        Some(Err(error)) => Some(error),
        _ => None,
    };
    let display_lyrics = match props.lyrics.clone() {
        Some(Ok(lyrics)) => Some(lyrics),
        Some(Err(_)) | None => last_successful_lyrics(),
    };
    let playback_seconds_signal = playback_position();
    let playback_seconds = if (props.current_time - playback_seconds_signal).abs() > 1.0 {
        props.current_time
    } else {
        playback_seconds_signal
    };
    let active_synced_index = if !props.sync_lyrics {
        None
    } else {
        display_lyrics.as_ref().and_then(|lyrics| {
            active_lyric_index(
                &lyrics.synced_lines,
                playback_seconds + props.offset_seconds,
            )
        })
    };

    let scroll_container_id = format!(
        "lyrics-scroll-{}",
        sanitize_dom_id(&props.panel_dom_key)
    );

    let on_lyrics_scrolled = {
        let programmatic_scroll_until_ms = programmatic_scroll_until_ms.clone();
        let mut manual_scroll_hold_until_ms = manual_scroll_hold_until_ms.clone();
        let mut last_centered_index = last_centered_index.clone();
        move |_| {
            let now = now_millis();
            if now < programmatic_scroll_until_ms() {
                return;
            }
            manual_scroll_hold_until_ms.set(now + 1800.0);
            last_centered_index.set(None);
        }
    };

    {
        let active_synced_index = active_synced_index;
        let scroll_container_id = scroll_container_id.clone();
        let sync_lyrics = props.sync_lyrics;
        let audio_state = audio_state.clone();
        let mut programmatic_scroll_until_ms = programmatic_scroll_until_ms.clone();
        let manual_scroll_hold_until_ms = manual_scroll_hold_until_ms.clone();
        let mut last_centered_index = last_centered_index.clone();
        use_effect(move || {
            let _playback_tick = (audio_state().current_time)();
            let Some(index) = active_synced_index else {
                return;
            };
            if !sync_lyrics {
                return;
            }
            if now_millis() < manual_scroll_hold_until_ms() {
                return;
            }

            let should_recenter = last_centered_index() != Some(index);
            if !should_recenter {
                return;
            }

            let line_id = format!("{scroll_container_id}-line-{index}");
            let script = format!(
                r#"(function() {{
                    const container = document.getElementById("{scroll_container_id}");
                    const line = document.getElementById("{line_id}");
                    if (!container || !line) return;
                    const cRect = container.getBoundingClientRect();
                    const lRect = line.getBoundingClientRect();
                    const target = container.scrollTop + (lRect.top - cRect.top) - (cRect.height / 2) + (lRect.height / 2);
                    container.scrollTo({{ top: target, behavior: "auto" }});
                }})();"#
            );
            let _ = document::eval(&script);
            programmatic_scroll_until_ms.set(now_millis() + 250.0);
            last_centered_index.set(Some(index));
        });
    }

    rsx! {
        div { class: "space-y-4",
            div { class: "flex items-center justify-between gap-2",
                button {
                    class: if search_panel_open() {
                        "px-3 py-1.5 rounded-lg border border-emerald-500/50 text-emerald-300 hover:text-emerald-200 text-xs transition-colors flex items-center gap-2"
                    } else {
                        "px-3 py-1.5 rounded-lg border border-zinc-700/70 text-zinc-300 hover:text-white text-xs transition-colors flex items-center gap-2"
                    },
                    onclick: on_toggle_search_panel,
                    Icon { name: "search".to_string(), class: "w-3.5 h-3.5".to_string() }
                    if search_panel_open() {
                        "Close Search"
                    } else {
                        "Find Lyrics"
                    }
                }
                div { class: "flex items-center gap-2",
                    button {
                        class: "px-3 py-1.5 rounded-lg border border-zinc-700/70 text-zinc-300 hover:text-white text-xs transition-colors",
                        onclick: move |evt| props.on_refresh.call(evt),
                        "Refresh"
                    }
                    button {
                        class: "px-3 py-1.5 rounded-lg bg-emerald-500/20 border border-emerald-500/40 text-emerald-300 hover:text-emerald-200 text-xs transition-colors",
                        onclick: on_open_settings,
                        "Lyrics Settings"
                    }
                }
            }

            if search_panel_open() {
                div { class: "rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-3 space-y-3",
                    p { class: "text-xs uppercase tracking-wider text-zinc-500", "Search And Pick Lyrics" }
                    div { class: "flex flex-col sm:flex-row gap-2",
                        input {
                            r#type: "text",
                            value: "{search_input}",
                            placeholder: "Enter a song title",
                            class: "flex-1 px-3 py-2 rounded-lg border border-zinc-700 bg-zinc-950 text-zinc-100 text-sm focus:outline-none focus:border-emerald-500/50",
                            oninput: move |evt| search_input.set(evt.value()),
                        }
                        button {
                            class: "px-3 py-2 rounded-lg bg-emerald-500 hover:bg-emerald-400 text-white text-sm transition-colors",
                            onclick: on_search_submit,
                            "Search"
                        }
                        button {
                            class: "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-400 hover:text-white transition-colors text-sm",
                            onclick: on_use_current_song,
                            "Use Current Song"
                        }
                    }

                    if let Some(search_term) = props
                        .lyrics_candidates_search_term
                        .clone()
                        .filter(|value| !value.trim().is_empty())
                    {
                        div { class: "rounded-lg border border-zinc-800/80 bg-zinc-950/60 p-2 space-y-2",
                            p { class: "text-[11px] text-zinc-500 px-1",
                                "Pick the best match for \"{search_term}\""
                            }
                            match props.lyrics_candidates.clone() {
                                None => rsx! {
                                    div { class: "px-2 py-3 flex items-center gap-2 text-zinc-500 text-sm",
                                        Icon { name: "loader".to_string(), class: "w-4 h-4".to_string() }
                                        "Searching..."
                                    }
                                },
                                Some(Err(error)) => rsx! {
                                    p { class: "px-2 py-2 text-xs text-zinc-500 break-words", "{error}" }
                                },
                                Some(Ok(candidates)) => {
                                    if candidates.is_empty() {
                                        rsx! {
                                            p { class: "px-2 py-2 text-xs text-zinc-500", "No lyric matches found." }
                                        }
                                    } else {
                                        rsx! {
                                            div { class: "max-h-56 overflow-y-auto pr-1 space-y-1",
                                                for candidate in candidates {
                                                    button {
                                                        class: if props
                                                            .selected_query_override
                                                            .as_ref()
                                                            == Some(&candidate.query)
                                                        {
                                                            "w-full text-left p-2 rounded-lg border border-emerald-500/40 bg-emerald-500/10"
                                                        } else {
                                                            "w-full text-left p-2 rounded-lg border border-zinc-800/70 hover:border-zinc-600 hover:bg-zinc-900/70 transition-colors"
                                                        },
                                                        onclick: {
                                                            let query = candidate.query.clone();
                                                            move |_| on_pick_candidate(query.clone())
                                                        },
                                                        div { class: "flex items-center justify-between gap-3",
                                                            div { class: "min-w-0",
                                                                p { class: "text-sm text-white truncate", "{candidate.title}" }
                                                                p { class: "text-xs text-zinc-500 truncate",
                                                                    "{candidate.artist}"
                                                                }
                                                            }
                                                            div { class: "text-right flex-shrink-0",
                                                                p { class: "text-[10px] uppercase tracking-wider text-zinc-500",
                                                                    "{candidate.provider.label()}"
                                                                }
                                                                if let Some(duration) = candidate.duration_seconds {
                                                                    p { class: "text-[11px] text-zinc-500 font-mono",
                                                                        "{format_duration(duration)}"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        if !candidate.album.trim().is_empty() {
                                                            p { class: "text-[11px] text-zinc-600 truncate mt-1", "{candidate.album}" }
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

            div {
                id: "{scroll_container_id}",
                onscroll: on_lyrics_scrolled,
                class: "rounded-xl border border-zinc-800/80 bg-zinc-900/40 min-h-[52vh] md:min-h-[64vh] max-h-[76vh] overflow-y-auto",
                match display_lyrics {
                    None => {
                        if let Some(error) = fetch_error {
                            rsx! {
                                div { class: "p-6 space-y-2",
                                    p { class: "text-sm text-zinc-400", "No lyrics found for this song." }
                                    p { class: "text-xs text-zinc-500",
                                        "Try a manual search and pick the exact match."
                                    }
                                    p { class: "text-xs text-zinc-600 break-words", "{error}" }
                                }
                            }
                        } else {
                            rsx! {
                                div { class: "p-6 flex items-center justify-center text-zinc-500 gap-2",
                                    Icon { name: "loader".to_string(), class: "w-4 h-4".to_string() }
                                    "Loading lyrics..."
                                }
                            }
                        }
                    }
                    Some(lyrics) => {
                        if !props.sync_lyrics || lyrics.synced_lines.is_empty() {
                            let lines = lyrics
                                .plain_lyrics
                                .lines()
                                .map(str::trim)
                                .filter(|line| !line.is_empty())
                                .collect::<Vec<_>>();

                            rsx! {
                                div { class: "p-5 space-y-2",
                                    if fetch_error.is_some() {
                                        p { class: "text-xs text-amber-300/90",
                                            "Using last loaded lyrics because the latest fetch failed."
                                        }
                                    }
                                    div { class: "text-xs uppercase tracking-wider text-zinc-500 pb-1",
                                        "Source: {lyrics.provider.label()}"
                                    }
                                    if props.sync_lyrics && lyrics.synced_lines.is_empty() {
                                        p { class: "text-xs text-zinc-500",
                                            "Synced timestamps are not available from this result. Showing plain lyrics."
                                        }
                                    }
                                    if lines.is_empty() {
                                        p { class: "text-base text-zinc-500", "Lyrics unavailable." }
                                    } else {
                                        for line in lines {
                                            p { class: "text-base text-zinc-300 leading-relaxed", "{line}" }
                                        }
                                    }
                                }
                            }
                        } else {
                            rsx! {
                                div { class: "p-4 space-y-1",
                                    if fetch_error.is_some() {
                                        p { class: "text-xs text-amber-300/90 pb-1",
                                            "Using last loaded lyrics because the latest fetch failed."
                                        }
                                    }
                                    div { class: "text-xs uppercase tracking-wider text-zinc-500 pb-1",
                                        "Source: {lyrics.provider.label()}"
                                    }
                                    for (index, line) in lyrics.synced_lines.iter().enumerate() {
                                        button {
                                            id: format!("{scroll_container_id}-line-{index}"),
                                            class: if Some(index) == active_synced_index {
                                                "w-full text-left px-3 py-2.5 rounded-lg bg-emerald-500/15 text-emerald-300"
                                            } else {
                                                "w-full text-left px-3 py-2 rounded-lg text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/60 transition-colors"
                                            },
                                            onclick: {
                                                let line = line.clone();
                                                move |_| on_seek_line(line.clone())
                                            },
                                            span { class: "text-xs text-zinc-500 mr-2 font-mono",
                                                "{format_timestamp(line.timestamp_seconds)}"
                                            }
                                            span {
                                                class: if Some(index) == active_synced_index {
                                                    "text-lg md:text-xl font-semibold leading-relaxed"
                                                } else {
                                                    "text-base leading-relaxed"
                                                },
                                                "{line.text}"
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

fn song_cover_url(song: &Song, servers: &[ServerConfig], size: u32) -> Option<String> {
    let server = servers.iter().find(|server| server.id == song.server_id)?;
    let cover_art = song.cover_art.as_ref()?;
    let client = NavidromeClient::new(server.clone());
    Some(client.get_cover_art_url(cover_art, size))
}

fn active_lyric_index(lines: &[LyricLine], playback_seconds: f64) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }

    let mut active = None;
    for (index, line) in lines.iter().enumerate() {
        if playback_seconds >= line.timestamp_seconds {
            active = Some(index);
        } else {
            break;
        }
    }
    active
}

fn format_timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u32;
    let mins = total / 60;
    let secs = total % 60;
    format!("{mins:02}:{secs:02}")
}

fn sanitize_dom_id(raw: &str) -> String {
    let sanitized = raw
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "lyrics".to_string()
    } else {
        sanitized
    }
}

fn now_millis() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as f64)
            .unwrap_or(0.0)
    }
}

fn adjusted_queue_index_after_reorder(
    current_index: usize,
    source_index: usize,
    target_index: usize,
) -> usize {
    if source_index == current_index {
        target_index
    } else if source_index < current_index && target_index >= current_index {
        current_index.saturating_sub(1)
    } else if source_index > current_index && target_index <= current_index {
        current_index.saturating_add(1)
    } else {
        current_index
    }
}

fn reorder_queue_entry(
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    source_index: usize,
    target_index: usize,
) {
    let current_index = queue_index();
    let mut reordered = false;
    let mut next_index = current_index;

    queue.with_mut(|items| {
        if items.len() < 2
            || source_index >= items.len()
            || target_index >= items.len()
            || source_index == target_index
        {
            return;
        }

        let moved_song = items.remove(source_index);
        let insert_index = if source_index < target_index {
            target_index.saturating_sub(1)
        } else {
            target_index
        };
        items.insert(insert_index, moved_song);
        next_index = adjusted_queue_index_after_reorder(current_index, source_index, insert_index);
        reordered = true;
    });

    if !reordered {
        return;
    }

    let updated_queue = queue();
    if updated_queue.is_empty() {
        queue_index.set(0);
        now_playing.set(None);
        return;
    }

    let clamped_index = next_index.min(updated_queue.len().saturating_sub(1));
    queue_index.set(clamped_index);
    if now_playing().is_some() {
        now_playing.set(updated_queue.get(clamped_index).cloned());
    }
}

fn remove_queue_entry(
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    mut is_playing: Signal<bool>,
    remove_index: usize,
) {
    let had_now_playing = now_playing().is_some();
    let was_playing = is_playing();
    let current_index = queue_index();
    let mut removed = false;

    queue.with_mut(|items| {
        if remove_index >= items.len() {
            return;
        }
        items.remove(remove_index);
        removed = true;
    });

    if !removed {
        return;
    }

    let updated_queue = queue();
    if updated_queue.is_empty() {
        queue_index.set(0);
        now_playing.set(None);
        is_playing.set(false);
        return;
    }

    let mut next_index = current_index.min(updated_queue.len().saturating_sub(1));
    if remove_index < current_index {
        next_index = current_index.saturating_sub(1);
    } else if remove_index == current_index {
        next_index = remove_index.min(updated_queue.len().saturating_sub(1));
    }

    queue_index.set(next_index);
    if had_now_playing {
        now_playing.set(updated_queue.get(next_index).cloned());
        is_playing.set(was_playing);
    }
}

async fn fetch_related_candidates(client: &NavidromeClient, song: &Song, count: u32) -> Vec<Song> {
    let mut related = client
        .get_similar_songs2(&song.id, count)
        .await
        .unwrap_or_default();

    if let Some(artist_id) = song.artist_id.as_deref() {
        let by_artist = client
            .get_similar_songs(artist_id, count)
            .await
            .unwrap_or_default();
        related.extend(by_artist);
    }

    if related.len() < count as usize {
        if let Some(artist_name) = song.artist.clone().filter(|name| !name.trim().is_empty()) {
            let top_songs = client
                .get_top_songs(&artist_name, count)
                .await
                .unwrap_or_default();
            related.extend(top_songs);
        }
    }

    if related.len() < count as usize {
        let fallback_query = format!(
            "{} {}",
            song.artist.clone().unwrap_or_default(),
            song.title.clone()
        );
        if let Ok(search) = client.search(&fallback_query, 0, 0, count).await {
            related.extend(search.songs);
        }
    }

    related
}

fn toggle_song_favorite(
    song: Song,
    should_star: bool,
    servers: Signal<Vec<ServerConfig>>,
    mut now_playing: Signal<Option<Song>>,
    mut queue: Signal<Vec<Song>>,
) {
    let Some(server) = servers()
        .iter()
        .find(|entry| entry.id == song.server_id)
        .cloned()
    else {
        return;
    };

    let song_id = song.id.clone();
    let song_server_id = song.server_id.clone();

    spawn(async move {
        let client = NavidromeClient::new(server);
        let result = if should_star {
            client.star(&song_id, "song").await
        } else {
            client.unstar(&song_id, "song").await
        };

        if result.is_ok() {
            let new_starred = if should_star {
                Some("local".to_string())
            } else {
                None
            };
            now_playing.with_mut(|current| {
                if let Some(current_song) = current.as_mut() {
                    if current_song.id == song_id && current_song.server_id == song_server_id {
                        current_song.starred = new_starred.clone();
                    }
                }
            });
            queue.with_mut(|items| {
                for entry in items.iter_mut() {
                    if entry.id == song_id && entry.server_id == song_server_id {
                        entry.starred = new_starred.clone();
                    }
                }
            });
        }
    });
}

fn set_now_playing_rating(
    servers: Signal<Vec<ServerConfig>>,
    mut now_playing: Signal<Option<Song>>,
    mut queue: Signal<Vec<Song>>,
    rating: u32,
) {
    let Some(song) = now_playing() else {
        return;
    };
    let Some(server) = servers()
        .iter()
        .find(|entry| entry.id == song.server_id)
        .cloned()
    else {
        return;
    };

    let song_id = song.id.clone();
    let song_server_id = song.server_id.clone();
    let normalized_rating = rating.min(5);

    spawn(async move {
        let client = NavidromeClient::new(server);
        if client.set_rating(&song_id, normalized_rating).await.is_ok() {
            let new_rating = if normalized_rating == 0 {
                None
            } else {
                Some(normalized_rating)
            };
            now_playing.with_mut(|current| {
                if let Some(current_song) = current.as_mut() {
                    if current_song.id == song_id && current_song.server_id == song_server_id {
                        current_song.user_rating = new_rating;
                    }
                }
            });
            queue.with_mut(|items| {
                for entry in items.iter_mut() {
                    if entry.id == song_id && entry.server_id == song_server_id {
                        entry.user_rating = new_rating;
                    }
                }
            });
        }
    });
}

async fn load_related_songs(maybe_song: Option<Song>, servers: Vec<ServerConfig>) -> Vec<Song> {
    let Some(song) = maybe_song else {
        return Vec::new();
    };

    let Some(server) = servers
        .into_iter()
        .find(|server| server.id == song.server_id)
    else {
        return Vec::new();
    };

    let client = NavidromeClient::new(server);
    let related = fetch_related_candidates(&client, &song, 30).await;

    let mut unique = Vec::<Song>::new();
    for candidate in related {
        if candidate.id == song.id && candidate.server_id == song.server_id {
            continue;
        }
        if unique.iter().any(|existing| {
            existing.id == candidate.id && existing.server_id == candidate.server_id
        }) {
            continue;
        }
        unique.push(candidate);
        if unique.len() >= 30 {
            break;
        }
    }

    unique
}

async fn build_queue_from_seed(seed_song: Song, servers: Vec<ServerConfig>) -> Vec<Song> {
    let Some(server) = servers
        .into_iter()
        .find(|server| server.id == seed_song.server_id)
    else {
        return vec![seed_song];
    };

    let client = NavidromeClient::new(server);
    let mut queue = vec![seed_song.clone()];

    let related = fetch_related_candidates(&client, &seed_song, 60).await;
    for candidate in related {
        if queue.iter().any(|existing| {
            existing.id == candidate.id && existing.server_id == candidate.server_id
        }) {
            continue;
        }
        queue.push(candidate);
        if queue.len() >= 60 {
            break;
        }
    }

    if queue.len() <= 1 {
        let random_songs = client.get_random_songs(40).await.unwrap_or_default();
        for candidate in random_songs {
            if queue.iter().any(|existing| {
                existing.id == candidate.id && existing.server_id == candidate.server_id
            }) {
                continue;
            }
            queue.push(candidate);
            if queue.len() >= 40 {
                break;
            }
        }
    }

    queue
}
