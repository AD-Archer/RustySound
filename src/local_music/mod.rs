//! Music stored on the device, beside the servers' ("On This Device").
//!
//! Local songs are ordinary `Song`s with `server_id = "local"`, so lists,
//! the queue and Now Playing treat them like any other source.
//! * `model`: ids, the tag record → `Song` mapping, playable formats and
//!   de-duplication against server copies (pure, every target).
//! * `index` + `scan`: the SQLite index and the lofty scanner (Mac and iPhone).
//! * `library`: queries off the main thread and the id → file lookups; its
//!   `LocalLibrary` is the `Local` variant of `api::MusicClient`.
//! * `host`: the app-level worker that scans, watches folders (Mac) or the
//!   Documents folder (iPhone), and publishes [`STATUS`].
//!
//! The web build (and Windows and Linux for now) has no local music: the
//! same functions exist and report nothing.

// Without local music the scanner half is compiled (and tested) but unused.
#![cfg_attr(
    not(any(all(feature = "desktop", target_os = "macos"), all(feature = "mobile", target_os = "ios"))),
    allow(dead_code, unused_imports)
)]

mod model;

pub use model::{
    dedupe_albums, dedupe_artists, dedupe_search, dedupe_songs, is_local, is_local_song,
    is_playable, LOCAL_SOURCE_ID, LOCAL_SOURCE_NAME, NOT_PLAYABLE,
};
/// For the Mac's audio route, which only takes ids the scanner made.
#[cfg_attr(not(all(feature = "desktop", target_os = "macos")), allow(unused_imports))]
pub use model::is_local_id;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod index;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod library;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod scan;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use library::LocalLibrary;

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
mod library_stub;
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
pub use library_stub::LocalLibrary;

#[cfg(any(all(feature = "desktop", target_os = "macos"), all(feature = "mobile", target_os = "ios")))]
mod host;

use crate::api::{ServerConfig, Song};
use dioxus::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Whether this build plays music from the device: the Mac app and the iPhone app.
pub const SUPPORTED: bool = cfg!(any(
    all(feature = "desktop", target_os = "macos"),
    all(feature = "mobile", target_os = "ios")
));

/// A folder the library reads from.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FolderInfo {
    pub id: String,
    pub path: String,
    /// The iPhone's Documents folder is built in and can't be removed.
    pub removable: bool,
}

/// What Settings and the library views show about the local library.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LibraryStatus {
    /// Bumped whenever the index changed; library views refetch on it.
    pub generation: u64,
    pub songs: usize,
    pub folders: Vec<FolderInfo>,
    /// `(read, to read)` while a scan reads changed files.
    pub scanning: Option<(usize, usize)>,
}

pub static STATUS: GlobalSignal<LibraryStatus> = Signal::global(LibraryStatus::default);

/// Song count for code outside rendering (no signal read).
static SONG_COUNT: AtomicUsize = AtomicUsize::new(0);

/// The index generation. Reading it inside an effect, memo or resource
/// re-runs that when local music changes.
pub fn generation() -> u64 {
    STATUS.read().generation
}

/// Whether the device has music, read reactively (render, effects).
pub fn has_music() -> bool {
    SUPPORTED && STATUS.read().songs > 0
}

/// Whether the device has music, without subscribing (fetch code).
pub fn has_music_now() -> bool {
    SUPPORTED && SONG_COUNT.load(Ordering::Relaxed) > 0
}

/// "On This Device" as a server entry, for code keyed on a `ServerConfig`
/// (the lyrics query). It has no address and is never saved or listed.
pub fn source_config() -> ServerConfig {
    ServerConfig {
        id: LOCAL_SOURCE_ID.to_string(),
        name: LOCAL_SOURCE_NAME.to_string(),
        url: String::new(),
        username: String::new(),
        password: String::new(),
        active: true,
    }
}

/// The URL the player loads for a local song. Mac: the offline-audio route
/// (an opaque id, never a path). iPhone: the file, which AVPlayer opens.
pub fn stream_url(song: &Song) -> Option<String> {
    if !SUPPORTED || !is_local_song(song) || !is_playable(song) {
        return None;
    }
    #[cfg(all(feature = "desktop", target_os = "macos"))]
    {
        return crate::offline_audio::protocol::url_for_local_song(&song.id);
    }
    #[cfg(all(feature = "mobile", target_os = "ios"))]
    {
        return song_file(&song.id).map(|path| format!("file://{}", path.display()));
    }
    #[allow(unreachable_code)]
    None
}

/// `<img src>` for a local cover (an art id from the scanner). Native
/// builds serve it from the cover cache route; it's rebuilt from the file
/// if the cache was cleared.
pub fn cover_art_src(art_id: &str) -> Option<String> {
    #[cfg(all(any(target_os = "macos", target_os = "ios"), any(feature = "desktop", feature = "mobile")))]
    {
        if model::is_art_id(art_id) {
            return Some(crate::offline_art::cover_art_display_url(LOCAL_SOURCE_ID, art_id, 0, ""));
        }
    }
    let _ = art_id;
    None
}

/// A local cover as a data URL (system Now Playing artwork and the like).
pub fn cover_art_data_url(art_id: &str) -> Option<String> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        if model::is_art_id(art_id) {
            return crate::offline_art::cached_cover_art_data_url(LOCAL_SOURCE_ID, art_id, 0);
        }
    }
    let _ = art_id;
    None
}

/// Rebuilds a local cover for the cover route after a cache miss.
/// `name` is the requested cache file name; `None` when it isn't local.
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub fn rebuild_cover(name: &str, cache_file: &std::path::Path) -> Option<Vec<u8>> {
    let art_id = name.strip_prefix("local__")?.strip_suffix("__0.img")?;
    library::rebuild_art(art_id, cache_file)
}

/// The file behind a local song id (the audio route's only way to a path).
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub fn song_file(song_id: &str) -> Option<std::path::PathBuf> {
    library::song_file(song_id)
}

/// Embedded lyrics of a local song (LRC or plain text), if the file had any.
pub fn embedded_lyrics(song_id: &str) -> Option<String> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        return library::embedded_lyrics(song_id);
    }
    #[allow(unreachable_code)]
    {
        let _ = song_id;
        None
    }
}

/// Counts a finished play of a local song (the stand-in for scrobbling).
pub fn record_play(song: &Song) {
    if !SUPPORTED || !is_local_song(song) {
        return;
    }
    let id = song.id.clone();
    spawn(async move {
        let _ = LocalLibrary.record_play(&id).await;
    });
}

// ---- Folder management (Settings) ----------------------------------------------

/// Runs the local library for the app's lifetime: scans on launch, watches
/// folders, rescans on foreground. Call once from the app shell.
pub fn use_local_music_host() {
    #[cfg(any(all(feature = "desktop", target_os = "macos"), all(feature = "mobile", target_os = "ios")))]
    host::use_host();
}

/// Opens the folder picker and adds the chosen folder (Mac).
pub fn pick_and_add_folder() {
    #[cfg(all(feature = "desktop", target_os = "macos"))]
    spawn(async move {
        let picked = rfd::AsyncFileDialog::new()
            .set_title("Add Music Folder")
            .pick_folder()
            .await;
        if let Some(folder) = picked {
            host::send(host::Command::Add(folder.path().to_string_lossy().into_owned()));
        }
    });
}

pub fn remove_folder(folder_id: &str) {
    #[cfg(any(all(feature = "desktop", target_os = "macos"), all(feature = "mobile", target_os = "ios")))]
    host::send(host::Command::Remove(folder_id.to_string()));
    let _ = folder_id;
}

/// Looks for new, changed and removed files in every folder.
pub fn rescan() {
    #[cfg(any(all(feature = "desktop", target_os = "macos"), all(feature = "mobile", target_os = "ios")))]
    host::send(host::Command::ScanAll);
}

/// Whether a row for `id` should fade in: it arrived with the latest scan.
/// Answers true once per id, so a row fades in the first time it's shown.
pub fn take_fresh(id: &str) -> bool {
    #[cfg(any(all(feature = "desktop", target_os = "macos"), all(feature = "mobile", target_os = "ios")))]
    {
        return host::take_fresh(id);
    }
    #[allow(unreachable_code)]
    {
        let _ = id;
        false
    }
}
