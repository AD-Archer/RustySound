use crate::api::*;
use crate::components::views::home::SongRow;
use crate::components::Icon;
use dioxus::prelude::*;

#[component]
pub fn RandomView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<Signal<bool>>();
    let shuffle_enabled = use_context::<Signal<bool>>();

    let refresh_counter = use_signal(|| 0);
    let mut shuffled_songs = use_signal(|| Vec::<Song>::new());

    let active_servers: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
    let counter = refresh_counter();

    let songs_resource = use_resource(move || {
        let servers = active_servers.clone();
        let _counter = counter; // Force refresh dependency
        async move {
            let mut songs = Vec::new();
            for server in servers {
                let client = NavidromeClient::new(server);
                if let Ok(server_songs) = client.get_random_songs(25).await {
                    songs.extend(server_songs);
                }
            }
            // Shuffle combined results using getrandom (wasm-compatible)
            shuffle_songs(&mut songs);
            songs.truncate(50);
            songs
        }
    });

    // Update shuffled_songs when songs resource changes
    use_effect(move || {
        if let Some(new_songs) = songs_resource() {
            shuffled_songs.set(new_songs);
        }
    });

    let on_play_all = {
        let shuffled_songs = shuffled_songs.clone();
        move |_| {
            let songs = shuffled_songs();
            if !songs.is_empty() {
                queue.set(songs.clone());
                queue_index.set(0);
                now_playing.set(Some(songs[0].clone()));
                is_playing.set(true);
            }
        }
    };

    let on_shuffle = {
        let mut shuffled_songs = shuffled_songs.clone();
        move |_: MouseEvent| {
            let mut current_songs = shuffled_songs();
            if !current_songs.is_empty() {
                shuffle_songs(&mut current_songs);
                shuffled_songs.set(current_songs);
            }
        }
    };

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header page-header--split",
                div {
                    h1 { class: "page-title", "Random Mix" }
                    p { class: "page-subtitle", "A random selection from your library" }
                }
                div { class: "flex flex-wrap gap-6",

                    button {
                        class: "px-6 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-white font-medium transition-colors flex items-center gap-2",
                        onclick: on_play_all,
                        Icon {
                            name: "play".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        "Play All"
                    }
                    button {
                        class: "px-6 py-2 rounded-xl bg-zinc-800 text-zinc-200 border border-zinc-700/60 hover:border-zinc-500 transition-colors flex items-center gap-2",
                        onclick: on_shuffle,
                        Icon {
                            name: "shuffle".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        "Shuffle"
                    }
                }
            }

            {
                let songs = shuffled_songs();
                if !songs.is_empty() {
                    rsx! {
                        div { class: "space-y-1",
                            for (index , song) in songs.iter().enumerate() {
                                SongRow {
                                    song: song.clone(),
                                    index: index + 1,
                                    onclick: {
                                        let song = song.clone();
                                        let songs_for_queue = songs.clone();
                                        move |_| {
                                            queue.set(songs_for_queue.clone());
                                            queue_index.set(index);
                                            now_playing.set(Some(song.clone()));
                                            is_playing.set(true);
                                        }
                                    },
                                }
                            }
                        }
                    }
                } else if songs_resource().is_none() {
                    rsx! {
                        div { class: "flex items-center justify-center py-20",
                            Icon {
                                name: "loader".to_string(),
                                class: "w-8 h-8 text-zinc-500".to_string(),
                            }
                        }
                    }
                } else {
                    rsx! {
                        div { class: "flex flex-col items-center justify-center py-20",
                            Icon {
                                name: "shuffle".to_string(),
                                class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                            }
                            h2 { class: "text-xl font-semibold text-white mb-2", "No songs available" }
                            p { class: "text-zinc-400", "Connect a server with music to get random picks" }
                        }
                    }
                }
            }
        }
    }
}

// Fisher-Yates shuffle using getrandom (wasm-compatible)
fn shuffle_songs(songs: &mut Vec<Song>) {
    let len = songs.len();
    if len <= 1 {
        return;
    }

    for i in (1..len).rev() {
        let mut bytes = [0u8; 4];
        let _ = getrandom::getrandom(&mut bytes);
        let j = u32::from_le_bytes(bytes) as usize % (i + 1);
        songs.swap(i, j);
    }
}
