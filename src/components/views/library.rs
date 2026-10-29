//! Library tab: the grouped list of library areas plus "Recently Added".
//! Minimal phase-1 version built from the kit; phase 2 (agent E) owns it.

use crate::api::{Album, NavidromeClient, ServerConfig};
use crate::components::ui::{Artwork, InsetList, ListInset, ListRow, SectionHeader, Skeleton};
use crate::components::{AppView, HomeFeedState, Navigation};
use dioxus::prelude::*;

const RECENT_LIMIT: usize = 8;

#[component]
pub fn LibraryView() -> Element {
    let navigation = use_context::<Navigation>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let home_feed = use_context::<HomeFeedState>();

    // Prefer the albums the shell already cached; fetch only when missing.
    let recent = use_resource(move || {
        let cached = (home_feed.recent_albums)();
        let active: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
        async move {
            if let Some(albums) = cached.filter(|list| !list.is_empty()) {
                return albums.into_iter().take(RECENT_LIMIT).collect::<Vec<Album>>();
            }
            let mut albums = Vec::new();
            for server in active {
                let client = NavidromeClient::new(server);
                if let Ok(mut list) = client.get_albums("newest", RECENT_LIMIT as u32, 0).await {
                    albums.append(&mut list);
                }
                if albums.len() >= RECENT_LIMIT {
                    break;
                }
            }
            albums.truncate(RECENT_LIMIT);
            albums
        }
    });

    let go = move |target: AppView| move |_| navigation.navigate_to(target.clone());
    let has_servers = !servers.read().is_empty();

    let areas: [(&str, &str, &str, AppView); 6] = [
        ("Playlists", "playlist", "library-playlists", AppView::PlaylistsView {}),
        ("Artists", "artist", "library-artists", AppView::ArtistsView {}),
        ("Albums", "album", "library-albums", AppView::Albums {}),
        ("Songs", "music", "library-songs", AppView::SongsView {}),
        ("Favorites", "star", "library-favorites", AppView::FavoritesView {}),
        ("Downloads", "download", "library-downloads", AppView::DownloadsView {}),
    ];
    let more: [(&str, &str, &str, AppView); 5] = [
        ("Radio", "radio", "library-radio", AppView::RadioView {}),
        ("Random Mix", "shuffle", "library-random", AppView::RandomView {}),
        ("Bookmarks", "bookmark", "library-bookmarks", AppView::BookmarksView {}),
        ("Up Next", "queue", "library-queue", AppView::QueueView {}),
        ("Stats", "stats", "library-stats", AppView::StatsView {}),
    ];

    rsx! {
        div { class: "rs-view rs-view--narrow", "data-testid": "library",
            div { class: "rs-page-head",
                h1 { class: "rs-large-title", "Library" }
            }

            InsetList { inset: ListInset::Icon, label: "Library".to_string(),
                for (label, icon, testid, target) in areas {
                    ListRow {
                        key: "{testid}",
                        title: label.to_string(),
                        icon: icon.to_string(),
                        chevron: true,
                        compact: true,
                        testid: testid.to_string(),
                        onclick: go(target),
                    }
                }
            }

            InsetList { inset: ListInset::Icon, label: "More".to_string(),
                for (label, icon, testid, target) in more {
                    ListRow {
                        key: "{testid}",
                        title: label.to_string(),
                        icon: icon.to_string(),
                        chevron: true,
                        compact: true,
                        testid: testid.to_string(),
                        onclick: go(target),
                    }
                }
            }

            InsetList { inset: ListInset::Icon, label: "Settings".to_string(),
                ListRow {
                    title: "Settings".to_string(),
                    icon: "settings".to_string(),
                    chevron: true,
                    compact: true,
                    testid: "settings-open".to_string(),
                    onclick: go(AppView::SettingsView {}),
                }
            }

            if has_servers {
                SectionHeader {
                    title: "Recently Added".to_string(),
                    action: "See All".to_string(),
                    testid: "library-recent-all".to_string(),
                    on_action: go(AppView::Albums {}),
                }
                match recent() {
                    None => rsx! { Skeleton { rows: 4 } },
                    Some(albums) if albums.is_empty() => rsx! {
                        p { class: "rs-list-foot", "Nothing new on your server yet." }
                    },
                    Some(albums) => rsx! {
                        InsetList { label: "Recently Added".to_string(),
                            for album in albums {
                                ListRow {
                                    key: "{album.server_id}:{album.id}",
                                    title: album.name.clone(),
                                    subtitle: album.artist.clone(),
                                    detail: album.year.map(|y| y.to_string()),
                                    leading: rsx! {
                                        Artwork {
                                            cover_art: album.cover_art.clone(),
                                            server_id: album.server_id.clone(),
                                            size_px: 44,
                                            placeholder: "album".to_string(),
                                        }
                                    },
                                    onclick: go(AppView::AlbumDetailView {
                                        album_id: album.id.clone(),
                                        server_id: album.server_id.clone(),
                                    }),
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}
