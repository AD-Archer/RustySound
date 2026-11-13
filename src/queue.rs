use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
pub struct Song {
    pub id: String,
    pub title: String,
    pub artist: String,
}

impl Song {
    pub fn phantom(id: &str) -> Self {
        Self {
            id: id.to_string(),
            title: format!("Loading track {id}…"),
            artist: "Phantom Queue".to_string(),
        }
    }
}

/// Loads the playback queue in the background.
/// Displays a temporary phantom queue while real songs are being fetched.
#[component]
pub fn BackgroundQueue() -> Element {
    let queue_data = use_resource(move || async move {
        // TODO: Replace with actual queue fetching logic
        // Placeholder resolves to real songs once the background task completes
        vec![Song::phantom("real-1"), Song::phantom("real-2")]
    });

    let display_songs: Vec<&Song> = match queue_data() {
        Some(ref songs) => songs.iter().collect(),
        None => (0..3).map(|i| &Song::phantom(&i.to_string())).collect(),
    };

    rsx! {
        div { class: "queue-list",
            for song in display_songs.iter() {
                div { class: "queue-item", "{song.title} - {song.artist}" }
            }
        }
    }
}
