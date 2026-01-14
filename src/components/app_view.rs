//! Defines the shared application view state.

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
    Stats,
    Queue,
    AlbumDetail(String, String),
    ArtistDetail(String, String),
    PlaylistDetail(String, String),
}

pub fn view_label(view: &AppView) -> &'static str {
    match view {
        AppView::Home => "Home",
        AppView::Search => "Search",
        AppView::Albums => "Albums",
        AppView::Artists => "Artists",
        AppView::Playlists => "Playlists",
        AppView::Radio => "Radio",
        AppView::Favorites => "Favorites",
        AppView::Random => "Random",
        AppView::Settings => "Settings",
        AppView::Stats => "Stats",
        AppView::Queue => "Queue",
        AppView::AlbumDetail(_, _) => "Album",
        AppView::ArtistDetail(_, _) => "Artist",
        AppView::PlaylistDetail(_, _) => "Playlist",
    }
}
