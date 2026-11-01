pub mod discovery;
pub mod jellyfin;
pub mod listening;
pub mod lyrics;
pub mod models;
pub mod music_client;
pub mod navidrome;
pub mod offline_queue;

pub use jellyfin::JellyfinClient;
pub use lyrics::*;
pub use models::*;
pub use music_client::{Capabilities, MusicClient};
pub use navidrome::*;
