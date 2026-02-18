// Core add-target models and controller state.

const QUICK_PREVIEW_DURATION_MS: u64 = 12000;

#[cfg(not(target_arch = "wasm32"))]
async fn quick_preview_delay_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(target_arch = "wasm32")]
async fn quick_preview_delay_ms(ms: u64) {
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
}

#[derive(Clone)]
#[allow(dead_code)]
pub enum AddTarget {
    Song(Song),
    Songs(Vec<Song>),
    Album {
        album_id: String,
        server_id: String,
        cover_art: Option<String>,
    },
    Playlist {
        playlist_id: String,
        server_id: String,
        cover_art: Option<String>,
    },
}

#[derive(Clone)]
pub struct AddIntent {
    pub target: AddTarget,
    pub label: String,
}

impl AddIntent {
    pub fn from_song(song: Song) -> Self {
        Self {
            label: song.title.clone(),
            target: AddTarget::Song(song),
        }
    }

    #[allow(dead_code)]
    pub fn from_songs(label: String, songs: Vec<Song>) -> Self {
        Self {
            label,
            target: AddTarget::Songs(songs),
        }
    }

    pub fn from_album(album: &Album) -> Self {
        Self {
            label: album.name.clone(),
            target: AddTarget::Album {
                album_id: album.id.clone(),
                server_id: album.server_id.clone(),
                cover_art: album.cover_art.clone(),
            },
        }
    }

    pub fn from_playlist(playlist: &Playlist) -> Self {
        Self {
            label: playlist.name.clone(),
            target: AddTarget::Playlist {
                playlist_id: playlist.id.clone(),
                server_id: playlist.server_id.clone(),
                cover_art: playlist.cover_art.clone(),
            },
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct AddMenuController {
    pub intent: Signal<Option<AddIntent>>,
}

impl AddMenuController {
    pub fn new(intent: Signal<Option<AddIntent>>) -> Self {
        Self { intent }
    }

    pub fn open(&mut self, intent: AddIntent) {
        self.intent.set(Some(intent));
    }

    pub fn close(&mut self) {
        self.intent.set(None);
    }

    pub fn current(&self) -> Option<AddIntent> {
        (self.intent)()
    }
}

#[derive(Clone, PartialEq)]
enum SuggestionDestination {
    Queue,
    Playlist {
        playlist_id: String,
        server_id: String,
    },
}

fn song_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
}
