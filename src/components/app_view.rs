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
}

pub fn view_label(view: &AppView) -> &'static str {
    match view {
        AppView::HomeView {} => "Home",
        AppView::SearchView {} => "Search",
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
    }
}

pub fn view_instance_key(view: &AppView) -> String {
    match view {
        AppView::HomeView {} => "home".to_string(),
        AppView::SearchView {} => "search".to_string(),
        AppView::SongsView {} => "songs".to_string(),
        AppView::Albums {} => "albums".to_string(),
        AppView::AlbumsWithGenre { genre } => format!("albums:{genre}"),
        AppView::ArtistsView {} => "artists".to_string(),
        AppView::PlaylistsView {} => "playlists".to_string(),
        AppView::RadioView {} => "radio".to_string(),
        AppView::BookmarksView {} => "bookmarks".to_string(),
        AppView::FavoritesView {} => "favorites".to_string(),
        AppView::DownloadsView {} => "downloads".to_string(),
        AppView::RandomView {} => "random".to_string(),
        AppView::SettingsView {} => "settings".to_string(),
        AppView::StatsView {} => "stats".to_string(),
        AppView::QueueView {} => "queue".to_string(),
        AppView::AlbumDetailView {
            album_id,
            server_id,
        } => format!("album:{server_id}:{album_id}"),
        AppView::ArtistDetailView {
            artist_id,
            server_id,
        } => format!("artist:{server_id}:{artist_id}"),
        AppView::PlaylistDetailView {
            playlist_id,
            server_id,
        } => format!("playlist:{server_id}:{playlist_id}"),
    }
}
