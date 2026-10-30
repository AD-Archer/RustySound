//! Generated mixes on the Playlists shelf. Tapping a mix card opens its page
//! (`/mix/<slug>`), like a server playlist card; the round play button over
//! the art plays it, and its "…" menu has Play, Shuffle, Add to Queue and
//! Save as Playlist (re-saving replaces the earlier copy).
//!
//! A mix is RustySound's, not one of the user's server playlists, so its
//! card carries the RustySound mark on the art and says "RustySound"
//! where a playlist says its song count (see [`RsMark`]).

use super::actions::{use_player, Player, Toast};
use super::cards::{songs_label, CARD_ART_PX};
use super::offserver::OffServerMenu;
use super::saved::save_and_remember;
use super::shelf::Card;
use crate::api::discovery::{mix_slug, Mix, OffServerItem};
use crate::api::{QueueSourceKind, ServerConfig};
use crate::components::ui::env::app_mark_src;
use crate::components::ui::{Artwork, Menu, MenuAction};
use crate::components::{AppView, Icon, Navigation};
use crate::db::AppSettings;
use dioxus::prelude::*;

/// The line that names who made a mix, on cards and the mix page.
pub const MIX_BYLINE: &str = "RustySound";

fn play_mix(player: Player, toast: Toast, mix: &Mix, shuffle: bool) {
    let source = format!("discover:{}", mix.id);
    if let Err(message) = player.play(mix.songs.clone(), QueueSourceKind::RandomMix, source, shuffle) {
        toast.show(message, true);
    }
}

/// "25 songs · 1 hr 38 min".
pub fn count_label(mix: &Mix) -> String {
    let songs = songs_label(mix.song_count);
    if mix.length_label.is_empty() {
        songs
    } else {
        format!("{songs} · {}", mix.length_label)
    }
}

/// The route of a mix's page.
pub fn mix_route(mix: &Mix) -> AppView {
    AppView::MixDetailView {
        mix_id: mix_slug(&mix.id),
    }
}

/// `(server_id, cover_art)` pairs for [`MixArt`].
pub fn mix_covers(mix: &Mix) -> Vec<(String, String)> {
    mix.covers
        .iter()
        .map(|c| (c.server_id.clone(), c.cover_art.clone()))
        .collect()
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

/// The RustySound app mark, the real icon. Decorative by default: every place
/// that shows it also says "RustySound" in text.
/// * `class`: sizing and placement (`rs-mixmark--card`, `rs-mixmark--title`).
#[component]
pub fn RsMark(#[props(default)] class: String) -> Element {
    rsx! {
        img {
            class: "rs-mixmark {class}",
            src: app_mark_src(),
            alt: "",
            aria_hidden: "true",
            draggable: "false",
            "data-testid": "rs-mark",
        }
    }
}

/// One mix: a 2×2 mosaic stamped with the RustySound mark, the name,
/// "RustySound" (who made it) and the blurb. Tap opens the mix page.
#[component]
pub fn MixCard(mix: Mix) -> Element {
    let player = use_player();
    let toast = use_context::<Toast>();
    let menus = use_context::<HomeMenus>();
    let navigation = use_context::<Navigation>();
    let covers = mix_covers(&mix);
    let target = mix_route(&mix);
    let to_play = mix.clone();
    let for_menu = mix.clone();
    rsx! {
        Card {
            title: mix.name.clone(),
            // Who made it, where a playlist card shows its song count; the
            // count is on the page and in the menu.
            subtitle: MIX_BYLINE.to_string(),
            reason: Some(mix.blurb.clone()),
            testid: "mix-card".to_string(),
            art: rsx! {
                MixArt { covers, size_px: CARD_ART_PX }
                RsMark { class: "rs-mixmark--card".to_string() }
            },
            overlay: rsx! {
                button {
                    r#type: "button",
                    class: "rs-hcard__play",
                    aria_label: "Play {mix.name}",
                    title: "Play",
                    "data-testid": "mix-card-play",
                    onclick: move |_| play_mix(player, toast, &to_play, false),
                    Icon { name: "play".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                }
            },
            onclick: move |_| navigation.navigate_to(target.clone()),
            on_menu: move |anchor| menus.open(HomeMenu::Mix(for_menu.clone()), anchor),
        }
    }
}

/// A mix's art, filling its box: a 2×2 mosaic of its first four album
/// covers, or one cover when it has fewer.
/// * `size_px`: the box's edge, for the artwork request size.
/// * `eager`: load now (the mix page's hero).
#[component]
pub fn MixArt(covers: Vec<(String, String)>, size_px: u32, #[props(default)] eager: bool) -> Element {
    if covers.len() < 4 {
        let first = covers.first().cloned();
        return rsx! {
            Artwork {
                server_id: first.as_ref().map(|c| c.0.clone()).unwrap_or_default(),
                cover_art: first.map(|c| c.1),
                size_px,
                fill: true,
                eager,
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
                    size_px: size_px / 2,
                    fill: true,
                    eager,
                    placeholder: "album".to_string(),
                }
            }
        }
    }
}

/// The mix's "…" menu (kit `Menu`: a sheet on compact, a popover on wide).
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
        Menu {
            title: Some(title),
            message: Some(format!("Made by RustySound · {}", count_label(&mix))),
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
    settings: Signal<AppSettings>,
    toast: Toast,
) {
    if settings.peek().offline_mode {
        toast.show("Saving a playlist needs a connection.", true);
        return;
    }
    let servers = servers.peek().clone();
    toast.show(format!("Saving “{}”…", mix.name), false);
    toast.spawn(async move {
        match save_and_remember(mix, servers, settings).await {
            Ok(message) => toast.show(message, false),
            Err(message) => toast.show(message, true),
        }
    });
}
