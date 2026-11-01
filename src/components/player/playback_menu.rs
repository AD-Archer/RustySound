//! The wide bar's playback menu ("…"): Shuffle, Repeat and the song's
//! rating, which are used too rarely to sit in the bar itself. The ids other
//! parts of the app press (`shuffle-btn`, `repeat-btn`, `rating-btn`) stay
//! in the bar as hidden press targets (`controls::PressTargets`).

use super::controls::PressTargets;
use crate::api::Capabilities;
use crate::components::song_details::actions::is_live_song;
use crate::components::ui::{Menu, MenuGap, MenuItem};
use crate::components::{use_player, Icon};
use crate::db::RepeatMode;
use dioxus::prelude::*;

/// Where the menu is: closed, or open at a viewport point (None centres it).
type MenuAt = Option<Option<(f64, f64)>>;

/// The menu card's width (`.rs-menu--popover`) and roughly its height with
/// the rating row, to open it right-aligned just above the button.
const MENU_WIDTH: f64 = 270.0;
const MENU_HEIGHT: f64 = 190.0;

#[component]
pub(super) fn PlaybackMenu() -> Element {
    let mut menu_at = use_signal(|| None as MenuAt);
    let mut trigger = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let open = menu_at();

    rsx! {
        PressTargets { on_rate: move |_| menu_at.set(Some(None)) }
        button {
            r#type: "button",
            class: "rs-iconbtn rs-iconbtn--quiet",
            aria_label: "Playback options",
            title: "Playback options",
            aria_haspopup: "menu",
            aria_expanded: if open.is_some() { "true" } else { "false" },
            "data-testid": "player-more",
            onmounted: move |evt| trigger.set(Some(evt.data())),
            // Placed from the button itself, not the pointer, so Enter and
            // Space open it in the same place. Above the button: the bar sits
            // at the bottom of the window.
            onclick: move |_| async move {
                let node = trigger.peek().clone();
                let rect = match node {
                    Some(node) => node.get_client_rect().await.ok(),
                    None => None,
                };
                menu_at.set(Some(rect.map(|rect| {
                    (rect.max_x() - MENU_WIDTH, rect.min_y() - MENU_HEIGHT - 8.0)
                })));
            },
            Icon { name: "more".to_string(), class: "rs-ic rs-ic--sm".to_string() }
        }
        if let Some(anchor) = open {
            PlaybackMenuCard { anchor, on_close: move |_| menu_at.set(None) }
        }
    }
}

/// The menu itself. Stays open while you change things, so each row shows
/// its new state; the scrim or Escape closes it.
#[component]
fn PlaybackMenuCard(anchor: Option<(f64, f64)>, on_close: EventHandler<()>) -> Element {
    let mut player = use_player();
    let shuffle = (player.shuffle)();
    let repeat = (player.repeat)();
    let song = (player.now_playing)();
    let live = song.as_ref().is_some_and(is_live_song);
    let rated = song.as_ref().filter(|song| {
        !is_live_song(song)
            && Capabilities::for_source(&player.servers.read(), &song.server_id).star_ratings
    });
    let (repeat_label, repeat_icon) = match repeat {
        RepeatMode::Off => ("Off", "repeat"),
        RepeatMode::All => ("All", "repeat"),
        RepeatMode::One => ("One", "repeat-1"),
    };

    rsx! {
        Menu {
            label: "Playback options".to_string(),
            anchor,
            testid: "player-menu".to_string(),
            on_close: move |_| on_close.call(()),
            MenuItem {
                label: "Shuffle".to_string(),
                detail: Some(if shuffle { "On" } else { "Off" }.to_string()),
                icon: Some("shuffle".to_string()),
                disabled: live,
                testid: Some("player-menu-shuffle".to_string()),
                onclick: move |_| player.toggle_shuffle(),
            }
            MenuItem {
                label: "Repeat".to_string(),
                detail: Some(repeat_label.to_string()),
                icon: Some(repeat_icon.to_string()),
                disabled: live,
                testid: Some("player-menu-repeat".to_string()),
                onclick: move |_| player.cycle_repeat(),
            }
            if let Some(song) = rated.cloned() {
                MenuGap {}
                RatingRow { song }
            }
        }
    }
}

/// 1–5 stars plus clear, the track menu's look.
#[component]
fn RatingRow(song: crate::api::Song) -> Element {
    let player = use_player();
    let rating = song.user_rating.unwrap_or(0).min(5);
    rsx! {
        div {
            class: "rs-menu__stars",
            role: "group",
            aria_label: if rating > 0 { format!("Rating: {rating} of 5") } else { "Rating".to_string() },
            "data-testid": "player-menu-rating",
            for value in 1..=5u32 {
                button {
                    key: "{value}",
                    r#type: "button",
                    class: "rs-menu__star",
                    class: if value <= rating { "rs-menu__star--on" },
                    aria_label: "{value} star",
                    aria_pressed: if value == rating { "true" } else { "false" },
                    onclick: {
                        let song = song.clone();
                        move |_| player.set_rating(song.clone(), value)
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
                onclick: move |_| player.set_rating(song.clone(), 0),
                Icon { name: "x".to_string(), class: "rs-ic rs-ic--sm".to_string() }
            }
        }
    }
}
