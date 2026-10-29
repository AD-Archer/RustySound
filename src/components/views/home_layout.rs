//! What remains of the old Home layout settings. The Home editor, QuickPlay
//! cards and custom sections are gone (Discover replaced Home); only the feed
//! load profile survives because the shell's start-up warm-up in `app.rs`
//! still reads it. Delete this file when that warm-up goes.

/// How aggressively the shell warms the old home feed at start-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HomeFeedLoadProfile {
    Conservative,
    #[default]
    Standard,
    Super,
}

impl HomeFeedLoadProfile {
    /// Parse the stored `home_feed_load_profile` setting.
    pub fn from_storage(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "conservative" => Self::Conservative,
            "super" => Self::Super,
            _ => Self::Standard,
        }
    }

    /// The stored form (the shell's warm-up cache keys use it).
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::Conservative => "conservative",
            Self::Standard => "standard",
            Self::Super => "super",
        }
    }
}
