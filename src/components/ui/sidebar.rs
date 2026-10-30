//! Sidebar for the wide layout.

use crate::api::ServerConfig;
use crate::components::ui::WindowDragRegion;
use crate::components::{AppView, Icon, Navigation, SongDetailsController};
use crate::db::AppSettings;
use dioxus::prelude::*;

/// Home, Search; Library (Playlists, Artists, Albums, Songs, Favorites,
/// Downloads); Radio, Stats; Settings with the server status at the bottom.
/// The "Library" heading opens `/library` (Random, Bookmarks and Queue live
/// there). Reads route, servers and settings itself; no props.
/// Testids: `nav-<route>` (nav-discover for Home, nav-albums, …, nav-library,
/// nav-settings).
#[component]
pub fn Sidebar() -> Element {
    let view = use_route::<AppView>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let navigation = use_context::<Navigation>();
    let song_details = use_context::<SongDetailsController>();

    let go = move |target: AppView| {
        let mut song_details = song_details.clone();
        move |_| {
            song_details.close();
            navigation.navigate_to(target.clone());
        }
    };

    let is = |target: &AppView| -> bool {
        match (&view, target) {
            (AppView::AlbumsWithGenre { .. }, AppView::Albums {}) => true,
            (AppView::AlbumDetailView { .. }, AppView::Albums {}) => true,
            (AppView::ArtistDetailView { .. }, AppView::ArtistsView {}) => true,
            (AppView::PlaylistDetailView { .. }, AppView::PlaylistsView {}) => true,
            (current, target) => current == target,
        }
    };

    let (status_text, led_class) = {
        let list = servers.read();
        let active: Vec<&ServerConfig> = list.iter().filter(|s| s.active).collect();
        if app_settings.read().offline_mode {
            ("Offline mode".to_string(), "rs-led rs-led--warn")
        } else if let Some(first) = active.first() {
            let name = if first.name.trim().is_empty() {
                first
                    .url
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .to_string()
            } else {
                first.name.clone()
            };
            let text = if active.len() > 1 {
                format!("{name} +{}", active.len() - 1)
            } else {
                name
            };
            (text, "rs-led")
        } else if list.is_empty() {
            ("No server".to_string(), "rs-led rs-led--off")
        } else {
            ("No active server".to_string(), "rs-led rs-led--off")
        }
    };

    let groups: [(Option<&str>, Vec<(&str, &str, &str, AppView)>); 3] = [
        (
            None,
            vec![
                ("Home", "home", "nav-discover", AppView::HomeView {}),
                ("Search", "search", "nav-search", AppView::SearchView {}),
            ],
        ),
        (
            Some("Library"),
            vec![
                ("Playlists", "playlist", "nav-playlists", AppView::PlaylistsView {}),
                ("Artists", "artist", "nav-artists", AppView::ArtistsView {}),
                ("Albums", "album", "nav-albums", AppView::Albums {}),
                ("Songs", "music", "nav-songs", AppView::SongsView {}),
                ("Favorites", "star", "nav-favorites", AppView::FavoritesView {}),
                ("Downloads", "download", "nav-downloads", AppView::DownloadsView {}),
            ],
        ),
        (
            None,
            vec![
                ("Radio", "radio", "nav-radio", AppView::RadioView {}),
                ("Stats", "stats", "nav-stats", AppView::StatsView {}),
            ],
        ),
    ];

    rsx! {
        aside { class: "rs-sidebar", aria_label: "Sidebar",
            WindowDragRegion { class: "rs-drag--sidebar".to_string() }
            nav { aria_label: "Main",
                for (heading, items) in groups {
                    div { class: "rs-side__grp",
                        if let Some(heading) = heading {
                            button {
                                r#type: "button",
                                class: "rs-side__head",
                                "data-testid": "nav-library",
                                aria_current: if matches!(view, AppView::LibraryView {}) { Some("page") } else { None },
                                onclick: go(AppView::LibraryView {}),
                                "{heading}"
                            }
                        }
                        for (label, icon, testid, target) in items {
                            button {
                                key: "{testid}",
                                r#type: "button",
                                class: "rs-nav",
                                "data-testid": testid,
                                aria_current: if is(&target) { Some("page") } else { None },
                                onclick: go(target.clone()),
                                Icon { name: icon.to_string(), class: "rs-nav__icon".to_string() }
                                span { "{label}" }
                            }
                        }
                    }
                }
            }
            div { class: "rs-side__foot",
                button {
                    r#type: "button",
                    class: "rs-nav",
                    "data-testid": "nav-settings",
                    aria_current: if matches!(view, AppView::SettingsView {}) { Some("page") } else { None },
                    onclick: go(AppView::SettingsView {}),
                    Icon { name: "settings".to_string(), class: "rs-nav__icon".to_string() }
                    span { "Settings" }
                }
                div { class: "rs-side__server", "data-testid": "server-status",
                    span { class: "{led_class}", aria_hidden: "true" }
                    span { "{status_text}" }
                }
            }
        }
    }
}
