use crate::api::*;
use crate::components::audio_manager::{
    apply_collection_shuffle_mode, queue_should_generate_similar_on_end, spawn_shuffle_queue,
};
use crate::components::{
    ios_diag_log, seek_to, AddIntent, AddMenuController, AudioState, Icon, PlaybackPositionSignal,
};

/// Visual size of a transport control.
#[derive(Clone, Copy, PartialEq, Default)]
pub(super) enum ControlSize {
    /// 34px buttons in the wide player bar.
    #[default]
    Bar,
    /// 44px touch targets in the compact mini player.
    Mini,
}
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
            class: "rs-iconbtn rs-iconbtn--quiet",
            aria_label: "Save bookmark",
            title: "Save bookmark",
            aria_pressed: if saved() { "true" } else { "false" },
            disabled: !has_song || saving() || is_live_radio,
            onclick: on_save,
            if saving() {
                span { class: "rs-spinner", aria_hidden: "true" }
            } else {
                Icon { name: "bookmark".to_string(), class: "rs-ic rs-ic--sm".to_string() }
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
        div { class: "rs-rate",
            button {
                id: "rating-btn",
                r#type: "button",
                class: "rs-iconbtn rs-iconbtn--quiet",
                class: if current_rating > 0 { "rs-iconbtn--rated" },
                aria_label: if current_rating > 0 { format!("Rating: {current_rating} of 5") } else { "Rate".to_string() },
                title: "Rate",
                aria_haspopup: "true",
                aria_expanded: if rating_open() { "true" } else { "false" },
                disabled: !has_song,
                onclick: move |_| rating_open.set(!rating_open()),
                Icon { name: "rating".to_string(), class: "rs-ic".to_string() }
                if current_rating > 0 {
                    span { class: "rs-rate__value num", "{current_rating}" }
                }
            }
            if rating_open() && has_song {
                div { class: "rs-rate__pop", role: "group", aria_label: "Rating",
                    for value in 1..=5 {
                        button {
                            r#type: "button",
                            class: "rs-iconbtn",
                            class: if value <= current_rating { "rs-iconbtn--rated" },
                            aria_label: "{value} star",
                            aria_pressed: if value == current_rating { "true" } else { "false" },
                            onclick: {
                                let on_rate = on_rate.clone();
                                move |_| on_rate(value)
                            },
                            Icon {
                                name: if value <= current_rating { "star-filled".to_string() } else { "star".to_string() },
                                class: "rs-ic rs-ic--sm".to_string(),
                            }
                        }
                    }
                    button {
                        r#type: "button",
                        class: "rs-rate__clear",
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
pub(super) fn PlayPauseButton(#[props(default)] size: ControlSize) -> Element {
    let mut is_playing = use_context::<crate::components::IsPlayingSignal>().0;
    let playing = is_playing();

    rsx! {
        button {
            id: "play-pause-btn",
            r#type: "button",
            class: "rs-iconbtn rs-iconbtn--play",
            aria_label: if playing { "Pause" } else { "Play" },
            title: if playing { "Pause" } else { "Play" },
            onclick: move |_| {
                let current = is_playing();
                eprintln!(
                    "[ui.play_pause] click current={} next={}",
                    current,
                    !current
                );
                ios_diag_log(
                    "ui.control",
                    &format!("source=player.play-pause current={current} next={}", !current),
                );
                is_playing.set(!current);
            },
            Icon {
                name: if playing { "pause".to_string() } else { "play".to_string() },
                class: match size {
                    ControlSize::Bar => "rs-ic rs-ic--lg".to_string(),
                    ControlSize::Mini => "rs-ic rs-ic--md".to_string(),
                },
            }
        }
    }
}

/// Previous button - completely isolated component
#[component]
pub(super) fn PrevButton(
    /// Keep `prev-btn` in the DOM for MediaSession without showing it.
    #[props(default)]
    hidden: bool,
) -> Element {
    let mut queue_index = use_context::<Signal<usize>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<crate::components::IsPlayingSignal>().0;
    let current_song = now_playing();
    let is_radio = current_song
        .as_ref()
        .map(|song| song.server_name == "Radio")
        .unwrap_or(false);

    rsx! {
        button {
            id: "prev-btn",
            r#type: "button",
            disabled: is_radio,
            class: if hidden { "rs-sr-only" } else { "rs-iconbtn" },
            tabindex: if hidden { Some("-1") } else { None },
            aria_hidden: if hidden { Some("true") } else { None },
            aria_label: "Previous",
            title: "Previous",
            onclick: move |_| {
                if is_radio {
                    return;
                }
                let idx = queue_index();
                let queue_list = queue();
                if idx > 0 && !queue_list.is_empty() {
                    let was_playing = *is_playing.peek();
                    let next_idx = idx - 1;
                    if let Some(song) = queue_list.get(next_idx).cloned() {
                        ios_diag_log(
                            "ui.control",
                            &format!(
                                "source=player.prev from_idx={idx} to_idx={next_idx} queue_len={} was_playing={} song_id={}",
                                queue_list.len(),
                                was_playing,
                                song.id
                            ),
                        );
                        queue_index.set(next_idx);
                        now_playing.set(Some(song));
                        if was_playing {
                            is_playing.set(true);
                        }
                    }
                }
            },
            Icon { name: "prev".to_string(), class: "rs-ic rs-ic--md".to_string() }
        }
    }
}

/// Next button - completely isolated component
#[component]
pub(super) fn NextButton(#[props(default)] size: ControlSize) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut is_playing = use_context::<crate::components::IsPlayingSignal>().0;
    let mut queue_index = use_context::<Signal<usize>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let audio_state = use_context::<Signal<AudioState>>();
    let repeat_mode = use_context::<Signal<RepeatMode>>();
    let shuffle_enabled = use_context::<crate::components::ShuffleEnabledSignal>().0;
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
            class: "rs-iconbtn",
            aria_label: "Next",
            title: "Next",
            onclick: move |_| {
                if is_radio {
                    return;
                }
                let was_playing = *is_playing.peek();
                let repeat = *repeat_mode.peek();
                if repeat == RepeatMode::One {
                    ios_diag_log(
                        "ui.control",
                        &format!("source=player.next repeat=one was_playing={was_playing} action=restart"),
                    );
                    seek_to(0.0);
                    if was_playing {
                        is_playing.set(true);
                    }
                    return;
                }
                let idx = *queue_index.peek();
                let queue_list = queue.peek().clone();
                let shuffle = *shuffle_enabled.peek();
                if idx < queue_list.len().saturating_sub(1) {
                    if let Some(song) = queue_list.get(idx + 1).cloned() {
                        ios_diag_log(
                            "ui.control",
                            &format!(
                                "source=player.next from_idx={idx} to_idx={} queue_len={} was_playing={} song_id={}",
                                idx + 1,
                                queue_list.len(),
                                was_playing,
                                song.id
                            ),
                        );
                        queue_index.set(idx + 1);
                        now_playing.set(Some(song));
                        if was_playing {
                            is_playing.set(true);
                        }
                    }
                } else if repeat == RepeatMode::All && !queue_list.is_empty() {
                    if let Some(song) = queue_list.get(0).cloned() {
                        ios_diag_log(
                            "ui.control",
                            &format!(
                                "source=player.next repeat=all from_idx={idx} to_idx=0 queue_len={} was_playing={} song_id={}",
                                queue_list.len(),
                                was_playing,
                                song.id
                            ),
                        );
                        queue_index.set(0);
                        now_playing.set(Some(song));
                        if was_playing {
                            is_playing.set(true);
                        }
                    }
                } else if repeat == RepeatMode::Off
                    && queue_should_generate_similar_on_end(
                        &queue_list,
                        now_playing.peek().as_ref(),
                        shuffle,
                    )
                {
                    ios_diag_log(
                        "ui.control",
                        &format!(
                            "source=player.next similar_queue_from_single shuffle={shuffle} queue_len={} was_playing={was_playing}",
                            queue_list.len()
                        ),
                    );
                    spawn_shuffle_queue(
                        servers.peek().clone(),
                        queue.clone(),
                        queue_index.clone(),
                        now_playing.clone(),
                        is_playing.clone(),
                        audio_state.clone(),
                        now_playing.peek().clone(),
                        Some(was_playing),
                    );
                } else {
                    ios_diag_log(
                        "ui.control",
                        &format!(
                            "source=player.next end_of_queue repeat={repeat:?} shuffle={shuffle} queue_len={} action=stop",
                            queue_list.len()
                        ),
                    );
                    is_playing.set(false);
                }
            },
            Icon {
                name: "next".to_string(),
                class: match size {
                    ControlSize::Bar => "rs-ic rs-ic--md".to_string(),
                    ControlSize::Mini => "rs-ic rs-ic--lg".to_string(),
                },
            }
        }
    }
}

/// Repeat button - completely isolated component
#[component]
pub(super) fn RepeatButton() -> Element {
    let mut repeat_mode = use_context::<Signal<RepeatMode>>();
    let mut app_settings = use_context::<Signal<AppSettings>>();
    let mode = repeat_mode();

    rsx! {
        button {
            id: "repeat-btn",
            r#type: "button",
            class: "rs-iconbtn rs-iconbtn--quiet",
            aria_label: match mode {
                RepeatMode::Off => "Repeat: off",
                RepeatMode::All => "Repeat: all",
                RepeatMode::One => "Repeat: one",
            },
            title: "Repeat",
            aria_pressed: if mode == RepeatMode::Off { "false" } else { "true" },
            onclick: move |_| {
                let next = match repeat_mode() {
                    RepeatMode::Off => RepeatMode::All,
                    RepeatMode::All => RepeatMode::One,
                    RepeatMode::One => RepeatMode::Off,
                };
                repeat_mode.set(next);
                app_settings.with_mut(|settings| {
                    settings.repeat_mode = next;
                });
                let settings_snapshot = app_settings();
                spawn(async move {
                    let _ = crate::db::save_settings(settings_snapshot).await;
                });
            },
            Icon {
                name: match mode {
                    RepeatMode::One => "repeat-1".to_string(),
                    _ => "repeat".to_string(),
                },
                class: "rs-ic rs-ic--sm".to_string(),
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
            class: "rs-iconbtn rs-iconbtn--quiet",
            aria_label: "Add to queue or playlist",
            title: "Add to queue or playlist",
            onclick: on_open_add_menu,
            Icon {
                name: "playlist".to_string(),
                class: "rs-ic rs-ic--sm".to_string(),
            }
        }
    }
}

/// Shuffle button - toggle shuffle mode
#[component]
pub(super) fn ShuffleButton() -> Element {
    let mut shuffle_enabled = use_context::<crate::components::ShuffleEnabledSignal>().0;
    let mut app_settings = use_context::<Signal<AppSettings>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let enabled = shuffle_enabled();

    rsx! {
        button {
            id: "shuffle-btn",
            r#type: "button",
            class: "rs-iconbtn rs-iconbtn--quiet",
            aria_label: "Shuffle",
            title: "Shuffle",
            aria_pressed: if enabled { "true" } else { "false" },
            onclick: move |_| {
                let next = !shuffle_enabled();
                eprintln!(
                    "[ui.shuffle] click current={} next={} queue_len={} queue_index={}",
                    !next,
                    next,
                    queue().len(),
                    queue_index()
                );
                shuffle_enabled.set(next);
                let changed = apply_collection_shuffle_mode(
                    queue.clone(),
                    queue_index.clone(),
                    now_playing.clone(),
                    next,
                );
                eprintln!("[ui.shuffle] applied changed={changed}");
                app_settings.with_mut(|settings| {
                    settings.shuffle_enabled = next;
                });
                let settings_snapshot = app_settings();
                spawn(async move {
                    let _ = crate::db::save_settings(settings_snapshot).await;
                });
            },
            Icon { name: "shuffle".to_string(), class: "rs-ic rs-ic--sm".to_string() }
        }
    }
}
