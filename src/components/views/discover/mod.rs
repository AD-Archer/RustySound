//! Home: the app's start screen at `/` (the route still calls it `HomeView`).
//!
//! Library-first: a black stage with Cover Flow of recently played albums,
//! then horizontal shelves of cards: Playlists (with generated mixes), Songs
//! you might like, Albums you might like, New in your library, Haven't heard
//! in a while, and, only when Settings allows it, Outside your server.
//! Data comes from `api::discovery`, loaded progressively into one signal per
//! shelf (see `data.rs`), so renders never refetch.

mod actions;
mod cards;
mod data;
mod mix_detail;
mod mixes;
mod offserver;
mod preview;
pub(super) mod saved;
mod shelf;
mod shelves;
mod stage;

use crate::api::ServerConfig;
use crate::components::ui::{use_layout, MainScroll};
use crate::db::AppSettings;
use actions::{NoticeCapsule, Toast};
use data::use_home_data;
pub use mix_detail::MixDetailView;
use dioxus::prelude::*;
use mixes::{HomeMenuHost, HomeMenus};
use offserver::OffServerShelf;
use preview::use_preview_provider;
use shelves::{AlbumsLikeShelf, NewShelf, PlaylistsShelf, RewindShelf, SongsShelf};
use stage::HomeStage;

/// Pull distance (after damping) that triggers a refresh on compact.
pub const PULL_TRIGGER_PX: f64 = 64.0;

#[component]
pub fn HomeView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let settings = use_context::<Signal<AppSettings>>();
    let main_scroll = try_use_context::<MainScroll>();
    let wide = !use_layout().is_compact();
    use_context_provider(Toast::new);
    use_context_provider(|| HomeMenus(Signal::new(None)));
    use_preview_provider();
    let data = use_home_data(servers, settings);
    let mut pull = use_signal(|| 0.0f64);
    let mut pull_start = use_signal(|| None::<f64>);

    // No server, or every server switched off: the first-run welcome handles both.
    if servers.read().is_empty() || data.active.read().is_empty() {
        return rsx! { crate::components::views::FirstRun {} };
    }

    let at_top = move || main_scroll.map(|m| *m.top.peek() <= 0.0).unwrap_or(true);
    let end_pull = move |_| {
        if pull_start.peek().is_none() {
            return;
        }
        pull_start.set(None);
        if *pull.peek() >= PULL_TRIGGER_PX {
            data.refresh();
        }
        if *pull.peek() != 0.0 {
            pull.set(0.0);
        }
    };
    let offline = (data.offline)();

    rsx! {
        div {
            class: "rs-view rs-view--flush rs-home",
            class: if wide { "rs-home--wide" },
            "data-testid": "home",
            ontouchstart: move |evt| {
                if wide || *data.refreshing.peek() || !at_top() {
                    return;
                }
                pull_start.set(evt.touches().first().map(|t| t.client_coordinates().y));
            },
            ontouchmove: move |evt| {
                let Some(start) = *pull_start.peek() else {
                    return;
                };
                let Some(y) = evt.touches().first().map(|t| t.client_coordinates().y) else {
                    return;
                };
                // Damped, and quantised so the indicator re-renders sparingly.
                let distance = (((y - start) * 0.5).max(0.0) / 4.0).floor() * 4.0;
                if distance != *pull.peek() {
                    pull.set(distance.min(PULL_TRIGGER_PX * 1.5));
                }
            },
            ontouchend: end_pull,
            ontouchcancel: move |_| {
                pull_start.set(None);
                pull.set(0.0);
            },

            HomeStage { wide, pull }

            div { class: "rs-home__page",
                if offline {
                    p { class: "rs-home__note rs-home__note--top", role: "status",
                        "You're offline. Home shows what's cached; only downloaded songs play."
                    }
                }
                PlaylistsShelf {}
                SongsShelf {}
                AlbumsLikeShelf {}
                NewShelf {}
                RewindShelf {}
                OffServerShelf {}
            }
            HomeMenuHost {}
            NoticeCapsule {}
        }
    }
}
