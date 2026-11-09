//! Remote sync in the player: the "Playing on Mac" mini player that takes
//! the local one's place while another device is active, and the "Play on"
//! choices under the Now Playing menu's Output row.

use super::{room_server, RemoteSync};
use crate::api::{NavidromeClient, ServerConfig, Song};
use crate::components::ui::env::{format_clock, sleep_ms};
use crate::components::ui::Artwork;
use crate::components::{Icon, PlayerCtx, SongDetailsController};
use dioxus::prelude::*;

/// How often the remote position redraws between `state` messages (a local
/// timer; nothing goes over the network).
const PROGRESS_TICK_MS: u64 = 500;

/// The song the active device plays: from this device's queue when it is
/// there, else one cached `getSong`.
pub(crate) fn use_remote_song(sync: RemoteSync) -> Resource<Option<Song>> {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let track = use_memo(move || {
        sync.view()
            .and_then(|view| view.state().and_then(|s| s.state.track_id.clone()))
    });
    use_resource(move || async move {
        let id = track()?;
        let server = room_server(&servers.peek())?.clone();
        let local = {
            let player = PlayerCtx::from_context();
            let queue = player.queue.peek();
            queue.iter().find(|s| s.id == id && s.server_id == server.id).cloned()
        };
        match local {
            Some(song) => Some(song),
            None => NavidromeClient::new(server).get_song(&id).await.ok(),
        }
    })
}

/// The mini player (compact) or player bar (wide) for another device's
/// playback: its song, "Playing on Mac", live progress, Play/Pause, Next and
/// Play here. Renders nothing unless a remote device is shown.
#[component]
pub fn RemotePlayer(compact: bool) -> Element {
    let sync = use_context::<RemoteSync>();
    let song_details = use_context::<SongDetailsController>();
    let song = use_remote_song(sync);
    let Some(view) = sync.view() else {
        return rsx! {};
    };
    let target = view.target().map(str::to_string);
    let playing = view.state().is_some_and(|s| s.state.playing);
    let song = song.read().clone().flatten();
    let caption = view.caption();
    let title = song.as_ref().map(|s| s.title.clone()).unwrap_or_else(|| "Not Playing".to_string());
    let artist = song.as_ref().and_then(|s| s.artist.clone()).unwrap_or_default();
    let duration_ms = song.as_ref().map(|s| s.duration as u64 * 1000).filter(|d| *d > 0);
    let (cover_art, server_id) = song
        .as_ref()
        .map(|s| (s.cover_art.clone(), s.server_id.clone()))
        .unwrap_or_default();

    let toggle = {
        let target = target.clone();
        move |_| {
            if let Some(target) = target.as_deref() {
                sync.toggle_remote(target);
            }
        }
    };
    let next = {
        let target = target.clone();
        move |_| {
            if let Some(target) = target.as_deref() {
                sync.send(target, super::protocol::Op::Next);
            }
        }
    };
    // Tapping the song opens Now Playing on it: lyrics, info and these
    // controls, following the other device.
    let open = {
        let song = song.clone();
        let mut song_details = song_details;
        move |_| {
            if let Some(song) = song.clone() {
                song_details.open(song);
            }
        }
    };
    let play_label = if playing { "Pause" } else { "Play" };
    let play_icon = if playing { "pause" } else { "play" };
    let region = format!("{caption}: {title}");

    if compact {
        return rsx! {
            div { class: "rs-mini rs-mini--remote", role: "region", aria_label: "{region}", "data-testid": "remote-player",
                button {
                    r#type: "button",
                    class: "rs-mini__open",
                    disabled: song.is_none(),
                    "data-testid": "remote-open",
                    onclick: open.clone(),
                    Artwork { cover_art: cover_art.clone(), server_id: server_id.clone(), size_px: 40, eager: true }
                    span { class: "rs-sr-only", "Open Now Playing: " }
                    span { class: "rs-mini__meta",
                        span { class: "rs-mini__title", style: "display:block", "{title}" }
                        span { class: "rs-mini__sub rs-remote__caption", "data-testid": "remote-caption",
                            Icon { name: "headphones".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                            span { class: "rs-remote__caption-text", "{caption}" }
                        }
                    }
                }
                if target.is_some() {
                    button {
                        r#type: "button",
                        class: "rs-iconbtn rs-iconbtn--play",
                        aria_label: "{play_label}",
                        title: "{play_label}",
                        "data-testid": "remote-play-pause",
                        onclick: toggle,
                        Icon { name: play_icon.to_string(), class: "rs-ic rs-ic--md".to_string() }
                    }
                    button {
                        r#type: "button",
                        class: "rs-iconbtn",
                        aria_label: "Next",
                        title: "Next",
                        "data-testid": "remote-next",
                        onclick: next,
                        Icon { name: "next".to_string(), class: "rs-ic rs-ic--lg".to_string() }
                    }
                }
                PlayHereButton {}
                RemoteProgress { duration_ms, line: true }
            }
        };
    }

    rsx! {
        div { class: "rs-bar rs-bar--remote", role: "region", aria_label: "{region}", "data-testid": "remote-player",
            div { class: "rs-bar__np",
                button {
                    r#type: "button",
                    class: "rs-bar__art",
                    aria_label: "Open Now Playing",
                    title: "Open Now Playing",
                    disabled: song.is_none(),
                    onclick: open,
                    Artwork { cover_art: cover_art.clone(), server_id: server_id.clone(), size_px: 44, eager: true }
                }
                div { class: "rs-bar__meta",
                    div { class: "rs-bar__title rs-truncate", title: "{title}", "{title}" }
                    div { class: "rs-bar__sub",
                        span { class: "rs-truncate rs-remote__caption", "data-testid": "remote-caption",
                            Icon { name: "headphones".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                            "{caption}"
                        }
                        if !artist.is_empty() {
                            span { class: "rs-bar__sep", aria_hidden: "true", "·" }
                            span { class: "rs-truncate", "{artist}" }
                        }
                    }
                }
            }
            div { class: "rs-bar__mid",
                if target.is_some() {
                    div { class: "rs-bar__transport",
                        button {
                            r#type: "button",
                            class: "rs-iconbtn rs-iconbtn--play",
                            aria_label: "{play_label}",
                            title: "{play_label}",
                            "data-testid": "remote-play-pause",
                            onclick: toggle,
                            Icon { name: play_icon.to_string(), class: "rs-ic rs-ic--lg".to_string() }
                        }
                        button {
                            r#type: "button",
                            class: "rs-iconbtn",
                            aria_label: "Next",
                            title: "Next",
                            "data-testid": "remote-next",
                            onclick: next,
                            Icon { name: "next".to_string(), class: "rs-ic rs-ic--md".to_string() }
                        }
                    }
                }
                RemoteProgress { duration_ms, line: false }
            }
            div { class: "rs-bar__rt",
                PlayHereButton {}
            }
        }
    }
}

/// "Play here": moves the active device's playback to this one.
#[component]
fn PlayHereButton() -> Element {
    let sync = use_context::<RemoteSync>();
    rsx! {
        button {
            r#type: "button",
            class: "rs-btn rs-btn--tinted rs-btn--small",
            "data-testid": "remote-play-here",
            onclick: move |_| sync.play_here(),
            "Play Here"
        }
    }
}

/// The remote position, ticking locally from the last `state` and the
/// server clock. `line`: the mini player's hairline; else times and a bar.
#[component]
fn RemoteProgress(duration_ms: Option<u64>, line: bool) -> Element {
    let sync = use_context::<RemoteSync>();
    let mut tick = use_signal(|| 0u64);
    use_future(move || async move {
        loop {
            sleep_ms(PROGRESS_TICK_MS).await;
            tick.with_mut(|t| *t += 1);
        }
    });
    let _ = tick();
    let _ = sync.room_state.read();
    let position = sync.live_position_ms(duration_ms).unwrap_or(0);
    let fraction = match duration_ms {
        Some(duration) => (position as f64 / duration as f64).clamp(0.0, 1.0),
        None => 0.0,
    };
    let now = format_clock(position as f64 / 1000.0);
    if line {
        return rsx! {
            div { class: "rs-mini__line", aria_hidden: "true", "data-testid": "remote-position", "data-position-ms": "{position}",
                i { style: "transform:scaleX({fraction:.4})" }
            }
        };
    }
    let total = duration_ms.map(|d| format_clock(d as f64 / 1000.0)).unwrap_or_default();
    rsx! {
        div { class: "rs-remote__progress",
            span { class: "rs-time", "data-testid": "remote-position", "data-position-ms": "{position}", "{now}" }
            div { class: "rs-remote__track", aria_hidden: "true",
                i { style: "transform:scaleX({fraction:.4})" }
            }
            span { class: "rs-time", "{total}" }
        }
    }
}

/// "Play on" under the Output row: the other devices that can play, the
/// active one marked "Playing"; choosing one moves playback there. While
/// another device is active, this device is listed first ("This Mac") to
/// bring it back. `on_done` closes the menu.
#[component]
pub fn PlayOnChoices(on_done: EventHandler<()>) -> Element {
    let sync = use_context::<RemoteSync>();
    let devices = sync.play_on();
    if devices.is_empty() {
        return rsx! {};
    }
    let elsewhere = sync.active_elsewhere();
    let me = sync.this_device_name();
    rsx! {
        div {
            class: "rs-menu__choices-label",
            aria_hidden: "true",
            "Play on"
        }
        div {
            class: "rs-menu__play-on",
            role: "group",
            aria_label: "Play on",
            "data-testid": "output-play-on",
            if elsewhere.is_some() {
                button {
                    r#type: "button",
                    class: "rs-menu__choice",
                    role: "menuitem",
                    "data-testid": "output-play-here",
                    onclick: move |_| {
                        sync.play_here();
                        on_done.call(());
                    },
                    "This device ({me})"
                }
            }
            for (index, device) in devices.into_iter().enumerate() {
                button {
                    key: "{device.info.id}",
                    r#type: "button",
                    class: "rs-menu__choice",
                    role: "menuitemradio",
                    aria_checked: if device.active { "true" } else { "false" },
                    title: "{device.info.name}",
                    "data-testid": "output-play-on-{index}",
                    onclick: {
                        let id = device.info.id.clone();
                        move |_| {
                            sync.transfer_to(&id);
                            on_done.call(());
                        }
                    },
                    "{device.info.name}"
                    if device.active {
                        span { class: "rs-menu__choice-note", " · Playing" }
                    }
                }
            }
        }
    }
}
