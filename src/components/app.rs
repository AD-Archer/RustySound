use crate::api::*;
use crate::components::views::*;
use crate::components::{AudioController, AudioState, Player, Sidebar};
use crate::db::{
    initialize_database, load_playback_state, load_servers, load_settings, save_playback_state,
    save_servers, save_settings, AppSettings, PlaybackState, QueueItem,
};
// Re-export RepeatMode for other components
pub use crate::db::RepeatMode;
use dioxus::prelude::*;

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

    // Provide state via context
    use_context_provider(|| servers);
    use_context_provider(|| current_view);
    use_context_provider(|| now_playing);
    use_context_provider(|| queue);
    use_context_provider(|| queue_index);
    use_context_provider(|| is_playing);
    use_context_provider(|| volume);
    use_context_provider(|| app_settings);
    use_context_provider(|| playback_position);
    use_context_provider(|| shuffle_enabled);
    use_context_provider(|| repeat_mode);
    use_context_provider(|| audio_state);

    // Initialize database and load saved state on mount
    use_effect(move || {
        spawn(async move {
            // Initialize DB
            if let Err(e) = initialize_database().await {
                #[cfg(not(target_arch = "wasm32"))]
                eprintln!("Failed to initialize database: {}", e);
                return;
            }
            db_initialized.set(true);

            // Load servers
            if let Ok(saved_servers) = load_servers().await {
                servers.set(saved_servers);
            }

            // Load settings
            if let Ok(mut settings) = load_settings().await {
                // Clamp volume to [0,1] to avoid media errors
                settings.volume = settings.volume.clamp(0.0, 1.0);
                volume.set(settings.volume);
                shuffle_enabled.set(settings.shuffle_enabled);
                repeat_mode.set(settings.repeat_mode);
                app_settings.set(settings);
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

    rsx! {
        div { class: "app-container flex h-screen bg-gradient-to-br from-zinc-950 via-zinc-900 to-zinc-950 text-white overflow-hidden",
            // Sidebar
            Sidebar {}

            // Main content area
            div { class: "flex-1 flex flex-col overflow-hidden",
                // Main scrollable content
                main { class: "flex-1 overflow-y-auto pb-28",
                    div { class: "p-6 lg:p-8",
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
