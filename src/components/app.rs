use crate::api::*;
use crate::components::views::*;
use crate::components::{Player, Sidebar};
use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
pub enum AppView {
    Home,
    Search,
    Albums,
    Artists,
    Playlists,
    Radio,
    Favorites,
    Random,
    Settings,
    AlbumDetail(String, String),    // album_id, server_id
    ArtistDetail(String, String),   // artist_id, server_id
    PlaylistDetail(String, String), // playlist_id, server_id
}

#[component]
pub fn AppShell() -> Element {
    let servers = use_signal(Vec::<ServerConfig>::new);
    let current_view = use_signal(|| AppView::Home);
    let now_playing = use_signal(|| None::<Song>);
    let queue = use_signal(Vec::<Song>::new);
    let queue_index = use_signal(|| 0usize);
    let is_playing = use_signal(|| false);
    let volume = use_signal(|| 0.8f64);

    // Provide state via context
    use_context_provider(|| servers);
    use_context_provider(|| current_view);
    use_context_provider(|| now_playing);
    use_context_provider(|| queue);
    use_context_provider(|| queue_index);
    use_context_provider(|| is_playing);
    use_context_provider(|| volume);

    let view = current_view();

    rsx! {
        div { class: "app-container flex h-screen bg-gradient-to-br from-zinc-950 via-zinc-900 to-zinc-950 text-white overflow-hidden",
            // Sidebar
            Sidebar {}

            // Main content area
            div { class: "flex-1 flex flex-col overflow-hidden",
                // Main scrollable content
                main { class: "flex-1 overflow-y-auto pb-28",
                    div { class: "p-6 lg:p-8",
                        {
                            match view {
                                AppView::Home => rsx! {
                                    HomeView {}
                                },
                                AppView::Search => rsx! {
                                    SearchView {}
                                },
                                AppView::Albums => rsx! {
                                    AlbumsView {}
                                },
                                AppView::Artists => rsx! {
                                    ArtistsView {}
                                },
                                AppView::Playlists => rsx! {
                                    PlaylistsView {}
                                },
                                AppView::Radio => rsx! {
                                    RadioView {}
                                },
                                AppView::Favorites => rsx! {
                                    FavoritesView {}
                                },
                                AppView::Random => rsx! {
                                    RandomView {}
                                },
                                AppView::Settings => rsx! {
                                    SettingsView {}
                                },
                                AppView::AlbumDetail(album_id, server_id) => rsx! {
                                    AlbumDetailView { album_id: album_id.clone(), server_id: server_id.clone() }
                                },
                                AppView::ArtistDetail(artist_id, server_id) => rsx! {
                                    ArtistDetailView { artist_id: artist_id.clone(), server_id: server_id.clone() }
                                },
                                AppView::PlaylistDetail(playlist_id, server_id) => rsx! {
                                    PlaylistDetailView { playlist_id: playlist_id.clone(), server_id: server_id.clone() }
                                },
                            }
                        }
                    }
                }
            }

            // Fixed bottom player
            Player {}
        }
    }
}
