use crate::api::*;
use crate::components::views::*;
use crate::components::{
    AudioController, AudioState, Icon, PlaybackPositionSignal, Player, Sidebar, VolumeSignal,
};
use crate::db::{
    initialize_database, load_playback_state, load_servers, load_settings, save_playback_state,
    save_servers, save_settings, AppSettings, PlaybackState, QueueItem,
};
// Re-export RepeatMode for other components
pub use crate::db::RepeatMode;
use dioxus::prelude::*;

fn normalize_volume(mut value: f64) -> f64 {
    if !value.is_finite() {
        return 0.8;
    }
    let mut passes = 0;
    while value > 1.0 && passes < 4 {
        value /= 100.0;
        passes += 1;
    }
    value.clamp(0.0, 1.0)
}

fn view_label(view: &AppView) -> &'static str {
    match view {
        AppView::Home => "Home",
        AppView::Search => "Search",
        AppView::Albums => "Albums",
        AppView::Artists => "Artists",
        AppView::Playlists => "Playlists",
        AppView::Radio => "Radio",
        AppView::Favorites => "Favorites",
        AppView::Random => "Random",
        AppView::Settings => "Settings",
        AppView::Queue => "Queue",
        AppView::AlbumDetail(_, _) => "Album",
        AppView::ArtistDetail(_, _) => "Artist",
        AppView::PlaylistDetail(_, _) => "Playlist",
    }
}

#[derive(Clone, PartialEq)]
pub enum AppView {
    Home,
    Search,
    Albums,
    Artists,
    Playlists,
    Radio,
    Favorites,
    Random,
    Settings,
    Queue,
    AlbumDetail(String, String),    // album_id, server_id
    ArtistDetail(String, String),   // artist_id, server_id
    PlaylistDetail(String, String), // playlist_id, server_id
}

#[component]
pub fn AppShell() -> Element {
    let mut servers = use_signal(Vec::<ServerConfig>::new);
    let current_view = use_signal(|| AppView::Home);
    let now_playing = use_signal(|| None::<Song>);
    let queue = use_signal(Vec::<Song>::new);
    let mut queue_index = use_signal(|| 0usize);
    let is_playing = use_signal(|| false);
    let mut volume = use_signal(|| 0.8f64);
    let mut app_settings = use_signal(AppSettings::default);
    let mut playback_position = use_signal(|| 0.0f64);
    let mut db_initialized = use_signal(|| false);
    let mut shuffle_enabled = use_signal(|| false);
    let mut repeat_mode = use_signal(|| RepeatMode::Off);
    let audio_state = use_signal(AudioState::default);
    let sidebar_open = use_signal(|| false);

    // Provide state via context
    use_context_provider(|| servers);
    use_context_provider(|| current_view);
    use_context_provider(|| now_playing);
    use_context_provider(|| queue);
    use_context_provider(|| queue_index);
    use_context_provider(|| is_playing);
    use_context_provider(|| VolumeSignal(volume));
    use_context_provider(|| app_settings);
    use_context_provider(|| PlaybackPositionSignal(playback_position));
    use_context_provider(|| shuffle_enabled);
    use_context_provider(|| repeat_mode);
    use_context_provider(|| audio_state);

    // Initialize database and load saved state on mount
    use_effect(move || {
        spawn(async move {
            // Initialize DB
            if let Err(_e) = initialize_database().await {
                #[cfg(not(target_arch = "wasm32"))]
                eprintln!("Failed to initialize database: {}", _e);
                return;
            }
            db_initialized.set(true);

            // Load servers
            if let Ok(saved_servers) = load_servers().await {
                servers.set(saved_servers);
            }

            // Load settings
            if let Ok(mut settings) = load_settings().await {
                let original_volume = settings.volume;
                settings.volume = normalize_volume(settings.volume);
                volume.set(settings.volume);
                shuffle_enabled.set(settings.shuffle_enabled);
                repeat_mode.set(settings.repeat_mode);
                let normalized_settings = settings.clone();
                app_settings.set(settings);
                if (normalized_settings.volume - original_volume).abs() > f64::EPSILON {
                    let _ = save_settings(normalized_settings).await;
                }
            }

            // Load playback state (but don't auto-play)
            if let Ok(state) = load_playback_state().await {
                queue_index.set(state.queue_index);
                playback_position.set(state.position);
                // Note: We don't restore the full queue/song here since we'd need to re-fetch song details
                // That would require knowing which server each song came from
            }
        });
    });

    // Auto-save servers when they change
    use_effect(move || {
        let current_servers = servers();
        if db_initialized() && !current_servers.is_empty() {
            spawn(async move {
                let _ = save_servers(current_servers).await;
            });
        }
    });

    // Auto-save settings when volume, shuffle, or repeat changes
    use_effect(move || {
        let vol = volume();
        let vol = normalize_volume(vol);
        let shuffle = shuffle_enabled();
        let repeat = repeat_mode();
        let mut settings = app_settings();
        
        if db_initialized() {
            let changed = (settings.volume - vol).abs() > 0.01 
                || settings.shuffle_enabled != shuffle 
                || settings.repeat_mode != repeat;
            
            if changed {
                settings.volume = vol;
                settings.shuffle_enabled = shuffle;
                settings.repeat_mode = repeat;
                app_settings.set(settings.clone());
                spawn(async move {
                    let _ = save_settings(settings).await;
                });
            }
        }
    });

    // Normalize volume if any writer pushes it out of range
    use_effect(move || {
        let vol = volume();
        let normalized = normalize_volume(vol);
        if (vol - normalized).abs() > f64::EPSILON {
            volume.set(normalized);
        }
    });

    // Auto-save playback position periodically
    use_effect(move || {
        let song = now_playing();
        let pos = playback_position();
        let q = queue();
        let idx = queue_index();

        if db_initialized() && song.is_some() {
            let state = PlaybackState {
                song_id: song.as_ref().map(|s| s.id.clone()),
                server_id: song.as_ref().map(|s| s.server_id.clone()),
                position: pos,
                queue: q
                    .iter()
                    .map(|s| QueueItem {
                        song_id: s.id.clone(),
                        server_id: s.server_id.clone(),
                    })
                    .collect(),
                queue_index: idx,
            };
            spawn(async move {
                let _ = save_playback_state(state).await;
            });
        }
    });

    let view = current_view();
    let sidebar_signal = sidebar_open.clone();

    rsx! {
        div { class: "app-container flex min-h-screen text-white overflow-hidden",
            if sidebar_open() {
                div {
                    class: "fixed inset-0 bg-black/60 backdrop-blur-sm z-30 md:hidden",
                    onclick: {
                        let mut sidebar_open = sidebar_open.clone();
                        move |_| sidebar_open.set(false)
                    },
                }
            }

            // Sidebar
            Sidebar { sidebar_open: sidebar_signal }

            // Main content area
            div { class: "flex-1 flex flex-col overflow-hidden",
                header { class: "md:hidden border-b border-zinc-800/60 bg-zinc-950/80 backdrop-blur-xl",
                    div { class: "flex items-center justify-between px-4 py-3",
                        button {
                            class: "p-2 rounded-lg text-zinc-300 hover:text-white hover:bg-zinc-800/60 transition-colors",
                            aria_label: "Open menu",
                            onclick: {
                                let mut sidebar_open = sidebar_open.clone();
                                move |_| sidebar_open.set(true)
                            },
                            Icon {
                                name: "menu".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                        div { class: "flex flex-col items-center text-center",
                            span { class: "text-xs uppercase tracking-widest text-zinc-500",
                                "RustySound"
                            }
                            span { class: "text-sm font-semibold text-white", "{view_label(&view)}" }
                        }
                        button {
                            class: "p-2 rounded-lg text-zinc-300 hover:text-white hover:bg-zinc-800/60 transition-colors",
                            aria_label: "Open queue",
                            onclick: {
                                let mut current_view = current_view.clone();
                                move |_| current_view.set(AppView::Queue)
                            },
                            Icon {
                                name: "bars".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                    }
                }

                // Main scrollable content
                main { class: "flex-1 overflow-y-auto main-scroll",
                    div { class: "page-shell",
                        {
                            match view {
                                AppView::Home => rsx! {
                                    HomeView {}
                                },
                                AppView::Search => rsx! {
                                    SearchView {}
                                },
                                AppView::Albums => rsx! {
                                    AlbumsView {}
                                },
                                AppView::Artists => rsx! {
                                    ArtistsView {}
                                },
                                AppView::Playlists => rsx! {
                                    PlaylistsView {}
                                },
                                AppView::Radio => rsx! {
                                    RadioView {}
                                },
                                AppView::Favorites => rsx! {
                                    FavoritesView {}
                                },
                                AppView::Random => rsx! {
                                    RandomView {}
                                },
                                AppView::Settings => rsx! {
                                    SettingsView {}
                                },
                                AppView::Queue => rsx! {
                                    QueueView {}
                                },
                                AppView::AlbumDetail(album_id, server_id) => rsx! {
                                    AlbumDetailView { album_id: album_id.clone(), server_id: server_id.clone() }
                                },
                                AppView::ArtistDetail(artist_id, server_id) => rsx! {
                                    ArtistDetailView { artist_id: artist_id.clone(), server_id: server_id.clone() }
                                },
                                AppView::PlaylistDetail(playlist_id, server_id) => rsx! {
                                    PlaylistDetailView { playlist_id: playlist_id.clone(), server_id: server_id.clone() }
                                },
                            }
                        }
                    }
                }
            }

            // Fixed bottom player
            Player {}
        }

        // Audio controller - manages playback separately from UI
        AudioController {}
    }
}
