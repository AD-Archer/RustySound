//! Generated mixes on the Playlists shelf. Tapping a mix card plays it; its
//! "…" menu has Play, Shuffle, Add to Queue and Save as Playlist (re-saving
//! replaces the earlier copy).

use super::actions::{use_player, Player, Toast};
use super::cards::{songs_label, CARD_ART_PX};
use super::saved::{parse_saved, remembered, save_mix, saved_message, serialize_saved};
use super::offserver::OffServerMenu;
use super::shelf::Card;
use crate::api::discovery::{Mix, OffServerItem};
use crate::api::{QueueSourceKind, ServerConfig};
use crate::components::ui::{ActionMenu, Artwork, MenuAction};
use crate::db::{save_settings, AppSettings};
use dioxus::prelude::*;

fn play_mix(player: Player, toast: Toast, mix: &Mix, shuffle: bool) {
    let source = format!("discover:{}", mix.id);
    if let Err(message) = player.play(mix.songs.clone(), QueueSourceKind::RandomMix, source, shuffle) {
        toast.show(message, true);
    }
}

/// "25 songs · 1 hr 38 min".
fn count_label(mix: &Mix) -> String {
    let songs = songs_label(mix.song_count);
    if mix.length_label.is_empty() {
        songs
    } else {
        format!("{songs} · {}", mix.length_label)
    }
}

/// The "…" menu Home shows, if any. Menus mount once at Home's root (not
/// inside a shelf's list markup), so only one is open at a time.
#[derive(Clone, PartialEq)]
pub enum HomeMenu {
    Mix(Mix),
    OffServer(OffServerItem),
}

/// Context handle for Home's card menus.
#[derive(Clone, Copy)]
pub struct HomeMenus(pub Signal<Option<(HomeMenu, Option<(f64, f64)>)>>);

impl HomeMenus {
    pub fn open(&self, menu: HomeMenu, anchor: Option<(f64, f64)>) {
        let mut signal = self.0;
        signal.set(Some((menu, anchor)));
    }

    pub fn close(&self) {
        let mut signal = self.0;
        if signal.peek().is_some() {
            signal.set(None);
        }
    }
}

/// Mounts the open card menu (at Home's root).
#[component]
pub fn HomeMenuHost() -> Element {
    let menus = use_context::<HomeMenus>();
    let Some((menu, anchor)) = (menus.0)() else {
        return rsx! {};
    };
    let on_close = move |_| menus.close();
    match menu {
        HomeMenu::Mix(mix) => rsx! {
            MixMenu { key: "{mix.id}", mix, anchor, on_close }
        },
        HomeMenu::OffServer(item) => rsx! {
            OffServerMenu { key: "{item.id}", item, anchor, on_close }
        },
    }
}

/// One mix: a 2×2 mosaic, the name, a small "Mix" tag and the counts.
#[component]
pub fn MixCard(mix: Mix) -> Element {
    let player = use_player();
    let toast = use_context::<Toast>();
    let menus = use_context::<HomeMenus>();
    let covers: Vec<(String, String)> = mix
        .covers
        .iter()
        .map(|c| (c.server_id.clone(), c.cover_art.clone()))
        .collect();
    let play = {
        let mix = mix.clone();
        move |_| play_mix(player, toast, &mix, false)
    };
    let for_menu = mix.clone();
    rsx! {
        Card {
            title: mix.name.clone(),
            // Short enough to fit beside the tag; the length is in the menu.
            subtitle: songs_label(mix.song_count),
            reason: Some(mix.blurb.clone()),
            tag: Some("Mix".to_string()),
            testid: "mix-card".to_string(),
            art: rsx! {
                MixArt { covers }
            },
            onclick: play,
            on_menu: move |anchor| menus.open(HomeMenu::Mix(for_menu.clone()), anchor),
        }
    }
}

/// A mix's art, filling the card: a 2×2 mosaic of its first four album
/// covers, or one cover when it has fewer.
#[component]
fn MixArt(covers: Vec<(String, String)>) -> Element {
    if covers.len() < 4 {
        let first = covers.first().cloned();
        return rsx! {
            Artwork {
                server_id: first.as_ref().map(|c| c.0.clone()).unwrap_or_default(),
                cover_art: first.map(|c| c.1),
                size_px: CARD_ART_PX,
                fill: true,
                placeholder: "playlist".to_string(),
            }
        };
    }
    rsx! {
        div { class: "rs-hcard__mosaic",
            for (server_id, cover) in covers.into_iter().take(4) {
                Artwork {
                    key: "{server_id}:{cover}",
                    server_id,
                    cover_art: Some(cover),
                    size_px: CARD_ART_PX / 2,
                    fill: true,
                    placeholder: "album".to_string(),
                }
            }
        }
    }
}

/// The mix's "…" menu (kit `ActionMenu`: a sheet on compact, a popover on wide).
#[component]
fn MixMenu(mix: Mix, anchor: Option<(f64, f64)>, on_close: EventHandler<()>) -> Element {
    let player = use_player();
    let toast = use_context::<Toast>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let settings = use_context::<Signal<AppSettings>>();
    let actions = vec![
        MenuAction::new("play", "Play", "play"),
        MenuAction::new("shuffle", "Shuffle", "shuffle"),
        MenuAction::new("queue", "Add to Queue", "add-to-queue"),
        MenuAction::new("save", "Save as Playlist", "playlist"),
    ];
    let title = mix.name.clone();
    rsx! {
        ActionMenu {
            title: Some(title),
            message: Some(count_label(&mix)),
            actions,
            anchor,
            testid: "mix-menu".to_string(),
            on_close: move |_| on_close.call(()),
            on_action: move |key: String| match key.as_str() {
                "play" => play_mix(player, toast, &mix, false),
                "shuffle" => play_mix(player, toast, &mix, true),
                "queue" => match player.enqueue(mix.songs.clone()) {
                    Ok(count) => toast.show(format!("Added {count} songs to the queue"), false),
                    Err(message) => toast.show(message, true),
                },
                _ => save_to_playlist(mix.clone(), servers, settings, toast),
            },
        }
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
