//! Carries out palette rows, shortcuts and menu items. Nothing here plays,
//! navigates or saves on its own: playback and the queue go through
//! `PlayerCtx`, pages through `Navigation`, Now Playing through its
//! controller, settings through `update_settings`, and the album/artist
//! jumps through the track menu's own helpers.

use super::actions::ActionId;
use super::recent::Recent;
use super::rows::{Choice, ChoicePage, Entry};
use super::{CommandPalette, PaletteProfiles};
use crate::api::{MusicClient, Song};
use crate::components::song_details::actions::is_live_song;
use crate::components::ui::{go_to_album, go_to_artist, StatusToast};
use crate::components::{
    set_playback_speed, update_cache_settings, update_settings, AddIntent, AddMenuController,
    AppView, Navigation, PageMotion, PlayerCtx, QueuePosition, SleepTimer, SongDetailsController,
    SongDetailsTab,
};
use dioxus::prelude::*;

/// Seek step for ←/→, in seconds.
const SEEK_STEP: f64 = 10.0;
/// Volume step for ↑/↓.
const VOLUME_STEP: f64 = 0.05;

/// Everything an action may touch, taken from the shell's contexts once.
#[derive(Clone)]
pub(crate) struct Runner {
    player: PlayerCtx,
    navigation: Navigation,
    song_details: SongDetailsController,
    add_menu: AddMenuController,
    sleep: SleepTimer,
    toast: StatusToast,
    palette: CommandPalette,
    profiles: Option<PaletteProfiles>,
}

impl Runner {
    pub(crate) fn from_context() -> Self {
        Self {
            player: PlayerCtx::from_context(),
            navigation: consume_context(),
            song_details: consume_context(),
            add_menu: consume_context(),
            sleep: consume_context(),
            toast: consume_context(),
            palette: consume_context(),
            profiles: try_consume_context(),
        }
    }

    fn song(&self) -> Option<Song> {
        self.player.now_playing.peek().clone()
    }

    /// Closes Now Playing, the add menu and the track menu, so a new page
    /// isn't hidden under them (as the macOS Go menu does).
    fn clear_overlays(&self) {
        let (mut song_details, mut add_menu, mut track_menu) =
            (self.song_details, self.add_menu.clone(), self.player.track_menu);
        song_details.close();
        add_menu.close();
        track_menu.close();
    }

    fn go(&self, view: AppView) {
        self.clear_overlays();
        self.navigation.navigate_with(view, PageMotion::Tab);
    }

    /// Opens Now Playing for the playing song, on `tab` when given.
    fn open_now_playing(&self, tab: Option<SongDetailsTab>) -> bool {
        let Some(song) = self.song() else {
            return false;
        };
        let mut song_details = self.song_details;
        if !song_details.current().is_open {
            song_details.open(song);
        }
        if let Some(tab) = tab {
            song_details.set_tab(tab);
        }
        true
    }

    /// Runs `f` on the shell's scope on the next tick, so what it starts
    /// outlives the palette that asked.
    pub(crate) fn later(&self, f: impl FnOnce() + 'static) {
        self.toast.spawn(async move { f() });
    }

    pub(crate) fn run(&self, id: ActionId) {
        let mut player = self.player;
        match id {
            ActionId::PlayPause => {
                if self.song().is_some() {
                    player.toggle_play();
                }
            }
            ActionId::Next => player.next("shortcut"),
            ActionId::Previous => player.previous("shortcut"),
            ActionId::Shuffle => player.toggle_shuffle(),
            ActionId::Repeat => player.cycle_repeat(),
            ActionId::SeekForward | ActionId::SeekBack => {
                let Some(song) = self.song().filter(|s| !is_live_song(s)) else {
                    return;
                };
                let audio = player.audio.peek();
                let now = *audio.current_time.peek();
                let length = Some(*audio.duration.peek())
                    .filter(|d| d.is_finite() && *d > 0.0)
                    .unwrap_or(song.duration as f64);
                drop(audio);
                let step = if id == ActionId::SeekForward { SEEK_STEP } else { -SEEK_STEP };
                player.seek((now + step).clamp(0.0, (length - 0.5).max(0.0)));
            }
            ActionId::VolumeUp | ActionId::VolumeDown => {
                let step = if id == ActionId::VolumeUp { VOLUME_STEP } else { -VOLUME_STEP };
                let next = *player.volume.peek() + step;
                player.set_volume((next * 100.0).round() / 100.0);
            }
            ActionId::Like => {
                if let Some(song) = self.song().filter(|s| !is_live_song(s)) {
                    player.toggle_favorite(song);
                }
            }
            ActionId::AddToPlaylist => {
                if let Some(song) = self.song() {
                    let mut add_menu = self.add_menu.clone();
                    add_menu.open(AddIntent::from_song(song));
                }
            }
            ActionId::GoToAlbum => {
                if let Some(song) = self.song() {
                    go_to_album(self.navigation, self.song_details, &song);
                }
            }
            ActionId::GoToArtist => {
                if let Some(song) = self.song() {
                    go_to_artist(self.navigation, self.song_details, player.servers, &song);
                }
            }
            ActionId::NowPlaying => {
                self.open_now_playing(None);
            }
            ActionId::Lyrics => {
                self.open_now_playing(Some(SongDetailsTab::Lyrics));
            }
            ActionId::UpNext => {
                self.open_now_playing(Some(SongDetailsTab::Queue));
            }
            ActionId::Ambient => {
                if self.open_now_playing(None) {
                    let mut song_details = self.song_details;
                    song_details.set_ambient(true);
                }
            }
            ActionId::GoHome => self.go(AppView::HomeView {}),
            ActionId::GoSearch => {
                self.go(AppView::SearchView {});
                crate::components::focus_global_search_input();
            }
            ActionId::GoLibrary => self.go(AppView::LibraryView {}),
            ActionId::GoAlbums => self.go(AppView::Albums {}),
            ActionId::GoArtists => self.go(AppView::ArtistsView {}),
            ActionId::GoSongs => self.go(AppView::SongsView {}),
            ActionId::GoPlaylists => self.go(AppView::PlaylistsView {}),
            ActionId::GoFavorites => self.go(AppView::FavoritesView {}),
            ActionId::GoDownloads => self.go(AppView::DownloadsView {}),
            ActionId::GoRadio => self.go(AppView::RadioView {}),
            ActionId::GoStats => self.go(AppView::StatsView {}),
            ActionId::GoSettings => self.go(AppView::SettingsView {}),
            ActionId::Wrapped => {
                self.clear_overlays();
                self.palette.show_wrapped();
            }
            ActionId::ToggleOffline => {
                update_cache_settings(player.settings, |s| s.offline_mode = !s.offline_mode);
            }
            ActionId::SleepTimer
            | ActionId::PlaybackSpeed
            | ActionId::Equalizer
            | ActionId::Theme
            | ActionId::Profile => {
                if let Some(page) = ChoicePage::for_action(id) {
                    self.palette.open_page(page);
                }
            }
            ActionId::Shortcuts => self.palette.toggle_shortcuts(),
            ActionId::Palette => self.palette.toggle(),
            // Every sheet and menu closes itself on Escape.
            ActionId::Close => {}
        }
    }

    pub(crate) fn run_choice(&self, choice: Choice) {
        let settings = self.player.settings;
        match choice {
            Choice::Sleep(choice) => self.sleep.set(choice),
            Choice::Speed(rate) => set_playback_speed(settings, rate),
            Choice::EqOff => {
                update_settings(settings, |s| s.equalizer.enabled = false);
            }
            Choice::Eq(preset) => {
                update_settings(settings, |s| {
                    s.equalizer.set_preset(preset);
                    s.equalizer.enabled = true;
                });
            }
            Choice::Theme(key) => {
                update_settings(settings, |s| s.theme = key.to_string());
            }
            Choice::Appearance(key) => {
                update_settings(settings, |s| s.appearance = key.to_string());
            }
            Choice::Profile(id) => {
                if let Some(profiles) = self.profiles {
                    profiles.switch.call(id);
                }
            }
        }
    }

    /// Runs a palette row: ↩ (`alternate` false) or ⌘↩. Returns what to
    /// remember among the recents.
    pub(crate) fn run_entry(&self, entry: Entry, alternate: bool) -> Option<Recent> {
        let player = self.player;
        match entry {
            Entry::Action(id) => {
                self.run(id);
                Some(Recent::Action { key: id.key().to_string() })
            }
            Entry::Choice(choice) => {
                self.run_choice(choice);
                None
            }
            Entry::Song(song) => {
                if alternate {
                    if player.enqueue(vec![song.clone()], QueuePosition::Next) > 0 {
                        self.toast.show(format!("“{}” plays next", song.title), false);
                    }
                } else if let Err(message) = player.play(vec![song.clone()], 0, None) {
                    self.toast.show(message, true);
                }
                Some(Recent::Song { song })
            }
            Entry::Album(album) => {
                if alternate {
                    let client = MusicClient::for_source(&player.servers.peek(), &album.server_id);
                    let (toast, name, id) = (self.toast, album.name.clone(), album.id.clone());
                    if let Some(client) = client {
                        toast.spawn(async move {
                            if let Ok((_, songs)) = client.get_album(&id).await {
                                if player.enqueue(songs, QueuePosition::Next) > 0 {
                                    toast.show(format!("“{name}” plays next"), false);
                                }
                            }
                        });
                    }
                } else {
                    self.clear_overlays();
                    self.navigation.navigate_to(AppView::AlbumDetailView {
                        album_id: album.id.clone(),
                        server_id: album.server_id.clone(),
                    });
                }
                Some(Recent::Album { album })
            }
            Entry::Artist(artist) => {
                self.clear_overlays();
                self.navigation.navigate_to(AppView::ArtistDetailView {
                    artist_id: artist.id.clone(),
                    server_id: artist.server_id.clone(),
                });
                Some(Recent::Artist { artist })
            }
            Entry::Playlist(playlist) => {
                if alternate {
                    let client = MusicClient::for_source(&player.servers.peek(), &playlist.server_id);
                    let (toast, name, id) = (self.toast, playlist.name.clone(), playlist.id.clone());
                    if let Some(client) = client {
                        toast.spawn(async move {
                            if let Ok((_, songs)) = client.get_playlist(&id).await {
                                if player.enqueue(songs, QueuePosition::Next) > 0 {
                                    toast.show(format!("“{name}” plays next"), false);
                                }
                            }
                        });
                    }
                } else {
                    self.clear_overlays();
                    self.navigation.navigate_to(AppView::PlaylistDetailView {
                        playlist_id: playlist.id.clone(),
                        server_id: playlist.server_id.clone(),
                    });
                }
                Some(Recent::Playlist { playlist })
            }
        }
    }
}
