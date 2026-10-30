//! Defines the shared application view state.

use crate::components::views::*;
use crate::components::AppShell;
use dioxus::prelude::*;

#[derive(Routable, Clone, PartialEq)]
#[allow(dead_code)]
pub enum AppView {
    #[layout(AppShell)]
    #[route("/")]
    HomeView {},
    #[route("/search")]
    SearchView {},
    #[route("/library")]
    LibraryView {},
    #[route("/songs")]
    SongsView {},
    #[route("/albums")]
    Albums {},
    #[route("/albums/:genre")]
    AlbumsWithGenre { genre: String },
    #[route("/artists")]
    ArtistsView {},
    #[route("/playlists")]
    PlaylistsView {},
    #[route("/radio")]
    RadioView {},
    #[route("/bookmarks")]
    BookmarksView {},
    #[route("/favorites")]
    FavoritesView {},
    #[route("/downloads")]
    DownloadsView {},
    #[route("/random")]
    RandomView {},
    #[route("/settings")]
    SettingsView {},
    #[route("/stats")]
    StatsView {},
    #[route("/queue")]
    QueueView {},
    #[route("/album/:album_id/:server_id")]
    AlbumDetailView { album_id: String, server_id: String },
    #[route("/artist/:artist_id/:server_id")]
    ArtistDetailView {
        artist_id: String,
        server_id: String,
    },
    #[route("/playlist/:playlist_id/:server_id")]
    PlaylistDetailView {
        playlist_id: String,
        server_id: String,
    },
    /// A generated mix; `mix_id` is its slug (`api::discovery::mix_slug`).
    #[route("/mix/:mix_id")]
    MixDetailView { mix_id: String },
}

#[allow(dead_code)]
pub fn view_label(view: &AppView) -> &'static str {
    match view {
        AppView::HomeView {} => "Home",
        AppView::SearchView {} => "Search",
        AppView::LibraryView {} => "Library",
        AppView::SongsView {} => "Songs",
        AppView::Albums {} => "Albums",
        AppView::AlbumsWithGenre { .. } => "Albums",
        AppView::ArtistsView {} => "Artists",
        AppView::PlaylistsView {} => "Playlists",
        AppView::RadioView {} => "Radio",
        AppView::BookmarksView {} => "Bookmarks",
        AppView::FavoritesView {} => "Favorites",
        AppView::DownloadsView {} => "Downloads",
        AppView::RandomView {} => "Random",
        AppView::SettingsView {} => "Settings",
        AppView::StatsView {} => "Stats",
        AppView::QueueView {} => "Queue",
        AppView::AlbumDetailView { .. } => "Album",
        AppView::ArtistDetailView { .. } => "Artist",
        AppView::PlaylistDetailView { .. } => "Playlist",
        AppView::MixDetailView { .. } => "Mix",
    }
}
