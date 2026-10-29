//! "Mixes for you": generated mixes as list rows with a play button and a
//! "…" menu (Play, Shuffle, Add to Queue, Save as Playlist).

use super::actions::{use_player, Player, Toast};
use super::saved::{parse_saved, remembered, save_mix, saved_message, serialize_saved};
use super::DiscoverData;
use crate::api::discovery::{Mix, MixKind};
use crate::api::{QueueSourceKind, ServerConfig};
use crate::components::ui::{
    use_layout, ArtworkMosaic, IconButton, IconButtonSize, InsetList, ListInset, Skeleton,
};
use crate::components::Icon;
use crate::db::{save_settings, AppSettings};
use dioxus::prelude::*;

fn play_mix(player: Player, toast: Toast, mix: &Mix, shuffle: bool) {
    let source = format!("discover:{}", mix.id);
    if let Err(message) = player.play(mix.songs.clone(), QueueSourceKind::RandomMix, source, shuffle) {
        toast.show(message, true);
    }
}

fn count_label(mix: &Mix) -> String {
    let songs = if mix.song_count == 1 {
        "1 song".to_string()
    } else {
        format!("{} songs", mix.song_count)
    };
    if mix.length_label.is_empty() {
        songs
    } else {
        format!("{songs} · {}", mix.length_label)
    }
}

/// The mix list for "For You". Skeleton while loading, a quiet note when empty.
#[component]
pub fn MixList(wide: bool) -> Element {
    let data = use_context::<DiscoverData>();
    let mut menu = use_signal(|| None::<(usize, Option<(f64, f64)>)>);
    let mixes = data.mixes.read();
    let Some(section) = mixes.as_ref() else {
        return rsx! {
            Skeleton { rows: 4 }
        };
    };
    if section.mixes.is_empty() {
        return rsx! {
            InsetList { inset: ListInset::Text, label: "Mixes for you".to_string(),
                li { class: "rs-discover__quiet", "Mixes show up once you've played some music." }
            }
        };
    }
    let open = menu().and_then(|(index, anchor)| section.mixes.get(index).cloned().map(|m| (m, anchor)));
    rsx! {
        InsetList { label: "Mixes for you".to_string(), testid: "mix-list".to_string(),
            for (index, mix) in section.mixes.iter().enumerate() {
                MixRow {
                    key: "{mix.id}",
                    mix: mix.clone(),
                    wide,
                    on_menu: move |anchor| menu.set(Some((index, anchor))),
                }
            }
        }
        if let Some((mix, anchor)) = open {
            MixMenu { key: "{mix.id}", mix, anchor, on_close: move |_| menu.set(None) }
        }
    }
}

/// One mix: tap the row or the play button to play; "…" opens the menu.
#[component]
fn MixRow(mix: Mix, wide: bool, on_menu: EventHandler<Option<(f64, f64)>>) -> Element {
    let player = use_player();
    let toast = use_context::<Toast>();
    let covers: Vec<(String, String)> = mix
        .covers
        .iter()
        .map(|c| (c.server_id.clone(), c.cover_art.clone()))
        .collect();
    let fresh = mix.kind == MixKind::Discovery && !mix.freshness.is_empty();
    let counts = count_label(&mix);
    let play = {
        let mix = mix.clone();
        move |_| play_mix(player, toast, &mix, false)
    };
    rsx! {
        li { "data-testid": "mix-row",
            div { class: "rs-row rs-discover-mix",
                button {
                    r#type: "button",
                    class: "rs-discover-mix__main",
                    aria_label: "Play {mix.name}, {counts}",
                    onclick: play.clone(),
                    ArtworkMosaic { covers, size_px: 44 }
                    span { class: "rs-row__meta",
                        span { class: "rs-row__title",
                            span { "{mix.name}" }
                            if fresh && wide {
                                span { class: "rs-discover-mix__fresh", "{mix.freshness}" }
                            } else if fresh {
                                span {
                                    class: "rs-row__fresh",
                                    role: "img",
                                    aria_label: "{mix.freshness}",
                                }
                            }
                        }
                        span { class: "rs-row__sub", style: "display:block", "{mix.blurb}" }
                        if !wide {
                            span { class: "rs-row__why rs-discover__num", style: "display:block", "{counts}" }
                        }
                    }
                }
                if wide {
                    span { class: "rs-row__detail", "{counts}" }
                }
                button {
                    r#type: "button",
                    class: "rs-playbtn",
                    aria_label: "Play {mix.name}",
                    title: "Play",
                    onclick: play,
                    Icon { name: "play".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                }
                IconButton {
                    icon: "more-horizontal".to_string(),
                    label: format!("More for {}", mix.name),
                    quiet: true,
                    size: IconButtonSize::Small,
                    testid: "mix-more".to_string(),
                    onclick: move |evt: MouseEvent| {
                        let point = evt.client_coordinates();
                        let anchor = (point.x > 0.0 || point.y > 0.0).then_some((point.x, point.y));
                        on_menu.call(anchor);
                    },
                }
            }
        }
    }
}

/// The mix's action menu: a bottom sheet on compact, a popover on wide.
#[component]
fn MixMenu(mix: Mix, anchor: Option<(f64, f64)>, on_close: EventHandler<()>) -> Element {
    let layout = use_layout();
    let player = use_player();
    let toast = use_context::<Toast>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let settings = use_context::<Signal<AppSettings>>();

    let act = {
        let mix = mix.clone();
        move |action: &'static str| {
            on_close.call(());
            match action {
                "play" => play_mix(player, toast, &mix, false),
                "shuffle" => play_mix(player, toast, &mix, true),
                "queue" => match player.enqueue(mix.songs.clone()) {
                    Ok(count) => toast.show(format!("Added {count} songs to the queue"), false),
                    Err(message) => toast.show(message, true),
                },
                _ => save_to_playlist(mix.clone(), servers, settings, toast),
            }
        }
    };
    let items: [(&str, &str, &str); 4] = [
        ("play", "Play", "play"),
        ("shuffle", "Shuffle", "shuffle"),
        ("queue", "Add to Queue", "add-to-queue"),
        ("save", "Save as Playlist", "playlist"),
    ];
    let covers: Vec<(String, String)> = mix
        .covers
        .iter()
        .map(|c| (c.server_id.clone(), c.cover_art.clone()))
        .collect();

    let card = rsx! {
        div {
            class: "rs-menu",
            class: if !layout.is_compact() { "rs-menu--popover" },
            style: if !layout.is_compact() { Some(popover_style(anchor)) } else { None },
            role: "menu",
            aria_label: "{mix.name}",
            tabindex: "-1",
            "data-testid": "mix-menu",
            onmounted: move |evt| async move {
                let _ = evt.set_focus(true).await;
            },
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    evt.prevent_default();
                    on_close.call(());
                }
            },
            div { class: "rs-menu__head",
                ArtworkMosaic { covers, size_px: 44 }
                div { class: "rs-menu__head-meta",
                    div { class: "rs-menu__head-title", "{mix.name}" }
                    div { class: "rs-menu__head-sub", "{count_label(&mix)}" }
                }
            }
            div { class: "rs-menu__gap", aria_hidden: "true" }
            for (action, label, icon) in items {
                button {
                    key: "{action}",
                    r#type: "button",
                    class: "rs-menu__item",
                    role: "menuitem",
                    "data-testid": "mix-menu-{action}",
                    onclick: {
                        let act = act.clone();
                        move |_| act(action)
                    },
                    "{label}"
                    Icon { name: icon.to_string(), class: "rs-ic".to_string() }
                }
            }
        }
    };

    if layout.is_compact() {
        return rsx! {
            div { class: "rs-scrim", aria_hidden: "true", onclick: move |_| on_close.call(()) }
            div { class: "rs-sheet rs-sheet--menu", role: "dialog", aria_label: "{mix.name}",
                {card}
                div { class: "rs-menu",
                    button {
                        r#type: "button",
                        class: "rs-menu__item rs-menu__cancel",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                }
            }
        };
    }
    rsx! {
        {card}
        div {
            class: "rs-scrim rs-scrim--clear",
            aria_hidden: "true",
            onclick: move |_| on_close.call(()),
        }
    }
}

/// Popover position: left of the "…" button, kept inside the window.
fn popover_style(anchor: Option<(f64, f64)>) -> String {
    match anchor {
        Some((x, y)) => format!(
            "left:clamp(8px, {:.0}px, calc(100vw - 278px));top:clamp(8px, {:.0}px, calc(100dvh - 300px))",
            x - 262.0,
            y + 10.0
        ),
        None => "left:50%;top:50%;transform:translate(-50%,-50%)".to_string(),
    }
}

/// Save (or re-save, replacing the earlier copy) and remember where it went.
fn save_to_playlist(
    mix: Mix,
    servers: Signal<Vec<ServerConfig>>,
    mut settings: Signal<AppSettings>,
    toast: Toast,
) {
    if settings.peek().offline_mode {
        toast.show("Saving a playlist needs a connection.", true);
        return;
    }
    let servers = servers.peek().clone();
    let map = parse_saved(&settings.peek().discover_saved_mix_playlists);
    let prior = remembered(&map, &mix.id, &servers);
    toast.show(format!("Saving “{}”…", mix.name), false);
    toast.spawn(async move {
        match save_mix(&servers, &mix, prior).await {
            Ok(outcome) => {
                if let Some(saved) = outcome.saved.clone() {
                    let mut next = settings.peek().clone();
                    let mut map = parse_saved(&next.discover_saved_mix_playlists);
                    map.insert(mix.id.clone(), saved);
                    next.discover_saved_mix_playlists = serialize_saved(&map);
                    settings.set(next.clone());
                    let _ = save_settings(next).await;
                }
                toast.show(saved_message(&mix, &outcome), false);
            }
            Err(message) => toast.show(format!("Couldn't save “{}”: {message}", mix.name), true),
        }
    });
}
