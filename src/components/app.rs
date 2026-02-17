use crate::api::*;
use crate::components::{
    view_label, AddIntent, AddMenuController, AddToMenuOverlay, AppView, AudioController,
    AudioState, Icon, Navigation, PlaybackPositionSignal, Player, SeekRequestSignal, Sidebar,
    SidebarOpenSignal, SongDetailsController, SongDetailsOverlay, SongDetailsState, VolumeSignal,
};
use crate::db::{
    initialize_database, load_playback_state, load_servers, load_settings, save_playback_state,
    save_servers, save_settings, AppSettings, PlaybackState, QueueItem,
};
#[cfg(target_arch = "wasm32")]
use dioxus::core::{Runtime, RuntimeGuard};
use dioxus_router::components::Outlet;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys::window;
// Re-export RepeatMode for other components
pub use crate::db::RepeatMode;
use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
const HISTORY_SWIPE_THRESHOLD: f64 = 100.0;
#[cfg(target_arch = "wasm32")]
const HISTORY_SWIPE_VERTICAL_SLOP: f64 = 72.0;
#[cfg(target_arch = "wasm32")]
const HISTORY_SWIPE_EDGE_ZONE: f64 = 28.0;

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

#[component]
pub fn AppShell() -> Element {
    let mut servers = use_signal(Vec::<ServerConfig>::new);
    let current_view = use_route::<AppView>();
    let now_playing = use_signal(|| None::<Song>);
    let queue = use_signal(Vec::<Song>::new);
    let mut queue_index = use_signal(|| 0usize);
    let is_playing = use_signal(|| false);
    let mut volume = use_signal(|| 0.8f64);
    let mut app_settings = use_signal(AppSettings::default);
    let mut playback_position = use_signal(|| 0.0f64);
    let mut db_initialized = use_signal(|| false);
    let mut settings_loaded = use_signal(|| false);
    let mut shuffle_enabled = use_signal(|| false);
    let mut repeat_mode = use_signal(|| RepeatMode::Off);
    let audio_state = use_signal(AudioState::default);
    let sidebar_open = use_signal(|| false);
    let navigation = Navigation::new();
    let seek_request = use_signal(|| None::<(String, f64)>);
    let mut resume_bookmark_loaded = use_signal(|| false);
    #[cfg(target_arch = "wasm32")]
    let swipe_start = use_signal(|| None::<(f64, f64, i8)>);
    let swipe_hint = use_signal(|| None::<(i8, f64)>);
    let add_menu_intent = use_signal(|| None::<AddIntent>);
    let add_menu = AddMenuController::new(add_menu_intent.clone());
    let song_details_state = use_signal(SongDetailsState::default);
    let song_details = SongDetailsController::new(song_details_state.clone());

    // Provide state via context
    use_context_provider(|| servers);
    use_context_provider(|| current_view);
    use_context_provider(|| navigation.clone());
    use_context_provider(|| add_menu.clone());
    use_context_provider(|| song_details.clone());

    #[cfg(target_arch = "wasm32")]
    let nav_for_swipe = navigation.clone();
    #[cfg(target_arch = "wasm32")]
    let sidebar_open_for_swipe = sidebar_open.clone();
    // Global pointer listeners so back swipe works anywhere on the screen (PWA-like)
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let Some(win) = window() else {
            return;
        };

        let runtime = Runtime::current();
        let mut swipe_start = swipe_start.clone();
        let mut swipe_hint = swipe_hint.clone();
        let nav = nav_for_swipe.clone();
        let sidebar_open_for_swipe = sidebar_open_for_swipe.clone();

        let runtime_down = runtime.clone();
        let down_cb = Closure::wrap(Box::new(move |e: web_sys::PointerEvent| {
            let _guard = RuntimeGuard::new(runtime_down.clone());
            if e.pointer_type() != "touch" || sidebar_open_for_swipe() {
                swipe_start.set(None);
                swipe_hint.set(None);
                return;
            }

            let viewport_width = window()
                .and_then(|w| w.inner_width().ok())
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0);
            if viewport_width <= 0.0 {
                swipe_start.set(None);
                swipe_hint.set(None);
                return;
            }

            let x = e.client_x() as f64;
            let y = e.client_y() as f64;
            let direction = if x <= HISTORY_SWIPE_EDGE_ZONE {
                1
            } else if x >= viewport_width - HISTORY_SWIPE_EDGE_ZONE {
                -1
            } else {
                0
            };

            if direction == 0 {
                swipe_start.set(None);
                swipe_hint.set(None);
                return;
            }

            swipe_start.set(Some((x, y, direction)));
            swipe_hint.set(Some((direction, 0.0)));
        }) as Box<dyn FnMut(_)>);
        let move_cb = {
            let mut swipe_start = swipe_start.clone();
            let nav = nav.clone();
            let mut swipe_hint = swipe_hint.clone();
            let runtime_move = runtime.clone();
            Closure::wrap(Box::new(move |e: web_sys::PointerEvent| {
                let _guard = RuntimeGuard::new(runtime_move.clone());
                if let Some((start_x, start_y, direction)) = swipe_start() {
                    let delta_x = e.client_x() as f64 - start_x;
                    let delta_y = e.client_y() as f64 - start_y;
                    if delta_y.abs() > HISTORY_SWIPE_VERTICAL_SLOP {
                        swipe_start.set(None);
                        swipe_hint.set(None);
                        return;
                    }

                    let travel = if direction > 0 {
                        delta_x.max(0.0)
                    } else {
                        (-delta_x).max(0.0)
                    };
                    let progress = (travel / HISTORY_SWIPE_THRESHOLD).clamp(0.0, 1.2);
                    swipe_hint.set(Some((direction, progress)));

                    if progress >= 1.0 {
                        if direction > 0 && nav.can_go_back() {
                            nav.go_back();
                        } else if direction < 0 && nav.can_go_forward() {
                            nav.go_forward();
                        }
                        swipe_start.set(None);
                        swipe_hint.set(None);
                    }
                }
            }) as Box<dyn FnMut(_)>)
        };
        let up_cb = {
            let mut swipe_start = swipe_start.clone();
            let mut swipe_hint = swipe_hint.clone();
            let runtime_up = runtime.clone();
            Closure::wrap(Box::new(move |_e: web_sys::PointerEvent| {
                let _guard = RuntimeGuard::new(runtime_up.clone());
                swipe_start.set(None);
                swipe_hint.set(None);
            }) as Box<dyn FnMut(_)>)
        };
        let cancel_cb = {
            let mut swipe_start = swipe_start.clone();
            let mut swipe_hint = swipe_hint.clone();
            let runtime_cancel = runtime.clone();
            Closure::wrap(Box::new(move |_e: web_sys::PointerEvent| {
                let _guard = RuntimeGuard::new(runtime_cancel.clone());
                swipe_start.set(None);
                swipe_hint.set(None);
            }) as Box<dyn FnMut(_)>)
        };

        let _ =
            win.add_event_listener_with_callback("pointerdown", down_cb.as_ref().unchecked_ref());
        let _ =
            win.add_event_listener_with_callback("pointermove", move_cb.as_ref().unchecked_ref());
        let _ = win.add_event_listener_with_callback("pointerup", up_cb.as_ref().unchecked_ref());
        let _ = win
            .add_event_listener_with_callback("pointercancel", cancel_cb.as_ref().unchecked_ref());

        down_cb.forget();
        move_cb.forget();
        up_cb.forget();
        cancel_cb.forget();
    });
    use_context_provider(|| now_playing);
    use_context_provider(|| queue);
    use_context_provider(|| queue_index);
    use_context_provider(|| is_playing);
    use_context_provider(|| VolumeSignal(volume));
    use_context_provider(|| app_settings);
    use_context_provider(|| PlaybackPositionSignal(playback_position));
    use_context_provider(|| SeekRequestSignal(seek_request));
    use_context_provider(|| SidebarOpenSignal(sidebar_open));
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
                    let _ = save_settings(normalized_settings.clone()).await;
                }
            }
            settings_loaded.set(true);

            // Load playback state (but don't auto-play)
            if let Ok(state) = load_playback_state().await {
                queue_index.set(state.queue_index);
                playback_position.set(state.position);
                // Note: We don't restore the full queue/song here since we'd need to re-fetch song details
                // That would require knowing which server each song came from
            }
        });
    });

    // Resume from the most recent bookmark on startup.
    use_effect(move || {
        if resume_bookmark_loaded() {
            return;
        }
        if !settings_loaded() {
            return;
        }
        if now_playing().is_some() {
            resume_bookmark_loaded.set(true);
            return;
        }

        let bookmark_autoplay_on_launch = app_settings().bookmark_autoplay_on_launch;
        if !bookmark_autoplay_on_launch {
            resume_bookmark_loaded.set(true);
            return;
        }
        let servers_snapshot = servers();
        if servers_snapshot.is_empty() {
            return;
        }

        let mut queue = queue.clone();
        let mut queue_index = queue_index.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let mut playback_position = playback_position.clone();
        let mut seek_request = seek_request.clone();
        let mut resume_bookmark_loaded = resume_bookmark_loaded.clone();
        spawn(async move {
            let mut candidates: Vec<Bookmark> = Vec::new();

            for server in servers_snapshot.iter().filter(|s| s.active).cloned() {
                let client = NavidromeClient::new(server.clone());
                if let Ok(mut bookmarks) = client.get_bookmarks().await {
                    for bm in bookmarks.iter_mut() {
                        if bm.entry.server_id.is_empty() {
                            bm.entry.server_id = server.id.clone();
                        }
                        if bm.entry.server_name.is_empty() {
                            bm.entry.server_name = server.name.clone();
                        }
                    }
                    candidates.extend(
                        bookmarks
                            .into_iter()
                            .filter(|bookmark| !bookmark.entry.id.trim().is_empty()),
                    );
                }
            }

            candidates.sort_by(|a, b| {
                b.changed
                    .cmp(&a.changed)
                    .then_with(|| b.created.cmp(&a.created))
            });

            let mut resumed_song: Option<(Song, f64)> = None;
            for bookmark in candidates.into_iter() {
                let Some(server) = servers_snapshot
                    .iter()
                    .find(|server| server.id == bookmark.server_id)
                    .cloned()
                else {
                    continue;
                };

                let client = NavidromeClient::new(server);
                if let Ok(song) = client.get_song(&bookmark.entry.id).await {
                    let position = bookmark.position as f64 / 1000.0;
                    resumed_song = Some((song, position));
                    break;
                }
            }

            if let Some((song, position)) = resumed_song {
                queue.set(vec![song.clone()]);
                queue_index.set(0);
                now_playing.set(Some(song.clone()));
                playback_position.set(position);
                seek_request.set(Some((song.id.clone(), position)));
                is_playing.set(true);
            }

            resume_bookmark_loaded.set(true);
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

    let view = use_route::<AppView>();
    let sidebar_signal = sidebar_open.clone();
    let can_go_back = navigation.can_go_back();
    let song_details_open = song_details_state().is_open;
    let app_container_class = if sidebar_open() {
        "app-container sidebar-open-mobile flex min-h-screen text-white overflow-hidden"
    } else {
        "app-container flex min-h-screen text-white overflow-hidden"
    };
    let swipe_hint_state = swipe_hint();

    rsx! {
        div { class: "{app_container_class}",
            if sidebar_open() && !song_details_open {
                div {
                    class: "fixed inset-0 bg-black/60 backdrop-blur-sm z-30 2xl:hidden",
                    onclick: {
                        let mut sidebar_open = sidebar_open.clone();
                        move |_| sidebar_open.set(false)
                    },
                }
            }

            // Sidebar
            Sidebar { sidebar_open: sidebar_signal, overlay_mode: false }

            // Main content area
            div { class: "flex-1 flex flex-col overflow-hidden",
                header { class: "mobile-safe-top 2xl:hidden border-b border-zinc-800/60 bg-zinc-950/80 backdrop-blur-xl",
                    div { class: "flex items-center justify-between px-4 py-3",
                        div { class: "flex items-center gap-1",
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
                            if can_go_back {
                                button {
                                    class: "p-2 rounded-lg text-zinc-300 hover:text-white hover:bg-zinc-800/60 transition-colors",
                                    aria_label: "Go back",
                                    onclick: {
                                        let navigation = navigation.clone();
                                        move |_| {
                                            let _ = navigation.go_back();
                                        }
                                    },
                                    Icon {
                                        name: "arrow-left".to_string(),
                                        class: "w-5 h-5".to_string(),
                                    }
                                }
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
                                let nav = navigation.clone();
                                move |_| nav.navigate_to(AppView::QueueView {})
                            },
                            Icon {
                                name: "bars".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                    }
                }

                // Main scrollable content
                main {
                    class: "flex-1 overflow-y-auto main-scroll",
                    div {
                        class: "page-shell",
                        Outlet::<AppView> {}
                    }
                }
            }

            // Fixed bottom player
            Player {}
        }

        if let Some((direction, progress)) = swipe_hint_state {
            if progress > 0.0 {
                div {
                    class: if direction > 0 {
                        "swipe-hint swipe-hint--back 2xl:hidden"
                    } else {
                        "swipe-hint swipe-hint--forward 2xl:hidden"
                    },
                    style: if direction > 0 {
                        format!(
                            "opacity: {}; transform: translateY(-50%) translateX({}px) scale({});",
                            0.2 + progress.min(1.0) * 0.8,
                            -12.0 + progress.min(1.0) * 12.0,
                            0.86 + progress.min(1.0) * 0.18
                        )
                    } else {
                        format!(
                            "opacity: {}; transform: translateY(-50%) translateX({}px) scale({});",
                            0.2 + progress.min(1.0) * 0.8,
                            12.0 - progress.min(1.0) * 12.0,
                            0.86 + progress.min(1.0) * 0.18
                        )
                    },
                    div {
                        class: "w-10 h-10 rounded-full border border-emerald-400/50 bg-zinc-950/80 text-emerald-300 shadow-lg backdrop-blur flex items-center justify-center",
                        Icon {
                            name: "arrow-left".to_string(),
                            class: if direction > 0 { "w-5 h-5".to_string() } else { "w-5 h-5 rotate-180".to_string() },
                        }
                    }
                }
            }
        }

        AddToMenuOverlay { controller: add_menu.clone() }

        SongDetailsOverlay { controller: song_details.clone() }

        if song_details_open {
            if sidebar_open() {
                div {
                    class: "fixed inset-0 bg-black/60 backdrop-blur-sm z-[115]",
                    onclick: {
                        let mut sidebar_open = sidebar_open.clone();
                        move |_| sidebar_open.set(false)
                    },
                }
            }
            Sidebar { sidebar_open: sidebar_open, overlay_mode: true }
        }

        // Audio controller - manages playback separately from UI
        AudioController {}
    }
}
