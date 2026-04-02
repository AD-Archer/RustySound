use serde::{Deserialize, Serialize};

pub const HOME_LAYOUT_SETTINGS_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HomeFeedLoadProfile {
    Conservative,
    #[default]
    Standard,
    Super,
}

impl HomeFeedLoadProfile {
    pub fn from_storage(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "conservative" => Self::Conservative,
            "super" => Self::Super,
            _ => Self::Standard,
        }
    }

    pub fn as_storage(self) -> &'static str {
        match self {
            Self::Conservative => "conservative",
            Self::Standard => "standard",
            Self::Super => "super",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HomeTopStripMode {
    #[default]
    QuickPlay,
    AlbumHighlights,
    Mixed,
}

impl HomeTopStripMode {
    pub fn as_value(self) -> &'static str {
        match self {
            Self::QuickPlay => "quick_play",
            Self::AlbumHighlights => "album_highlights",
            Self::Mixed => "mixed",
        }
    }

    pub fn from_value(value: &str) -> Self {
        match value {
            "album_highlights" => Self::AlbumHighlights,
            "mixed" => Self::Mixed,
            _ => Self::QuickPlay,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HomeSortDirection {
    Asc,
    #[default]
    Desc,
}

impl HomeSortDirection {
    pub fn as_value(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }

    pub fn from_value(value: &str) -> Self {
        match value {
            "asc" => Self::Asc,
            _ => Self::Desc,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HomeAlbumSource {
    #[default]
    RecentlyAdded,
    RecentlyPlayed,
    MostPlayed,
    AtoZ,
    Rating,
    Random,
}

impl HomeAlbumSource {
    pub fn as_value(self) -> &'static str {
        match self {
            Self::RecentlyAdded => "recently_added",
            Self::RecentlyPlayed => "recently_played",
            Self::MostPlayed => "most_played",
            Self::AtoZ => "a_to_z",
            Self::Rating => "rating",
            Self::Random => "random",
        }
    }

    pub fn from_value(value: &str) -> Self {
        match value {
            "recently_played" => Self::RecentlyPlayed,
            "most_played" => Self::MostPlayed,
            "a_to_z" => Self::AtoZ,
            "rating" => Self::Rating,
            "random" => Self::Random,
            _ => Self::RecentlyAdded,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HomeSongSource {
    #[default]
    MostPlayed,
    RecentlyPlayed,
    Random,
    AtoZ,
    Rating,
    QuickPicks,
}

impl HomeSongSource {
    pub fn as_value(self) -> &'static str {
        match self {
            Self::MostPlayed => "most_played",
            Self::RecentlyPlayed => "recently_played",
            Self::Random => "random",
            Self::AtoZ => "a_to_z",
            Self::Rating => "rating",
            Self::QuickPicks => "quick_picks",
        }
    }

    pub fn from_value(value: &str) -> Self {
        match value {
            "recently_played" => Self::RecentlyPlayed,
            "random" => Self::Random,
            "a_to_z" => Self::AtoZ,
            "rating" => Self::Rating,
            "quick_picks" => Self::QuickPicks,
            _ => Self::MostPlayed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HomeQuickPicksLayout {
    #[default]
    List,
    Grid,
}

impl HomeQuickPicksLayout {
    pub fn as_value(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Grid => "grid",
        }
    }

    pub fn from_value(value: &str) -> Self {
        match value {
            "grid" => Self::Grid,
            _ => Self::List,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HomeQuickPicksSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl HomeQuickPicksSize {
    pub fn as_value(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    pub fn from_value(value: &str) -> Self {
        match value {
            "small" => Self::Small,
            "large" => Self::Large,
            _ => Self::Medium,
        }
    }

    pub fn target_columns(self) -> usize {
        match self {
            Self::Small => 6,
            Self::Medium => 5,
            Self::Large => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HomeQuickPlayAction {
    #[default]
    AllSongs,
    Favorites,
    Downloads,
    Playlists,
    RandomMix,
    RadioStations,
    AllAlbums,
    Artists,
    Bookmarks,
    Stats,
    Queue,
}

impl HomeQuickPlayAction {
    pub fn all() -> [HomeQuickPlayAction; 11] {
        [
            HomeQuickPlayAction::AllSongs,
            HomeQuickPlayAction::Favorites,
            HomeQuickPlayAction::Downloads,
            HomeQuickPlayAction::Playlists,
            HomeQuickPlayAction::RandomMix,
            HomeQuickPlayAction::RadioStations,
            HomeQuickPlayAction::AllAlbums,
            HomeQuickPlayAction::Artists,
            HomeQuickPlayAction::Bookmarks,
            HomeQuickPlayAction::Stats,
            HomeQuickPlayAction::Queue,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeQuickPlaySettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_top_quick_play_rows")]
    pub rows: u8,
    #[serde(default = "default_top_quick_play_columns")]
    pub columns: u8,
    #[serde(default = "default_quick_play_actions")]
    pub actions: Vec<HomeQuickPlayAction>,
}

impl Default for HomeQuickPlaySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            rows: default_top_quick_play_rows(),
            columns: default_top_quick_play_columns(),
            actions: default_quick_play_actions(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeAlbumSectionConfig {
    pub id: String,
    pub title: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub source: HomeAlbumSource,
    #[serde(default)]
    pub direction: HomeSortDirection,
    #[serde(default)]
    pub min_rating: u8,
    #[serde(default = "default_section_initial_visible")]
    pub initial_visible: u8,
    #[serde(default = "default_section_load_step")]
    pub load_step: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeSongSectionConfig {
    pub id: String,
    pub title: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub source: HomeSongSource,
    #[serde(default)]
    pub direction: HomeSortDirection,
    #[serde(default)]
    pub min_rating: u8,
    #[serde(default = "default_section_initial_visible")]
    pub initial_visible: u8,
    #[serde(default = "default_section_load_step")]
    pub load_step: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeQuickPicksSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub layout: HomeQuickPicksLayout,
    #[serde(default)]
    pub size: HomeQuickPicksSize,
    #[serde(default = "default_quick_picks_columns")]
    pub columns: u8,
    #[serde(default = "default_quick_picks_rows")]
    pub rows: u8,
    #[serde(default = "default_quick_picks_visible_count")]
    pub visible_count: u8,
}

impl Default for HomeQuickPicksSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            layout: HomeQuickPicksLayout::default(),
            size: HomeQuickPicksSize::default(),
            columns: default_quick_picks_columns(),
            rows: default_quick_picks_rows(),
            visible_count: default_quick_picks_visible_count(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeLayoutSettings {
    #[serde(default = "default_layout_version")]
    pub version: u8,
    #[serde(default)]
    pub fetch_profile: HomeFeedLoadProfile,
    #[serde(default)]
    pub top_strip_mode: HomeTopStripMode,
    #[serde(default)]
    pub top_album_source: HomeAlbumSource,
    #[serde(default)]
    pub top_album_direction: HomeSortDirection,
    #[serde(default = "default_top_album_visible")]
    pub top_album_visible: u8,
    #[serde(default)]
    pub quick_play: HomeQuickPlaySettings,
    #[serde(default = "default_album_sections")]
    pub album_sections: Vec<HomeAlbumSectionConfig>,
    #[serde(default = "default_song_sections")]
    pub song_sections: Vec<HomeSongSectionConfig>,
    #[serde(default)]
    pub quick_picks: HomeQuickPicksSettings,
}

impl Default for HomeLayoutSettings {
    fn default() -> Self {
        Self {
            version: HOME_LAYOUT_SETTINGS_VERSION,
            fetch_profile: HomeFeedLoadProfile::Standard,
            top_strip_mode: HomeTopStripMode::QuickPlay,
            top_album_source: HomeAlbumSource::MostPlayed,
            top_album_direction: HomeSortDirection::Desc,
            top_album_visible: default_top_album_visible(),
            quick_play: HomeQuickPlaySettings::default(),
            album_sections: default_album_sections(),
            song_sections: default_song_sections(),
            quick_picks: HomeQuickPicksSettings::default(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_layout_version() -> u8 {
    HOME_LAYOUT_SETTINGS_VERSION
}

fn default_top_quick_play_rows() -> u8 {
    1
}

fn default_top_quick_play_columns() -> u8 {
    4
}

fn default_top_album_visible() -> u8 {
    8
}

fn default_section_initial_visible() -> u8 {
    9
}

fn default_section_load_step() -> u8 {
    6
}

fn default_quick_picks_columns() -> u8 {
    2
}

fn default_quick_picks_rows() -> u8 {
    4
}

fn default_quick_picks_visible_count() -> u8 {
    8
}

fn default_quick_play_actions() -> Vec<HomeQuickPlayAction> {
    vec![
        HomeQuickPlayAction::AllSongs,
        HomeQuickPlayAction::Favorites,
        HomeQuickPlayAction::Downloads,
        HomeQuickPlayAction::Playlists,
    ]
}

fn default_album_sections() -> Vec<HomeAlbumSectionConfig> {
    vec![
        HomeAlbumSectionConfig {
            id: "albums-recently-added".to_string(),
            title: "Recently Added Albums".to_string(),
            enabled: true,
            source: HomeAlbumSource::RecentlyAdded,
            direction: HomeSortDirection::Desc,
            min_rating: 0,
            initial_visible: 9,
            load_step: 6,
        },
        HomeAlbumSectionConfig {
            id: "albums-most-played".to_string(),
            title: "Most Played Albums".to_string(),
            enabled: true,
            source: HomeAlbumSource::MostPlayed,
            direction: HomeSortDirection::Desc,
            min_rating: 0,
            initial_visible: 9,
            load_step: 6,
        },
    ]
}

fn default_song_sections() -> Vec<HomeSongSectionConfig> {
    vec![
        HomeSongSectionConfig {
            id: "songs-most-played".to_string(),
            title: "Most Played Songs".to_string(),
            enabled: true,
            source: HomeSongSource::MostPlayed,
            direction: HomeSortDirection::Desc,
            min_rating: 0,
            initial_visible: 9,
            load_step: 6,
        },
        HomeSongSectionConfig {
            id: "songs-recently-played".to_string(),
            title: "Recently Played Songs".to_string(),
            enabled: true,
            source: HomeSongSource::RecentlyPlayed,
            direction: HomeSortDirection::Desc,
            min_rating: 0,
            initial_visible: 9,
            load_step: 6,
        },
        HomeSongSectionConfig {
            id: "songs-random".to_string(),
            title: "Random Songs".to_string(),
            enabled: true,
            source: HomeSongSource::Random,
            direction: HomeSortDirection::Desc,
            min_rating: 0,
            initial_visible: 9,
            load_step: 6,
        },
    ]
}

pub fn parse_home_layout_settings(raw: &str) -> HomeLayoutSettings {
    if raw.trim().is_empty() {
        return HomeLayoutSettings::default();
    }

    serde_json::from_str::<HomeLayoutSettings>(raw)
        .unwrap_or_default()
        .normalized()
}

pub fn serialize_home_layout_settings(layout: &HomeLayoutSettings) -> String {
    serde_json::to_string(layout).unwrap_or_default()
}

impl HomeLayoutSettings {
    pub fn normalized(mut self) -> Self {
        self.version = HOME_LAYOUT_SETTINGS_VERSION;
        self.top_album_visible = self.top_album_visible.clamp(3, 24);
        self.quick_play.rows = self.quick_play.rows.clamp(1, 4);
        self.quick_play.columns = self.quick_play.columns.clamp(1, 5);
        self.quick_picks.columns = self.quick_picks.columns.clamp(1, 6);
        self.quick_picks.rows = self.quick_picks.rows.clamp(1, 8);
        self.quick_picks.visible_count = self.quick_picks.visible_count.clamp(2, 48);
        if self.quick_picks.visible_count % 2 != 0 {
            self.quick_picks.visible_count = if self.quick_picks.visible_count < 48 {
                self.quick_picks.visible_count.saturating_add(1)
            } else {
                self.quick_picks.visible_count.saturating_sub(1)
            };
        }

        if self.quick_play.actions.is_empty() {
            self.quick_play.actions = default_quick_play_actions();
        }
        if self.quick_play.actions.len() > 12 {
            self.quick_play.actions.truncate(12);
        }

        if self.album_sections.is_empty() {
            self.album_sections = default_album_sections();
        }
        if self.album_sections.len() > 8 {
            self.album_sections.truncate(8);
        }
        for section in &mut self.album_sections {
            section.title = section.title.trim().to_string();
            section.min_rating = section.min_rating.min(5);
            section.initial_visible = section.initial_visible.clamp(3, 48);
            section.load_step = section.load_step.clamp(1, 24);
            if section.id.trim().is_empty() {
                section.id = "album-section".to_string();
            }
        }

        if self.song_sections.is_empty() {
            self.song_sections = default_song_sections();
        }
        if self.song_sections.len() > 8 {
            self.song_sections.truncate(8);
        }
        for section in &mut self.song_sections {
            section.title = section.title.trim().to_string();
            section.min_rating = section.min_rating.min(5);
            section.initial_visible = section.initial_visible.clamp(3, 48);
            section.load_step = section.load_step.clamp(1, 24);
            if section.id.trim().is_empty() {
                section.id = "song-section".to_string();
            }
        }

        self
    }
}
