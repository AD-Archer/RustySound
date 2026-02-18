use crate::api::*;
use crate::components::audio_manager::spawn_shuffle_queue;
use crate::components::{
    seek_to, AddIntent, AddMenuController, Icon, PlaybackPositionSignal,
};
use crate::db::{AppSettings, RepeatMode};
use dioxus::prelude::*;

/// Bookmark button - capture current playback position on the server
#[component]
pub(super) fn BookmarkButton() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let saving = use_signal(|| false);
    let saved = use_signal(|| false);
    let base_class = "flex items-center justify-center";

    {
        let now_playing = now_playing.clone();
        let mut saved_signal = saved.clone();
        use_effect(move || {
            let _ = now_playing();
            saved_signal.set(false);
        });
    }

    let current_song = now_playing();
    let has_song = current_song.is_some();
    let is_live_radio = current_song
        .as_ref()
        .map(|song| song.server_name == "Radio")
        .unwrap_or(false);

    let on_save = move |_| {
        if saving() {
            return;
        }
        if let Some(song) = now_playing.peek().clone() {
            if let Some(server) = servers().iter().find(|s| s.id == song.server_id).cloned() {
                let song_id = song.id.clone();
                let position_ms = (playback_position() * 1000.0).round().max(0.0) as u64;
                let bookmark_limit = app_settings().bookmark_limit.clamp(1, 5000) as usize;
                let mut saving = saving.clone();
                let mut saved = saved.clone();
                spawn(async move {
                    saving.set(true);
                    let client = NavidromeClient::new(server);
                    let res = client
                        .create_bookmark_with_limit(
                            &song_id,
                            position_ms,
                            None,
                            Some(bookmark_limit),
                        )
                        .await;
                    saving.set(false);
                    saved.set(res.is_ok());
                });
            }
        }
    };

    rsx! {
        button {
            id: "bookmark-btn",
            r#type: "button",
            disabled: !has_song || saving() || is_live_radio,
            class: if saved() { format!(
                "{base_class} p-1.5 sm:p-2 text-emerald-400 hover:text-emerald-300 transition-colors",
            ) } else { format!("{base_class} p-1.5 sm:p-2 text-zinc-400 hover:text-white transition-colors") },
            onclick: on_save,
            if saving() {
                Icon { name: "loader".to_string(), class: "w-5 h-5".to_string() }
            } else {
                Icon {
                    name: "bookmark".to_string(),
                    class: "w-4 h-4 sm:w-5 sm:h-5".to_string(),
                }
            }
        }
    }
}

/// Rating button - set a star rating for the current song
#[component]
pub(super) fn RatingButton() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let mut rating_open = use_signal(|| false);

    let current = now_playing();
    let current_rating = current
        .as_ref()
        .and_then(|s| s.user_rating)
        .unwrap_or(0)
        .min(5);
    let has_song = current.is_some();

    let on_rate = {
        let servers = servers.clone();
        let mut now_playing = now_playing.clone();
        let mut queue = queue.clone();
        let mut rating_open = rating_open.clone();
        move |rating: u32| {
            if let Some(song) = now_playing() {
                let server = servers().iter().find(|s| s.id == song.server_id).cloned();
                if let Some(server) = server {
                    let song_id = song.id.clone();
                    let normalized = if rating > 5 { 5 } else { rating };
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        if client.set_rating(&song_id, normalized).await.is_ok() {
                            let updated = if normalized == 0 {
                                None
                            } else {
                                Some(normalized)
                            };
                            now_playing.with_mut(|current| {
                                if let Some(ref mut s) = current {
                                    if s.id == song_id {
                                        s.user_rating = updated;
                                    }
                                }
                            });
                            queue.with_mut(|items| {
                                for song in items.iter_mut() {
                                    if song.id == song_id {
                                        song.user_rating = updated;
                                    }
                                }
                            });
                        }
                        rating_open.set(false);
                    });
                }
            }
        }
    };

    rsx! {
        div { class: "relative",
            button {
                id: "rating-btn",
                r#type: "button",
                disabled: !has_song,
                class: if current_rating > 0 { "p-1.5 sm:p-2 text-amber-400 hover:text-amber-300 transition-colors" } else { "p-1.5 sm:p-2 text-zinc-400 hover:text-white transition-colors" },
                onclick: move |_| rating_open.set(!rating_open()),
                Icon {
                    name: if current_rating > 0 { "star-filled".to_string() } else { "star".to_string() },
                    class: "w-4 h-4 sm:w-5 sm:h-5".to_string(),
                }
            }
            if rating_open() && has_song {
                div { class: "absolute bottom-12 left-1/2 -translate-x-1/2 bg-zinc-900/95 border border-zinc-800 rounded-xl px-3 py-2 shadow-xl flex items-center gap-2",
                    for value in 1..=5 {
                        button {
                            r#type: "button",
                            class: if value <= current_rating { "text-amber-400 hover:text-amber-300 transition-colors" } else { "text-zinc-500 hover:text-zinc-300 transition-colors" },
                            onclick: {
                                let on_rate = on_rate.clone();
                                move |_| on_rate(value)
                            },
                            Icon {
                                name: if value <= current_rating { "star-filled".to_string() } else { "star".to_string() },
                                class: "w-4 h-4".to_string(),
                            }
                        }
                    }
                    button {
                        r#type: "button",
                        class: "ml-1 text-xs text-zinc-400 hover:text-white transition-colors",
                        onclick: {
                            let on_rate = on_rate.clone();
                            move |_| on_rate(0)
                        },
                        "Clear"
                    }
                }
            }
        }
    }
}

/// Play/Pause button - completely isolated component
#[component]
pub(super) fn PlayPauseButton() -> Element {
    let mut is_playing = use_context::<Signal<bool>>();
    let playing = is_playing();

    rsx! {
        button {
            id: "play-pause-btn",
            r#type: "button",
            class: "w-10 h-10 rounded-full bg-white flex items-center justify-center hover:scale-105 transition-transform shadow-lg",
            onclick: move |_| {
                let current = is_playing();
                is_playing.set(!current);
            },
            if playing {
                Icon {
                    name: "pause".to_string(),
                    class: "w-5 h-5 text-black".to_string(),
                }
            } else {
                Icon {
                    name: "play".to_string(),
                    class: "w-5 h-5 text-black ml-0.5".to_string(),
                }
            }
        }
    }
}

/// Previous button - completely isolated component
#[component]
pub(super) fn PrevButton() -> Element {
    let mut queue_index = use_context::<Signal<usize>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();
    let current_song = now_playing();
    let is_radio = current_song
        .as_ref()
        .map(|song| song.server_name == "Radio")
        .unwrap_or(false);
    let was_playing = is_playing();

    rsx! {
        button {
            id: "prev-btn",
            r#type: "button",
            disabled: is_radio,
            class: if is_radio { "p-1.5 sm:p-2 text-zinc-600 cursor-not-allowed" } else { "p-1.5 sm:p-2 text-zinc-300 hover:text-white transition-colors" },
            onclick: move |_| {
                if is_radio {
                    return;
                }
                let idx = queue_index();
                let queue_list = queue();
                if idx > 0 && !queue_list.is_empty() {
                    let next_idx = idx - 1;
                    if let Some(song) = queue_list.get(next_idx).cloned() {
                        queue_index.set(next_idx);
                        now_playing.set(Some(song));
                        if was_playing {
                            is_playing.set(true);
                        }
                    }
                }
            },
            Icon { name: "prev".to_string(), class: "w-4 h-4 sm:w-5 sm:h-5".to_string() }
        }
    }
}

/// Next button - completely isolated component
#[component]
pub(super) fn NextButton() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut is_playing = use_context::<Signal<bool>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let repeat_mode = use_context::<Signal<RepeatMode>>();
    let shuffle_enabled = use_context::<Signal<bool>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let current_song = now_playing();
    let is_radio = current_song
        .as_ref()
        .map(|song| song.server_name == "Radio")
        .unwrap_or(false);

    rsx! {
        button {
            id: "next-btn",
            r#type: "button",
            disabled: is_radio,
            class: if is_radio { "p-1.5 sm:p-2 text-zinc-600 cursor-not-allowed" } else { "p-1.5 sm:p-2 text-zinc-300 hover:text-white transition-colors" },
            onclick: move |_| {
                if is_radio {
                    return;
                }
                let was_playing = *is_playing.peek();
                let repeat = *repeat_mode.peek();
                if repeat == RepeatMode::One {
                    seek_to(0.0);
                    if was_playing {
                        is_playing.set(true);
                    }
                    return;
                }
                let idx = *queue_index.peek();
                let queue_list = queue.peek();
                let can_shuffle = repeat == RepeatMode::Off;
                if can_shuffle && *shuffle_enabled.peek() {
                    if queue_list.is_empty() {
                        spawn_shuffle_queue(
                            servers.peek().clone(),
                            queue.clone(),
                            queue_index.clone(),
                            now_playing.clone(),
                            is_playing.clone(),
                            now_playing.peek().clone(),
                            Some(was_playing),
                        );
                    } else if idx < queue_list.len().saturating_sub(1) {
                        if let Some(song) = queue_list.get(idx + 1).cloned() {
                            queue_index.set(idx + 1);
                            now_playing.set(Some(song));
                            if was_playing {
                                is_playing.set(true);
                            }
                        }
                    } else {
                        spawn_shuffle_queue(
                            servers.peek().clone(),
                            queue.clone(),
                            queue_index.clone(),
                            now_playing.clone(),
                            is_playing.clone(),
                            now_playing.peek().clone(),
                            Some(was_playing),
                        );
                    }
                } else if idx < queue_list.len().saturating_sub(1) {
                    if let Some(song) = queue_list.get(idx + 1).cloned() {
                        queue_index.set(idx + 1);
                        now_playing.set(Some(song));
                        if was_playing {
                            is_playing.set(true);
                        }
                    }
                } else if repeat == RepeatMode::All && !queue_list.is_empty() {
                    if let Some(song) = queue_list.get(0).cloned() {
                        queue_index.set(0);
                        now_playing.set(Some(song));
                        if was_playing {
                            is_playing.set(true);
                        }
                    }
                }
            },
            Icon { name: "next".to_string(), class: "w-4 h-4 sm:w-5 sm:h-5".to_string() }
        }
    }
}

/// Repeat button - completely isolated component
#[component]
pub(super) fn RepeatButton() -> Element {
    let mut repeat_mode = use_context::<Signal<RepeatMode>>();
    let mode = repeat_mode();

    rsx! {
        button {
            id: "repeat-btn",
            r#type: "button",
            class: match mode {
                RepeatMode::Off => "p-1.5 sm:p-2 text-zinc-400 hover:text-white transition-colors",
                RepeatMode::All | RepeatMode::One => {
                    "p-1.5 sm:p-2 text-emerald-400 hover:text-emerald-300 transition-colors"
                }
            },
            onclick: move |_| {
                let next = match repeat_mode() {
                    RepeatMode::Off => RepeatMode::All,
                    RepeatMode::All => RepeatMode::One,
                    RepeatMode::One => RepeatMode::Off,
                };
                repeat_mode.set(next);
            },
            Icon {
                name: match mode {
                    RepeatMode::One => "repeat-1".to_string(),
                    _ => "repeat".to_string(),
                },
                class: "w-4 h-4 sm:w-5 sm:h-5".to_string(),
            }
        }
    }
}

/// Add current song to queue/playlist menu
#[component]
pub(super) fn AddToMenuButton() -> Element {
    let now_playing = use_context::<Signal<Option<Song>>>();
    let add_menu = use_context::<AddMenuController>();

    let current_song = now_playing();
    let is_live_radio = current_song
        .as_ref()
        .map(|song| song.server_name == "Radio")
        .unwrap_or(false);
    let has_song = current_song.is_some() && !is_live_radio;

    let on_open_add_menu = {
        let mut add_menu = add_menu.clone();
        move |_| {
            if let Some(song) = now_playing() {
                if song.server_name == "Radio" {
                    return;
                }
                add_menu.open(AddIntent::from_song(song));
            }
        }
    };

    rsx! {
        button {
            id: "add-menu-btn",
            r#type: "button",
            disabled: !has_song,
            class: if has_song {
                "p-1.5 sm:p-2 text-zinc-300 hover:text-white transition-colors"
            } else {
                "p-1.5 sm:p-2 text-zinc-600 cursor-not-allowed"
            },
            onclick: on_open_add_menu,
            Icon {
                name: "playlist".to_string(),
                class: "w-4 h-4 sm:w-5 sm:h-5".to_string(),
            }
        }
    }
}

/// Shuffle button - toggle shuffle mode
#[component]
pub(super) fn ShuffleButton() -> Element {
    let mut shuffle_enabled = use_context::<Signal<bool>>();
    let enabled = shuffle_enabled();

    rsx! {
        button {
            id: "shuffle-btn",
            r#type: "button",
            class: if enabled { "p-3 md:p-2 text-emerald-400 hover:text-emerald-300 transition-colors" } else { "p-3 md:p-2 text-zinc-400 hover:text-white transition-colors" },
            onclick: move |_| {
                let current = shuffle_enabled();
                shuffle_enabled.set(!current);
            },
            Icon { name: "shuffle".to_string(), class: "w-5 h-5".to_string() }
        }
    }
}
