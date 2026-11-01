//! TV-focused home layout with large focusable cards instead of sidebar.
//! Optimized for 10-foot UI and D-pad navigation.

use crate::components::{AppView, Navigation, use_servers, Icon};
use dioxus::prelude::*;

#[derive(Clone)]
struct TvCard {
    label: &'static str,
    icon: &'static str,
    testid: &'static str,
    route: AppView,
}

fn tv_cards() -> Vec<TvCard> {
    vec![
        TvCard { label: "Home", icon: "home", testid: "tv-nav-home", route: AppView::HomeView {} },
        TvCard { label: "Search", icon: "search", testid: "tv-nav-search", route: AppView::SearchView {} },
        TvCard { label: "Albums", icon: "album", testid: "tv-nav-albums", route: AppView::Albums {} },
        TvCard { label: "Artists", icon: "artist", testid: "tv-nav-artists", route: AppView::ArtistsView {} },
        TvCard { label: "Playlists", icon: "playlist", testid: "tv-nav-playlists", route: AppView::PlaylistsView {} },
        TvCard { label: "Songs", icon: "music", testid: "tv-nav-songs", route: AppView::SongsView {} },
        TvCard { label: "Favorites", icon: "heart", testid: "tv-nav-favorites", route: AppView::FavoritesView {} },
        TvCard { label: "Downloads", icon: "download", testid: "tv-nav-downloads", route: AppView::DownloadsView {} },
        TvCard { label: "Radio", icon: "radio", testid: "tv-nav-radio", route: AppView::RadioView {} },
        TvCard { label: "Stats", icon: "stats", testid: "tv-nav-stats", route: AppView::StatsView {} },
        TvCard { label: "Queue", icon: "queue", testid: "tv-nav-queue", route: AppView::QueueView {} },
        TvCard { label: "Settings", icon: "settings", testid: "tv-nav-settings", route: AppView::SettingsView {} },
    ]
}

#[component]
pub fn TvHome() -> Element {
    let navigation = use_context::<Navigation>();
    let servers = use_servers();
    let cards = tv_cards();

    rsx! {
        div {
            class: "rs-view rs-tv-home",
            "data-testid": "tv-home",
            role: "main",
            aria_label: "TV Home",
            div { class: "rs-tv-home__header",
                img {
                    class: "rs-tv-home__logo",
                    src: crate::components::ui::env::app_mark_src(),
                    alt: "RustySound",
                    width: "64",
                    height: "64",
                }
                h1 { class: "rs-tv-home__title", "RustySound" }
                if let Some(server) = servers.read().first() {
                    span { class: "rs-tv-home__server", "{server.name}" }
                }
            }
            div { class: "rs-tv-home__grid", role: "grid",
                for card in cards {
                    button {
                        key: "{card.label}",
                        r#type: "button",
                        class: "rs-tv-card",
                        role: "gridcell",
                        tabindex: "0",
                        "data-testid": card.testid,
                        aria_label: card.label,
                        onclick: {
                            let route = card.route.clone();
                            move |_| navigation.navigate_to(route.clone())
                        },
                        div { class: "rs-tv-card__icon",
                            Icon { name: card.icon.to_string(), class: "rs-ic rs-ic--xl".to_string() }
                        }
                        span { class: "rs-tv-card__label", "{card.label}" }
                    }
                }
            }
        }
    }
}
