use dioxus::prelude::*;
use crate::api::*;
use crate::components::{AppView, Icon};

#[component]
pub fn ArtistDetailView(artist_id: String, server_id: String) -> Element {
    let _servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut current_view = use_context::<Signal<AppView>>();
    
    // For now, we'll show basic artist info
    // A full implementation would fetch artist albums via getArtist endpoint
    
    rsx! {
        div { class: "space-y-8",
            // Back button
            button {
                class: "flex items-center gap-2 text-zinc-400 hover:text-white transition-colors mb-4",
                onclick: move |_| current_view.set(AppView::Artists),
                Icon { name: "prev".to_string(), class: "w-4 h-4".to_string() }
                "Back to Artists"
            }

            div { class: "flex flex-col items-center justify-center py-20",
                Icon {
                    name: "artist".to_string(),
                    class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                }
                p { class: "text-zinc-400", "Artist detail view coming soon" }
                p { class: "text-zinc-500 text-sm", "Artist ID: {artist_id}" }
            }
        }
    }
}
