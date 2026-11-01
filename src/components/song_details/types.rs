//! Now Playing open/close state, shared through `SongDetailsController`.

use crate::api::Song;
use dioxus::prelude::*;

/// Which lower panel Now Playing shows. `Details` and `Related` both land on
/// the Info panel (Related lives inside Info); `Queue` is Up Next.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SongDetailsTab {
    Details,
    Queue,
    /// Kept for callers; lands on Info, where Related lives.
    #[allow(dead_code)]
    Related,
    Lyrics,
}

/// The three segments on the stage.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Panel {
    Lyrics,
    UpNext,
    Info,
}

impl Panel {
    pub(crate) const ALL: [Panel; 3] = [Panel::Lyrics, Panel::UpNext, Panel::Info];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Panel::Lyrics => "Lyrics",
            Panel::UpNext => "Up Next",
            Panel::Info => "Info",
        }
    }

    pub(crate) fn index(self) -> usize {
        match self {
            Panel::Lyrics => 0,
            Panel::UpNext => 1,
            Panel::Info => 2,
        }
    }

    pub(crate) fn from_tab(tab: SongDetailsTab) -> Self {
        match tab {
            SongDetailsTab::Lyrics => Panel::Lyrics,
            SongDetailsTab::Queue => Panel::UpNext,
            SongDetailsTab::Details | SongDetailsTab::Related => Panel::Info,
        }
    }

    pub(crate) fn tab(self) -> SongDetailsTab {
        match self {
            Panel::Lyrics => SongDetailsTab::Lyrics,
            Panel::UpNext => SongDetailsTab::Queue,
            Panel::Info => SongDetailsTab::Details,
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct SongDetailsState {
    pub is_open: bool,
    /// The song Now Playing was opened for. While something is playing the
    /// sheet shows the playing song; this is the fallback when nothing is.
    pub song: Option<Song>,
    pub active_tab: SongDetailsTab,
    /// True once a caller or the user picked a panel. While false the sheet
    /// picks: Lyrics when the song has lyrics, otherwise Up Next.
    pub tab_chosen: bool,
    /// Ambient mode, shown over Now Playing (`components/ambient`).
    pub ambient: bool,
}

impl Default for SongDetailsState {
    fn default() -> Self {
        Self {
            is_open: false,
            song: None,
            active_tab: SongDetailsTab::Lyrics,
            tab_chosen: false,
            ambient: false,
        }
    }
}

/// Context handle for Now Playing (provided by the shell).
#[derive(Clone, Copy, PartialEq)]
pub struct SongDetailsController {
    state: Signal<SongDetailsState>,
}

impl SongDetailsController {
    pub fn new(state: Signal<SongDetailsState>) -> Self {
        Self { state }
    }

    /// Opens Now Playing. The panel goes back to automatic choice unless a
    /// `set_tab` follows.
    pub fn open(&mut self, song: Song) {
        self.state.with_mut(|state| {
            state.is_open = true;
            state.song = Some(song);
            state.tab_chosen = false;
        });
    }

    pub fn close(&mut self) {
        if self.state.peek().is_open {
            self.state.with_mut(|state| {
                state.is_open = false;
                state.ambient = false;
            });
        }
    }

    /// Opens or closes ambient mode. It lives over Now Playing, so it only
    /// opens while the sheet is open.
    pub fn set_ambient(&mut self, on: bool) {
        let on = on && self.state.peek().is_open;
        if self.state.peek().ambient != on {
            self.state.with_mut(|state| state.ambient = on);
        }
    }

    /// Whether ambient mode is showing; subscribes the caller.
    pub fn ambient(&self) -> bool {
        self.state.read().ambient
    }

    pub fn set_tab(&mut self, tab: SongDetailsTab) {
        self.state.with_mut(|state| {
            state.active_tab = tab;
            state.tab_chosen = true;
        });
    }

    /// Current state; subscribes the caller.
    pub fn current(&self) -> SongDetailsState {
        (self.state)()
    }
}

/// Compact Now Playing's focused panel view: the panel takes the screen
/// under a compact header strip. Provided by the overlay; unused on wide.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct NowPlayingFocus(pub Signal<bool>);
