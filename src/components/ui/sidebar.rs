//! Sidebar for the wide layout.

use crate::api::{Capabilities, ServerConfig};
use crate::components::ui::WindowDragRegion;
use crate::components::views::SidebarPlaylists;
use crate::components::{use_servers, use_setting, AppView, Icon, Navigation, PageMotion, SongDetailsController};
use dioxus::prelude::*;

/// Whether any active server keeps radio stations. With no server yet, Radio
/// stays (most servers do). Also used by the Library page's Radio tile.
pub fn radio_available(servers: &[ServerConfig]) -> bool {
    let mut active = servers.iter().filter(|s| s.active).peekable();
    active.peek().is_none() || active.any(|s| Capabilities::of(s).internet_radio)
}

/// Home, Search; Library (Playlists, Artists, Albums, Songs, Favorites,
/// Downloads); Radio, Stats; the user's playlists (drop targets for carried
/// songs, `SidebarPlaylists`); Settings with the server status at the bottom.
/// The "Library" heading opens `/library` (Random, Bookmarks and Queue live
/// there). Reads route, servers and settings itself; no props.
/// Testids: `nav-<route>` (nav-discover for Home, nav-albums, …, nav-library,
/// nav-settings).
#[component]
pub fn Sidebar() -> Element {
    let view = use_route::<AppView>();
    let servers = use_servers();
    let offline = use_setting(|s| s.offline_mode);
    let navigation = use_context::<Navigation>();
    let song_details = use_context::<SongDetailsController>();

    let go = move |target: AppView| {
        let mut song_details = song_details.clone();
        move |_| {
            song_details.close();
            // Sidebar items are sections, not deeper pages: cross-fade.
            navigation.navigate_with(target.clone(), PageMotion::Tab);
        }
    };

    let is = |target: &AppView| -> bool {
        match (&view, target) {
            (AppView::AlbumsWithGenre { .. }, AppView::Albums {}) => true,
            (AppView::AlbumDetailView { .. }, AppView::Albums {}) => true,
            (AppView::ArtistDetailView { .. }, AppView::ArtistsView {}) => true,
            (AppView::PlaylistDetailView { .. }, AppView::PlaylistsView {}) => true,
            (AppView::MixDetailView { .. }, AppView::HomeView {}) => true,
            (current, target) => current == target,
        }
    };

    let (status_text, led_class) = {
        let list = servers.read();
        let active: Vec<&ServerConfig> = list.iter().filter(|s| s.active).collect();
        if offline() {
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

    let mut groups: [(Option<&str>, Vec<(&str, &str, &str, AppView)>); 3] = [
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
                ("Favorites", "heart", "nav-favorites", AppView::FavoritesView {}),
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
    // Radio stations live on Subsonic servers; with only Jellyfin ones there's nothing to show.
    if !radio_available(&servers.read()) {
        groups[2].1.retain(|item| item.2 != "nav-radio");
    }
    let current_playlist = match &view {
        AppView::PlaylistDetailView { playlist_id, server_id } => Some((playlist_id.clone(), server_id.clone())),
        _ => None,
    };
    let open_playlist = move |target: AppView| {
        let mut song_details = song_details.clone();
        song_details.close();
        navigation.navigate_with(target, PageMotion::Tab);
    };

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
                SidebarPlaylists { current: current_playlist, on_open: open_playlist }
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
