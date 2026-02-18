// Song details state machine and tab metadata.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SongDetailsTab {
    Details,
    Queue,
    Related,
    Lyrics,
}

impl SongDetailsTab {
    fn label(self) -> &'static str {
        match self {
            Self::Details => "Details",
            Self::Queue => "Up Next",
            Self::Related => "Related",
            Self::Lyrics => "Lyrics",
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct SongDetailsState {
    pub is_open: bool,
    pub song: Option<Song>,
    pub active_tab: SongDetailsTab,
}

impl Default for SongDetailsState {
    fn default() -> Self {
        Self {
            is_open: false,
            song: None,
            active_tab: SongDetailsTab::Details,
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct SongDetailsController {
    state: Signal<SongDetailsState>,
}

impl SongDetailsController {
    pub fn new(state: Signal<SongDetailsState>) -> Self {
        Self { state }
    }

    pub fn open(&mut self, song: Song) {
        self.state.with_mut(|state| {
            state.is_open = true;
            state.song = Some(song);
        });
    }

    pub fn close(&mut self) {
        self.state.with_mut(|state| {
            state.is_open = false;
        });
    }

    pub fn set_tab(&mut self, tab: SongDetailsTab) {
        self.state.with_mut(|state| {
            state.active_tab = tab;
        });
    }

    pub fn current(&self) -> SongDetailsState {
        (self.state)()
    }
}

const DESKTOP_TABS: [SongDetailsTab; 3] = [
    SongDetailsTab::Lyrics,
    SongDetailsTab::Queue,
    SongDetailsTab::Related,
];
const MOBILE_TABS: [SongDetailsTab; 4] = [
    SongDetailsTab::Details,
    SongDetailsTab::Queue,
    SongDetailsTab::Related,
    SongDetailsTab::Lyrics,
];
fn is_live_song(song: &Song) -> bool {
    song.server_name == "Radio"
        || song
            .stream_url
            .as_ref()
            .map(|url| !url.trim().is_empty())
            .unwrap_or(false)
}
