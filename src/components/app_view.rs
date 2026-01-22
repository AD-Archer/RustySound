//! Defines the shared application view state.

#[derive(Clone, PartialEq)]
#[allow(dead_code)]
pub enum AppView {
    Home,
    Search,
    Songs,
    Albums(Option<String>),
    Artists,
    Playlists,
    Radio,
    Bookmarks,
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
        AppView::Songs => "Songs",
        AppView::Albums(genre) => {
            if let Some(ref _genre_name) = genre {
                // We can't return a reference to the String, so we return a static string
                // The actual title will be handled in the component
                "Albums"
            } else {
                "Albums"
            }
        }
        AppView::Artists => "Artists",
        AppView::Playlists => "Playlists",
        AppView::Radio => "Radio",
        AppView::Bookmarks => "Bookmarks",
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
