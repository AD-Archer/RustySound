//! The one action list: everything the palette offers and every keyboard
//! shortcut runs. [`actions`] builds it from what the app can do right now
//! ([`Caps`]); `run.rs` carries each action out through the app's existing
//! code paths. Pure data, so it is unit-tested without a UI.

use crate::db::RepeatMode;

/// Every action, palette row or shortcut. `key` names it in the shortcut
/// listener, the macOS menu ids and the recents file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionId {
    // Playback
    PlayPause,
    Next,
    Previous,
    Shuffle,
    Repeat,
    SeekForward,
    SeekBack,
    VolumeUp,
    VolumeDown,
    // The playing song
    Like,
    AddToPlaylist,
    GoToAlbum,
    GoToArtist,
    NowPlaying,
    Lyrics,
    UpNext,
    Ambient,
    SleepTimer,
    PlaybackSpeed,
    Equalizer,
    // Pages
    GoHome,
    GoSearch,
    GoLibrary,
    GoAlbums,
    GoArtists,
    GoSongs,
    GoPlaylists,
    GoFavorites,
    GoDownloads,
    GoRadio,
    GoStats,
    GoSettings,
    Wrapped,
    // App
    ToggleOffline,
    Theme,
    Profile,
    Shortcuts,
    Palette,
    /// Escape: closes the top sheet, menu or overlay (their own handlers).
    Close,
}

// `ALL` and `from_key` serve the macOS menu ids (and the tests).
#[cfg_attr(not(all(feature = "desktop", target_os = "macos")), allow(dead_code))]
impl ActionId {
    pub const ALL: [ActionId; 39] = [
        ActionId::PlayPause,
        ActionId::Next,
        ActionId::Previous,
        ActionId::Shuffle,
        ActionId::Repeat,
        ActionId::SeekForward,
        ActionId::SeekBack,
        ActionId::VolumeUp,
        ActionId::VolumeDown,
        ActionId::Like,
        ActionId::AddToPlaylist,
        ActionId::GoToAlbum,
        ActionId::GoToArtist,
        ActionId::NowPlaying,
        ActionId::Lyrics,
        ActionId::UpNext,
        ActionId::Ambient,
        ActionId::SleepTimer,
        ActionId::PlaybackSpeed,
        ActionId::Equalizer,
        ActionId::GoHome,
        ActionId::GoSearch,
        ActionId::GoLibrary,
        ActionId::GoAlbums,
        ActionId::GoArtists,
        ActionId::GoSongs,
        ActionId::GoPlaylists,
        ActionId::GoFavorites,
        ActionId::GoDownloads,
        ActionId::GoRadio,
        ActionId::GoStats,
        ActionId::GoSettings,
        ActionId::Wrapped,
        ActionId::ToggleOffline,
        ActionId::Theme,
        ActionId::Profile,
        ActionId::Shortcuts,
        ActionId::Palette,
        ActionId::Close,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ActionId::PlayPause => "play-pause",
            ActionId::Next => "next",
            ActionId::Previous => "previous",
            ActionId::Shuffle => "shuffle",
            ActionId::Repeat => "repeat",
            ActionId::SeekForward => "seek-forward",
            ActionId::SeekBack => "seek-back",
            ActionId::VolumeUp => "volume-up",
            ActionId::VolumeDown => "volume-down",
            ActionId::Like => "like",
            ActionId::AddToPlaylist => "add-to-playlist",
            ActionId::GoToAlbum => "go-album",
            ActionId::GoToArtist => "go-artist",
            ActionId::NowPlaying => "now-playing",
            ActionId::Lyrics => "lyrics",
            ActionId::UpNext => "up-next",
            ActionId::Ambient => "ambient",
            ActionId::SleepTimer => "sleep-timer",
            ActionId::PlaybackSpeed => "speed",
            ActionId::Equalizer => "equalizer",
            ActionId::GoHome => "go-home",
            ActionId::GoSearch => "go-search",
            ActionId::GoLibrary => "go-library",
            ActionId::GoAlbums => "go-albums",
            ActionId::GoArtists => "go-artists",
            ActionId::GoSongs => "go-songs",
            ActionId::GoPlaylists => "go-playlists",
            ActionId::GoFavorites => "go-favorites",
            ActionId::GoDownloads => "go-downloads",
            ActionId::GoRadio => "go-radio",
            ActionId::GoStats => "go-stats",
            ActionId::GoSettings => "go-settings",
            ActionId::Wrapped => "wrapped",
            ActionId::ToggleOffline => "offline",
            ActionId::Theme => "theme",
            ActionId::Profile => "profile",
            ActionId::Shortcuts => "shortcuts",
            ActionId::Palette => "palette",
            ActionId::Close => "close",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.key() == key)
    }

    /// The name the cheat sheet and menus use, whatever the state.
    pub fn name(self) -> &'static str {
        match self {
            ActionId::PlayPause => "Play/Pause",
            ActionId::Next => "Next",
            ActionId::Previous => "Previous",
            ActionId::Shuffle => "Shuffle",
            ActionId::Repeat => "Repeat",
            ActionId::SeekForward => "Forward 10 Seconds",
            ActionId::SeekBack => "Back 10 Seconds",
            ActionId::VolumeUp => "Volume Up",
            ActionId::VolumeDown => "Volume Down",
            ActionId::Like => "Like",
            ActionId::AddToPlaylist => "Add to Playlist…",
            ActionId::GoToAlbum => "Go to Album",
            ActionId::GoToArtist => "Go to Artist",
            ActionId::NowPlaying => "Now Playing",
            ActionId::Lyrics => "Lyrics",
            ActionId::UpNext => "Up Next",
            ActionId::Ambient => "Ambient Mode",
            ActionId::SleepTimer => "Sleep Timer…",
            ActionId::PlaybackSpeed => "Playback Speed…",
            ActionId::Equalizer => "Equalizer Preset…",
            ActionId::GoHome => "Home",
            ActionId::GoSearch => "Search",
            ActionId::GoLibrary => "Library",
            ActionId::GoAlbums => "Albums",
            ActionId::GoArtists => "Artists",
            ActionId::GoSongs => "Songs",
            ActionId::GoPlaylists => "Playlists",
            ActionId::GoFavorites => "Favorites",
            ActionId::GoDownloads => "Downloads",
            ActionId::GoRadio => "Radio",
            ActionId::GoStats => "Stats",
            ActionId::GoSettings => "Settings",
            ActionId::Wrapped => "Wrapped",
            ActionId::ToggleOffline => "Offline Mode",
            ActionId::Theme => "Switch Theme…",
            ActionId::Profile => "Switch Profile…",
            ActionId::Shortcuts => "Keyboard Shortcuts",
            ActionId::Palette => "Command Palette",
            ActionId::Close => "Close",
        }
    }

    /// Extra words a query may match (no highlight): synonyms and the
    /// stable name of a row whose title follows the state.
    fn keywords(self) -> &'static str {
        match self {
            ActionId::PlayPause => "play pause resume stop toggle",
            ActionId::Next => "skip forward track",
            ActionId::Previous => "back previous track restart",
            ActionId::Shuffle => "shuffle random toggle",
            ActionId::Repeat => "repeat loop",
            ActionId::Like => "like love favorite favourite heart star",
            ActionId::AddToPlaylist => "add playlist",
            ActionId::NowPlaying => "now playing player open",
            ActionId::Lyrics => "lyrics words karaoke",
            ActionId::UpNext => "queue up next",
            ActionId::Ambient => "ambient fullscreen screensaver",
            ActionId::SleepTimer => "sleep timer stop",
            ActionId::PlaybackSpeed => "speed rate tempo",
            ActionId::Equalizer => "equalizer eq bass treble",
            ActionId::GoHome => "go discover",
            ActionId::GoSearch => "go find",
            ActionId::GoFavorites => "go liked starred",
            ActionId::GoStats => "go statistics history",
            ActionId::GoSettings => "go preferences options",
            ActionId::Wrapped => "year review",
            ActionId::ToggleOffline => "offline online network",
            ActionId::Theme => "theme appearance dark light colour color",
            ActionId::Profile => "profile account user",
            ActionId::Shortcuts => "keyboard shortcuts keys help",
            _ => "go to",
        }
    }
}

/// The groups the empty palette lists actions in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Playback,
    Song,
    Pages,
    App,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Playback => "Playback",
            Group::Song => "Playing Song",
            Group::Pages => "Go To",
            Group::App => "App",
        }
    }
}

/// What the playing song allows.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct SongCaps {
    /// Internet radio: no next/previous, speed, like or lyrics.
    pub live: bool,
    pub liked: bool,
    pub album: bool,
    pub artist: bool,
    /// Its server edits playlists.
    pub playlist_edits: bool,
}

/// What the app can do right now; the action list follows it.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Caps {
    /// The playing (or last) song, if any.
    pub song: Option<SongCaps>,
    pub playing: bool,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub offline: bool,
    /// This build's player has an equalizer.
    pub equalizer: bool,
    /// A server keeps internet radio stations.
    pub radio: bool,
    /// Profiles exist (provided by the profiles feature, see `PaletteProfiles`).
    pub profiles: bool,
}

/// One palette action row.
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub id: ActionId,
    /// The title, following the state ("Pause" while playing).
    pub title: String,
    pub keywords: &'static str,
    pub icon: &'static str,
    pub group: Group,
    /// Opens a list of choices inside the palette instead of running.
    pub choices: bool,
}

fn action(id: ActionId, title: impl Into<String>, icon: &'static str, group: Group) -> Action {
    Action {
        id,
        title: title.into(),
        keywords: id.keywords(),
        icon,
        group,
        choices: matches!(
            id,
            ActionId::SleepTimer | ActionId::PlaybackSpeed | ActionId::Equalizer | ActionId::Theme | ActionId::Profile
        ),
    }
}

/// The palette's actions for `caps`, in listing order.
pub fn actions(caps: &Caps) -> Vec<Action> {
    use ActionId::*;
    use Group::*;
    let mut list = Vec::with_capacity(40);
    let song = caps.song;
    let live = song.is_some_and(|s| s.live);

    if song.is_some() {
        list.push(action(PlayPause, if caps.playing { "Pause" } else { "Play" }, if caps.playing { "pause" } else { "play" }, Playback));
        if !live {
            list.push(action(Next, "Next", "next", Playback));
            list.push(action(Previous, "Previous", "prev", Playback));
        }
    }
    list.push(action(Shuffle, if caps.shuffle { "Turn Shuffle Off" } else { "Turn Shuffle On" }, "shuffle", Playback));
    let (repeat_title, repeat_icon) = match caps.repeat {
        RepeatMode::Off => ("Repeat All", "repeat"),
        RepeatMode::All => ("Repeat One", "repeat-1"),
        RepeatMode::One => ("Repeat Off", "repeat"),
    };
    list.push(action(Repeat, repeat_title, repeat_icon, Playback));
    list.push(action(SleepTimer, "Sleep Timer…", "moon", Playback));
    if !live {
        list.push(action(PlaybackSpeed, "Playback Speed…", "speed", Playback));
    }
    if caps.equalizer {
        list.push(action(Equalizer, "Equalizer Preset…", "sliders", Playback));
    }

    if let Some(song) = song {
        list.push(action(NowPlaying, "Open Now Playing", "maximize", Song));
        if !song.live {
            list.push(action(Like, if song.liked { "Unlike Song" } else { "Like Song" }, if song.liked { "heart-filled" } else { "heart" }, Song));
            if song.playlist_edits {
                list.push(action(AddToPlaylist, "Add Song to Playlist…", "plus", Song));
            }
            if song.album {
                list.push(action(GoToAlbum, "Go to Album", "album", Song));
            }
            if song.artist {
                list.push(action(GoToArtist, "Go to Artist", "artist", Song));
            }
            list.push(action(Lyrics, "Lyrics", "lyrics", Song));
        }
        list.push(action(UpNext, "Up Next", "add-to-queue", Song));
        list.push(action(Ambient, "Ambient Mode", "maximize", Song));
    }

    for (id, icon) in [
        (GoHome, "home"),
        (GoSearch, "search"),
        (GoLibrary, "library"),
        (GoAlbums, "album"),
        (GoArtists, "artist"),
        (GoSongs, "music"),
        (GoPlaylists, "playlist"),
        (GoFavorites, "heart"),
        (GoDownloads, "download"),
        (GoRadio, "radio"),
        (GoStats, "stats"),
        (GoSettings, "settings"),
        (Wrapped, "star"),
    ] {
        if id == GoRadio && !caps.radio {
            continue;
        }
        list.push(action(id, id.name(), icon, Pages));
    }

    list.push(action(ToggleOffline, if caps.offline { "Go Online" } else { "Go Offline" }, "offline", App));
    list.push(action(Theme, "Switch Theme…", "swatch", App));
    if caps.profiles {
        list.push(action(Profile, "Switch Profile…", "headphones", App));
    }
    list.push(action(Shortcuts, "Keyboard Shortcuts", "zap", App));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(caps: &Caps) -> Vec<ActionId> {
        actions(caps).into_iter().map(|a| a.id).collect()
    }

    #[test]
    fn keys_round_trip_and_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for id in ActionId::ALL {
            assert_eq!(ActionId::from_key(id.key()), Some(id));
            seen.insert(id.key());
        }
        // Every variant once.
        assert_eq!(seen.len(), ActionId::ALL.len());
    }

    #[test]
    fn nothing_playing_hides_song_actions() {
        let list = ids(&Caps::default());
        for id in [ActionId::PlayPause, ActionId::Next, ActionId::Like, ActionId::Lyrics, ActionId::GoToAlbum, ActionId::NowPlaying] {
            assert!(!list.contains(&id), "{id:?}");
        }
        for id in [ActionId::GoHome, ActionId::GoSettings, ActionId::Wrapped, ActionId::Shuffle, ActionId::Theme, ActionId::Shortcuts] {
            assert!(list.contains(&id), "{id:?}");
        }
    }

    #[test]
    fn a_song_brings_its_actions_and_titles_follow_state() {
        let caps = Caps {
            song: Some(SongCaps { liked: true, album: true, artist: true, playlist_edits: true, live: false }),
            playing: true,
            shuffle: true,
            repeat: RepeatMode::All,
            equalizer: true,
            ..Caps::default()
        };
        let list = actions(&caps);
        let title = |id| list.iter().find(|a| a.id == id).map(|a| a.title.clone()).unwrap();
        assert_eq!(title(ActionId::PlayPause), "Pause");
        assert_eq!(title(ActionId::Shuffle), "Turn Shuffle Off");
        assert_eq!(title(ActionId::Repeat), "Repeat One");
        assert_eq!(title(ActionId::Like), "Unlike Song");
        for id in [ActionId::Next, ActionId::AddToPlaylist, ActionId::GoToAlbum, ActionId::GoToArtist, ActionId::Equalizer, ActionId::PlaybackSpeed] {
            assert!(list.iter().any(|a| a.id == id), "{id:?}");
        }
        assert!(list.iter().find(|a| a.id == ActionId::SleepTimer).unwrap().choices);
    }

    #[test]
    fn radio_and_capabilities_trim_the_list() {
        let caps = Caps {
            song: Some(SongCaps { live: true, ..SongCaps::default() }),
            ..Caps::default()
        };
        let list = ids(&caps);
        for id in [ActionId::Next, ActionId::Previous, ActionId::PlaybackSpeed, ActionId::Like, ActionId::Lyrics, ActionId::GoRadio, ActionId::Equalizer, ActionId::Profile] {
            assert!(!list.contains(&id), "{id:?}");
        }
        assert!(list.contains(&ActionId::PlayPause));
        let more = ids(&Caps { radio: true, profiles: true, ..Caps::default() });
        assert!(more.contains(&ActionId::GoRadio));
        assert!(more.contains(&ActionId::Profile));
    }
}
