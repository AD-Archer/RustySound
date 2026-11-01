//! Android TV's navigation: the sidebar's places as a row of buttons at the
//! top of every page, tvOS style (the TV has no sidebar or tab bar). Home
//! itself is the regular Home: Cover Flow, then the suggestion shelves.

use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

/// (label, icon, page) for each button, in sidebar order.
fn places() -> [(&'static str, &'static str, AppView); 12] {
    [
        ("Home", "home", AppView::HomeView {}),
        ("Search", "search", AppView::SearchView {}),
        ("Albums", "album", AppView::Albums {}),
        ("Artists", "artist", AppView::ArtistsView {}),
        ("Playlists", "playlist", AppView::PlaylistsView {}),
        ("Songs", "music", AppView::SongsView {}),
        ("Favorites", "heart", AppView::FavoritesView {}),
        ("Radio", "radio", AppView::RadioView {}),
        ("Up Next", "queue", AppView::QueueView {}),
        ("Downloads", "download", AppView::DownloadsView {}),
        ("Stats", "stats", AppView::StatsView {}),
        ("Settings", "settings", AppView::SettingsView {}),
    ]
}

#[component]
pub fn TvNavBar() -> Element {
    let navigation = use_context::<Navigation>();
    let current = use_route::<AppView>();

    rsx! {
        nav { class: "rs-tv-nav", aria_label: "Main", "data-testid": "tv-nav",
            for (label, icon, page) in places() {
                button {
                    key: "{label}",
                    r#type: "button",
                    class: "rs-tv-nav__item",
                    class: if page == current { "rs-tv-nav__item--on" },
                    aria_current: if page == current { "page" },
                    "data-testid": "tv-nav-{label.to_lowercase().replace(' ', \"-\")}",
                    onclick: move |_| navigation.navigate_to(page.clone()),
                    Icon { name: icon.to_string(), class: "rs-ic".to_string() }
                    span { "{label}" }
                }
            }
        }
    }
}
