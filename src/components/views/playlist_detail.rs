use crate::api::*;
use crate::cache_service::{get_json as cache_get_json, put_json as cache_put_json};
use crate::components::{
    AddIntent, AddMenuController, AppView, Icon, Navigation, PlaybackPositionSignal,
    PreviewPlaybackSignal, SeekRequestSignal,
};
use crate::db::AppSettings;
use crate::diagnostics::{log_perf, PerfTimer};
use crate::offline_audio::{
    download_songs_batch, is_song_downloaded, mark_collection_downloaded, prefetch_song_audio,
};
use dioxus::prelude::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

const QUICK_PREVIEW_DURATION_MS: u64 = 12000;
const AUTO_RECOMMENDATION_LIMIT: usize = 25;
const AUTO_RECOMMENDATION_FIRST_SEED_COUNT: usize = 4;
const AUTO_RECOMMENDATION_LAST_SEED_COUNT: usize = 4;
const AUTO_RECOMMENDATION_RECENT_SEED_COUNT: usize = 17;

#[cfg(not(target_arch = "wasm32"))]
async fn quick_preview_delay_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(target_arch = "wasm32")]
async fn quick_preview_delay_ms(ms: u64) {
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn playlist_search_delay_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(target_arch = "wasm32")]
async fn playlist_search_delay_ms(ms: u64) {
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
}

#[component]
fn PlaylistSongRow(
    song: Song,
    display_index: usize,
    songs: Vec<Song>,
    queue: Signal<Vec<Song>>,
    queue_index: Signal<usize>,
    now_playing: Signal<Option<Song>>,
    is_playing: Signal<bool>,
    servers: Signal<Vec<ServerConfig>>,
    add_menu: AddMenuController,
    can_remove_from_playlist: bool,
    on_remove_from_playlist: EventHandler<usize>,
) -> Element {
    let navigation = use_context::<Navigation>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let current_rating = use_signal(|| song.user_rating.unwrap_or(0).min(5));
    let is_favorited = use_signal(|| song.starred.is_some());
    let download_busy = use_signal(|| false);
    let mut show_mobile_actions = use_signal(|| false);
    let initially_downloaded = is_song_downloaded(&song);
    let downloaded = use_signal(move || initially_downloaded);
    let is_current = now_playing()
        .as_ref()
        .map(|current| current.id == song.id)
        .unwrap_or(false);

    let cover_url = servers()
        .iter()
        .find(|s| s.id == song.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            song.cover_art
                .as_ref()
                .map(|ca| client.get_cover_art_url(ca, 80))
        });

    let make_on_open_menu = {
        let add_menu = add_menu.clone();
        let song = song.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move || {
            let mut add_menu = add_menu.clone();
            let song = song.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                add_menu.open(AddIntent::from_song(song.clone()));
            }
        }
    };

    let make_on_set_rating = {
        let servers = servers.clone();
        let song_id = song.id.clone();
        let server_id = song.server_id.clone();
        let current_rating = current_rating.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move |new_rating: u32| {
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            let mut current_rating = current_rating.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                let normalized = new_rating.min(5);
                current_rating.set(normalized);
                let servers = servers.clone();
                let song_id = song_id.clone();
                let server_id = server_id.clone();
                spawn(async move {
                    if let Some(server) = servers().iter().find(|s| s.id == server_id) {
                        let client = NavidromeClient::new(server.clone());
                        let _ = client.set_rating(&song_id, normalized).await;
                    }
                });
            }
        }
    };

    let make_on_toggle_favorite = {
        let servers = servers.clone();
        let song_id = song.id.clone();
        let server_id = song.server_id.clone();
        let is_favorited = is_favorited.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move || {
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            let mut is_favorited = is_favorited.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                let should_star = !is_favorited();
                let servers = servers.clone();
                let song_id = song_id.clone();
                let server_id = server_id.clone();
                spawn(async move {
                    if let Some(server) = servers().iter().find(|s| s.id == server_id) {
                        let client = NavidromeClient::new(server.clone());
                        let result = if should_star {
                            client.star(&song_id, "song").await
                        } else {
                            client.unstar(&song_id, "song").await
                        };
                        if result.is_ok() {
                            is_favorited.set(should_star);
                        }
                    }
                });
            }
        }
    };

    let make_on_download_song = {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let song = song.clone();
        let download_busy = download_busy.clone();
        let downloaded = downloaded.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move || {
            let servers = servers.clone();
            let app_settings = app_settings.clone();
            let song = song.clone();
            let mut download_busy = download_busy.clone();
            let mut downloaded = downloaded.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                if download_busy() || downloaded() {
                    return;
                }
                let servers_snapshot = servers();
                if servers_snapshot.is_empty() {
                    return;
                }
                let mut settings_snapshot = app_settings();
                settings_snapshot.downloads_enabled = true;
                download_busy.set(true);
                let song = song.clone();
                spawn(async move {
                    if prefetch_song_audio(&song, &servers_snapshot, &settings_snapshot)
                        .await
                        .is_ok()
                    {
                        downloaded.set(true);
                    }
                    download_busy.set(false);
                });
            }
        }
    };

    let make_on_remove_from_playlist = {
        let show_mobile_actions = show_mobile_actions.clone();
        let on_remove_from_playlist = on_remove_from_playlist.clone();
        let remove_index = display_index.saturating_sub(1);
        move || {
            let mut show_mobile_actions = show_mobile_actions.clone();
            let on_remove_from_playlist = on_remove_from_playlist.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                on_remove_from_playlist.call(remove_index);
            }
        }
    };

    let mut on_click_row = {
        let song = song.clone();
        let songs_for_queue = songs.clone();
        let mut queue = queue.clone();
        let mut queue_index = queue_index.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        move |_| {
            queue.set(songs_for_queue.clone());
            queue_index.set(display_index - 1);
            now_playing.set(Some(song.clone()));
            is_playing.set(true);
        }
    };

    let on_album_cover = {
        let navigation = navigation.clone();
        let album_id = song.album_id.clone();
        let server_id = song.server_id.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            if let Some(album_id_val) = album_id.clone() {
                navigation.navigate_to(AppView::AlbumDetailView {
                    album_id: album_id_val,
                    server_id: server_id.clone(),
                });
            }
        }
    };

    rsx! {
        div {
            class: if is_current {
                "relative w-full flex items-center gap-4 p-3 rounded-xl bg-emerald-500/5 transition-colors group cursor-pointer"
            } else {
                "relative w-full flex items-center gap-4 p-3 rounded-xl hover:bg-zinc-800/50 transition-colors group cursor-pointer"
            },
            onclick: move |evt| {
                show_mobile_actions.set(false);
                on_click_row(evt);
            },
            if is_current {
                span { class: "w-6 text-sm text-emerald-400",
                    Icon { name: "play".to_string(), class: "w-4 h-4".to_string() }
                }
            } else {
                span { class: "w-6 text-sm text-zinc-500 group-hover:hidden", "{display_index}" }
                span { class: "w-6 text-sm text-white hidden group-hover:block",
                    Icon { name: "play".to_string(), class: "w-4 h-4".to_string() }
                }
            }
            button {
                class: "w-12 h-12 rounded bg-zinc-800 overflow-hidden flex-shrink-0",
                aria_label: "Open album",
                onclick: on_album_cover,
                match cover_url {
                    Some(url) => rsx! {
                        img { class: "w-full h-full object-cover", src: "{url}" }
                    },
                    None => rsx! {
                        div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                            Icon { name: "music".to_string(), class: "w-5 h-5 text-zinc-500".to_string() }
                        }
                    },
                }
            }
            div { class: "flex-1 min-w-0 text-center md:text-left",
                p { class: if is_current { "text-sm font-medium text-emerald-400 truncate transition-colors" } else { "text-sm font-medium text-white truncate group-hover:text-emerald-400 transition-colors" },
                    "{song.title}"
                }
                p { class: "text-xs text-zinc-400 truncate",
                    "{song.album.clone().unwrap_or_default()}"
                }
            }
            div { class: "flex items-center gap-2 md:gap-3 relative",
                if downloaded() {
                    span {
                        class: "hidden md:inline-flex p-2 text-emerald-400",
                        title: "Downloaded",
                        Icon { name: "check".to_string(), class: "w-4 h-4".to_string() }
                    }
                } else {
                    button {
                        class: if download_busy() {
                            "hidden md:inline-flex p-2 text-zinc-500 cursor-not-allowed"
                        } else {
                            "hidden md:inline-flex p-2 text-zinc-500 hover:text-emerald-400 transition-colors"
                        },
                        aria_label: "Download song",
                        disabled: download_busy(),
                        onclick: make_on_download_song(),
                        Icon {
                            name: if download_busy() { "loader".to_string() } else { "download".to_string() },
                            class: "w-4 h-4".to_string(),
                        }
                    }
                }
                if current_rating() > 0 {
                    div { class: "hidden md:flex items-center gap-1 text-amber-400",
                        for i in 1..=5 {
                            Icon {
                                name: if i <= current_rating() { "star-filled".to_string() } else { "star".to_string() },
                                class: "w-3.5 h-3.5".to_string(),
                            }
                        }
                    }
                }
                button {
                    class: if is_favorited() { "hidden md:inline-flex p-2 text-emerald-400 hover:text-emerald-300 transition-colors" } else { "hidden md:inline-flex p-2 text-zinc-500 hover:text-emerald-400 transition-colors" },
                    aria_label: "Favorite",
                    onclick: make_on_toggle_favorite(),
                    Icon {
                        name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                        class: "w-4 h-4".to_string(),
                    }
                }
                button {
                    class: "hidden md:inline-flex p-2 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                    aria_label: "Add to queue",
                    onclick: make_on_open_menu(),
                    Icon { name: "plus".to_string(), class: "w-4 h-4".to_string() }
                }
                if can_remove_from_playlist {
                    button {
                        class: "hidden md:inline-flex p-2 rounded-lg text-zinc-500 hover:text-red-300 hover:bg-red-500/10 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                        aria_label: "Remove from playlist",
                        onclick: make_on_remove_from_playlist(),
                        Icon { name: "trash".to_string(), class: "w-4 h-4".to_string() }
                    }
                }
                span { class: "text-sm text-zinc-500", "{format_duration(song.duration)}" }
                button {
                    class: "md:hidden p-2 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors",
                    aria_label: "Song actions",
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        show_mobile_actions.set(!show_mobile_actions());
                    },
                    Icon {
                        name: "more-horizontal".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
                if show_mobile_actions() {
                    div {
                        class: "md:hidden absolute right-0 top-10 z-20 w-44 rounded-xl border border-zinc-700 bg-zinc-900/95 shadow-2xl p-1.5 space-y-1",
                        onclick: move |evt: MouseEvent| evt.stop_propagation(),
                        button {
                            class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                            onclick: make_on_open_menu(),
                            Icon { name: "plus".to_string(), class: "w-4 h-4".to_string() }
                            "Add To..."
                        }
                        if downloaded() {
                            div { class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-emerald-300 bg-emerald-500/10",
                                Icon { name: "check".to_string(), class: "w-4 h-4".to_string() }
                                "Downloaded"
                            }
                        } else {
                            button {
                                class: if download_busy() { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-500 cursor-not-allowed" } else { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors" },
                                disabled: download_busy(),
                                onclick: make_on_download_song(),
                                Icon {
                                    name: if download_busy() { "loader".to_string() } else { "download".to_string() },
                                    class: "w-4 h-4".to_string(),
                                }
                                if download_busy() {
                                    "Downloading..."
                                } else {
                                    "Download"
                                }
                            }
                        }
                        button {
                            class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                            onclick: make_on_toggle_favorite(),
                            Icon {
                                name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                                class: "w-4 h-4".to_string(),
                            }
                            if is_favorited() {
                                "Unfavorite"
                            } else {
                                "Favorite"
                            }
                        }
                        if can_remove_from_playlist {
                            button {
                                class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-red-300 hover:bg-red-500/10 transition-colors",
                                onclick: make_on_remove_from_playlist(),
                                Icon { name: "trash".to_string(), class: "w-4 h-4".to_string() }
                                "Remove from playlist"
                            }
                        }
                        div { class: "px-2.5 pt-1 text-[11px] uppercase tracking-wide text-zinc-500", "Rating" }
                        div { class: "flex items-center gap-1 px-2 pb-1",
                            for i in 1..=5 {
                                button {
                                    class: "p-1 rounded text-amber-400 hover:text-amber-300 transition-colors",
                                    onclick: make_on_set_rating(i as u32),
                                    Icon {
                                        name: if i <= current_rating() { "star-filled".to_string() } else { "star".to_string() },
                                        class: "w-3.5 h-3.5".to_string(),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn PlaylistDetailView(playlist_id: String, server_id: String) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<Signal<bool>>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let seek_request = use_context::<SeekRequestSignal>().0;
    let preview_playback = use_context::<PreviewPlaybackSignal>().0;
    let add_menu = use_context::<AddMenuController>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let mut is_favorited = use_signal(|| false);
    let reload = use_signal(|| 0usize);
    let mut song_search = use_signal(String::new);
    let song_search_debounced = use_signal(String::new);
    let song_search_generation = use_signal(|| 0u64);
    let edit_mode = use_signal(|| false);
    let deleting_playlist = use_signal(|| false);
    let delete_error = use_signal(|| None::<String>);
    let reorder_error = use_signal(|| None::<String>);
    let mut song_list = use_signal(|| Vec::<Song>::new());
    let mut show_delete_confirm = use_signal(|| false);
    let recently_added_seed = use_signal(|| None::<Song>);
    let dismissed_recommendations = use_signal(HashSet::<String>::new);
    let recommendation_refresh_nonce = use_signal(|| 0u64);
    let preview_session = use_signal(|| 0u64);
    let preview_song_key = use_signal(|| None::<String>);
    let download_busy = use_signal(|| false);
    let download_status = use_signal(|| None::<String>);

    let server = servers().into_iter().find(|s| s.id == server_id);
    let server_for_playlist = server.clone();

    {
        let mut song_search_debounced = song_search_debounced.clone();
        let mut song_search_generation = song_search_generation.clone();
        use_effect(move || {
            let query = song_search().trim().to_string();
            song_search_generation.with_mut(|value| *value = value.saturating_add(1));
            let generation = *song_search_generation.peek();

            if query.len() < 2 {
                song_search_debounced.set(String::new());
                return;
            }

            let mut song_search_debounced = song_search_debounced.clone();
            let song_search_generation = song_search_generation.clone();
            spawn(async move {
                playlist_search_delay_ms(220).await;
                if *song_search_generation.peek() != generation {
                    return;
                }
                song_search_debounced.set(query);
            });
        });
    }

    let playlist_data = use_resource(move || {
        let server = server_for_playlist.clone();
        let playlist_id = playlist_id.clone();
        let _reload = reload();
        async move {
            if let Some(server) = server {
                let client = NavidromeClient::new(server);
                client.get_playlist(&playlist_id).await.ok()
            } else {
                None
            }
        }
    });

    let search_results = {
        let server = server.clone();
        use_resource(move || {
            let server = server.clone();
            let query = song_search_debounced();
            async move { search_playlist_add_candidates(server, query).await }
        })
    };

    let auto_recommendations = {
        let server = server.clone();
        let edit_mode = edit_mode.clone();
        let song_list = song_list.clone();
        let recently_added_seed = recently_added_seed.clone();
        let dismissed_recommendations = dismissed_recommendations.clone();
        let recommendation_refresh_nonce = recommendation_refresh_nonce.clone();
        use_resource(move || {
            let server = server.clone();
            let editing = edit_mode();
            let playlist_songs = song_list();
            let recent_seed = recently_added_seed();
            let dismissed_keys = dismissed_recommendations();
            let _refresh = recommendation_refresh_nonce();
            async move {
                if !editing {
                    return Vec::new();
                }
                build_playlist_add_recommendations(
                    server,
                    playlist_songs,
                    recent_seed,
                    dismissed_keys,
                )
                .await
            }
        })
    };

    let on_preview_song = Rc::new({
        let queue_index = queue_index.clone();
        let now_playing = now_playing.clone();
        let is_playing = is_playing.clone();
        let playback_position = playback_position.clone();
        let seek_request = seek_request.clone();
        let preview_playback = preview_playback.clone();
        let preview_session = preview_session.clone();
        let preview_song_key = preview_song_key.clone();
        move |song: Song| {
            let queue_index = queue_index.clone();
            let mut now_playing = now_playing.clone();
            let mut is_playing = is_playing.clone();
            let mut playback_position = playback_position.clone();
            let mut seek_request = seek_request.clone();
            let mut preview_playback = preview_playback.clone();
            let mut preview_session = preview_session.clone();
            let mut preview_song_key = preview_song_key.clone();
            let saved_queue_index = queue_index();
            let saved_now_playing = now_playing();
            let saved_is_playing = is_playing();
            let saved_playback_position = playback_position();
            let saved_seek_request = seek_request();
            let saved_seek = saved_seek_request.or_else(|| {
                saved_now_playing
                    .as_ref()
                    .map(|current| (current.id.clone(), saved_playback_position.max(0.0)))
            });

            preview_session.with_mut(|session| *session = session.saturating_add(1));
            let session = preview_session();
            preview_song_key.set(Some(song_identity_key(&song)));
            preview_playback.set(true);

            playback_position.set(0.0);
            seek_request.set(Some((song.id.clone(), 0.0)));
            now_playing.set(Some(song));
            is_playing.set(true);

            let mut queue_index = queue_index.clone();
            let mut now_playing = now_playing.clone();
            let mut is_playing = is_playing.clone();
            let mut playback_position = playback_position.clone();
            let mut seek_request = seek_request.clone();
            let preview_session = preview_session.clone();
            let mut preview_song_key = preview_song_key.clone();
            spawn(async move {
                quick_preview_delay_ms(QUICK_PREVIEW_DURATION_MS).await;
                if preview_session() != session {
                    return;
                }
                queue_index.set(saved_queue_index);
                now_playing.set(saved_now_playing);
                is_playing.set(saved_is_playing);
                playback_position.set(saved_playback_position.max(0.0));
                seek_request.set(saved_seek);
                preview_song_key.set(None);
                preview_playback.set(false);
            });
        }
    });

    let on_play_all = {
        let playlist_data_ref = playlist_data.clone();
        move |_| {
            if let Some(Some((_, songs))) = playlist_data_ref() {
                if !songs.is_empty() {
                    queue.set(songs.clone());
                    queue_index.set(0);
                    now_playing.set(Some(songs[0].clone()));
                    is_playing.set(true);
                }
            }
        }
    };

    let on_open_playlist_menu = {
        let playlist_data_ref = playlist_data.clone();
        let mut add_menu = add_menu.clone();
        move |_: MouseEvent| {
            if let Some(Some((playlist, _))) = playlist_data_ref() {
                add_menu.open(AddIntent::from_playlist(&playlist));
            }
        }
    };

    let on_download_playlist = {
        let playlist_data_ref = playlist_data.clone();
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let mut download_busy = download_busy.clone();
        let mut download_status = download_status.clone();
        move |_| {
            if download_busy() {
                return;
            }

            let Some(Some((playlist, songs))) = playlist_data_ref() else {
                download_status.set(Some("No songs available to download.".to_string()));
                return;
            };
            if songs.is_empty() {
                download_status.set(Some("No songs available to download.".to_string()));
                return;
            }

            let servers_snapshot = servers();
            if servers_snapshot.is_empty() {
                download_status.set(Some("No servers configured.".to_string()));
                return;
            }

            let settings_snapshot = app_settings();
            let playlist_meta = playlist.clone();
            download_busy.set(true);
            download_status.set(Some("Downloading playlist songs...".to_string()));
            spawn(async move {
                let report =
                    download_songs_batch(&songs, &servers_snapshot, &settings_snapshot).await;
                if report.downloaded > 0 || report.skipped > 0 {
                    mark_collection_downloaded(
                        "playlist",
                        &playlist_meta.server_id,
                        &playlist_meta.id,
                        &playlist_meta.name,
                        songs.len(),
                    );
                }
                download_status.set(Some(format!(
                    "Playlist download complete: {} new, {} skipped, {} failed, {} purged.",
                    report.downloaded, report.skipped, report.failed, report.purged
                )));
                download_busy.set(false);
            });
        }
    };

    use_effect(move || {
        if let Some(Some((playlist, _))) = playlist_data() {
            is_favorited.set(playlist.starred.is_some());
        }
    });

    let is_auto_imported = {
        if let Some(Some((playlist, _))) = playlist_data() {
            playlist
                .comment
                .as_ref()
                .map(|c| c.to_lowercase().contains("auto-imported"))
                .unwrap_or(false)
        } else {
            false
        }
    };

    let on_favorite_toggle = {
        let playlist_data_ref = playlist_data.clone();
        let mut is_favorited = is_favorited.clone();
        let servers = servers.clone();
        move |_| {
            if let Some(Some((playlist, _))) = playlist_data_ref() {
                if let Some(server) = servers()
                    .iter()
                    .find(|s| s.id == playlist.server_id)
                    .cloned()
                {
                    let playlist_id = playlist.id.clone();
                    let should_star = !is_favorited();
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        let result = if should_star {
                            client.star(&playlist_id, "playlist").await
                        } else {
                            client.unstar(&playlist_id, "playlist").await
                        };
                        if result.is_ok() {
                            is_favorited.set(should_star);
                        }
                    });
                }
            }
        }
    };

    let on_remove_song = {
        let playlist_data_ref = playlist_data.clone();
        let servers = servers.clone();
        let song_list_signal = song_list.clone();
        move |song_index: usize| {
            if let Some(Some((playlist, _))) = playlist_data_ref() {
                if let Some(server) = servers()
                    .iter()
                    .find(|s| s.id == playlist.server_id)
                    .cloned()
                {
                    let playlist_id = playlist.id.clone();
                    let mut song_list = song_list_signal.clone();
                    if song_index >= song_list().len() {
                        return;
                    }
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        let result = client
                            .remove_songs_from_playlist(&playlist_id, &[song_index])
                            .await;
                        if result.is_ok() {
                            song_list.with_mut(|list| {
                                if song_index < list.len() {
                                    list.remove(song_index);
                                }
                            });
                        }
                    });
                }
            }
        }
    };

    let on_add_song = {
        let playlist_data_ref = playlist_data.clone();
        let servers = servers.clone();
        let mut reload = reload.clone();
        let mut recently_added_seed = recently_added_seed.clone();
        move |song: Song| {
            if let Some(Some((playlist, _))) = playlist_data_ref() {
                if let Some(server) = servers()
                    .iter()
                    .find(|s| s.id == playlist.server_id)
                    .cloned()
                {
                    let playlist_id = playlist.id.clone();
                    let song_id = song.id.clone();
                    let song_for_seed = song.clone();
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        if client
                            .add_songs_to_playlist(&playlist_id, &[song_id])
                            .await
                            .is_ok()
                        {
                            recently_added_seed.set(Some(song_for_seed));
                            reload.set(reload() + 1);
                        }
                    });
                }
            }
        }
    };

    let on_reorder_song = {
        let playlist_data_ref = playlist_data.clone();
        let servers = servers.clone();
        let mut song_list = song_list.clone();
        let mut reorder_error = reorder_error.clone();
        let reload = reload.clone();
        Rc::new(RefCell::new(
            move |source_index: usize, target_index: usize| {
                let mut ordered_song_ids = Vec::<String>::new();
                let mut reordered = false;
                song_list.with_mut(|list| {
                    if list.len() < 2
                        || source_index >= list.len()
                        || target_index >= list.len()
                        || source_index == target_index
                    {
                        return;
                    }

                    let moved_song = list.remove(source_index);
                    let insert_index = target_index;
                    list.insert(insert_index, moved_song);
                    ordered_song_ids = list.iter().map(|song| song.id.clone()).collect();
                    reordered = true;
                });

                if !reordered {
                    return;
                }

                reorder_error.set(None);

                if let Some(Some((playlist, _))) = playlist_data_ref() {
                    if let Some(server) = servers()
                        .iter()
                        .find(|s| s.id == playlist.server_id)
                        .cloned()
                    {
                        let playlist_id = playlist.id.clone();
                        let total_songs = ordered_song_ids.len();
                        let mut reorder_error = reorder_error.clone();
                        let mut reload = reload.clone();
                        spawn(async move {
                            let client = NavidromeClient::new(server);
                            if let Err(err) = client
                                .reorder_playlist(&playlist_id, &ordered_song_ids, total_songs)
                                .await
                            {
                                reorder_error
                                    .set(Some(format!("Failed to save playlist order: {err}")));
                                reload.set(reload().saturating_add(1));
                            }
                        });
                    }
                }
            },
        ))
    };

    let on_toggle_edit_mode = {
        let mut edit_mode = edit_mode.clone();
        let mut song_search = song_search.clone();
        let mut reorder_error = reorder_error.clone();
        let mut dismissed_recommendations = dismissed_recommendations.clone();
        let mut recommendation_refresh_nonce = recommendation_refresh_nonce.clone();
        move |_| {
            let next_edit_state = !edit_mode();
            edit_mode.set(next_edit_state);
            song_search.set(String::new());
            reorder_error.set(None);
            dismissed_recommendations.set(HashSet::new());
            recommendation_refresh_nonce.set(0);
        }
    };

    let on_refresh_recommendations = {
        let mut recommendation_refresh_nonce = recommendation_refresh_nonce.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            recommendation_refresh_nonce.set(recommendation_refresh_nonce().saturating_add(1));
        }
    };

    let delete_playlist_action = {
        let playlist_data_ref = playlist_data.clone();
        let servers = servers.clone();
        let deleting_playlist = deleting_playlist.clone();
        let delete_error = delete_error.clone();
        let navigation = navigation.clone();
        Rc::new(RefCell::new(move || {
            let mut deleting_playlist_flag = deleting_playlist.clone();
            if deleting_playlist_flag() {
                return;
            }
            if let Some(Some((playlist, _))) = playlist_data_ref() {
                if let Some(server) = servers()
                    .iter()
                    .find(|s| s.id == playlist.server_id)
                    .cloned()
                {
                    deleting_playlist_flag.set(true);
                    let playlist_id = playlist.id.clone();
                    let navigation = navigation.clone();
                    let mut deleting_playlist_clone = deleting_playlist_flag.clone();
                    let mut delete_error_clone = delete_error.clone();
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        match client.delete_playlist(&playlist_id).await {
                            Ok(_) => navigation.navigate_to(AppView::PlaylistsView {}),
                            Err(err) => delete_error_clone.set(Some(err)),
                        }
                        deleting_playlist_clone.set(false);
                    });
                }
            }
        }))
    };
    let on_delete_playlist = {
        let mut show_delete_confirm = show_delete_confirm.clone();
        let mut delete_error = delete_error.clone();
        move |_: MouseEvent| {
            delete_error.set(None);
            show_delete_confirm.set(true);
        }
    };
    let on_confirm_delete = {
        let delete_playlist_action = delete_playlist_action.clone();
        let mut show_delete_confirm = show_delete_confirm.clone();
        move |_: MouseEvent| {
            show_delete_confirm.set(false);
            delete_playlist_action.borrow_mut()();
        }
    };

    use_effect(move || {
        if let Some(Some((_, songs))) = playlist_data() {
            song_list.set(songs.clone());
        }
    });

    rsx! {
        div { class: "space-y-8",
            button {
                class: "flex items-center gap-2 text-zinc-400 hover:text-white transition-colors mb-4",
                onclick: move |_| {
                    if navigation.go_back().is_none() {
                        navigation.navigate_to(AppView::PlaylistsView {});
                    }
                },
                Icon { name: "prev".to_string(), class: "w-4 h-4".to_string() }
                "Back to Playlists"
            }

            match playlist_data() {
                Some(Some((playlist, songs))) => {
                    let cover_url = servers()
                        .iter()
                        .find(|s| s.id == playlist.server_id)
                        .and_then(|server| {
                            let client = NavidromeClient::new(server.clone());
                            playlist
                                .cover_art
                                .as_ref()
                                .map(|ca| client.get_cover_art_url(ca, 500))
                        });
                    let hide_comment = playlist
                        .comment
                        .as_ref()
                        .map(|c| c.to_lowercase().contains("auto-imported"))
                        .unwrap_or(false);
                    let editing_allowed = !is_auto_imported;
                    let downloaded_song_count =
                        songs.iter().filter(|song| is_song_downloaded(song)).count();

                    rsx! {
                        div { class: "flex flex-col md:flex-row gap-8 mb-8 items-center md:items-end",
                            div { class: "w-64 h-64 rounded-2xl bg-zinc-800 overflow-hidden shadow-2xl flex-shrink-0",
                                match cover_url {
                                    Some(url) => rsx! {





                                        img { class: "w-full h-full object-cover", src: "{url}" }
                                    },
                                    None => rsx! {


                                        div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-indigo-600 to-purple-700",
                                            Icon {
                                                name: "playlist".to_string(),
                                                class: "w-20 h-20 text-white/70".to_string(),
                                            }
                                        }
                                    },
                                }
                            }
                            div { class: "flex flex-col justify-end text-center md:text-left",
                                p { class: "text-sm text-zinc-400 uppercase tracking-wide mb-2", "Playlist" }
                                h1 { class: "text-4xl font-bold text-white mb-4", "{playlist.name}" }
                                if let Some(comment) = &playlist.comment {
                                    if !hide_comment {
                                        p { class: "text-zinc-400 mb-4", "{comment}" }
                                    }
                                }
                                if hide_comment {
                                    p { class: "text-xs text-amber-300 bg-amber-500/10 border border-amber-500/40 rounded-lg px-3 py-2",
                                        "Auto-imported playlists cannot be edited."
                                    }
                                }
                                if let Some(err) = delete_error() {
                                    div { class: "p-3 rounded-lg bg-red-500/10 border border-red-500/40 text-red-200 text-sm mb-3",
                                        "{err}"
                                    }
                                }
                                if let Some(err) = reorder_error() {
                                    div { class: "p-3 rounded-lg bg-amber-500/10 border border-amber-500/40 text-amber-200 text-sm mb-3",
                                        "{err}"
                                    }
                                }
                                div { class: "flex items-center gap-4 text-sm text-zinc-400 justify-center md:justify-start",
                                    if let Some(owner) = &playlist.owner {
                                        span { "by {owner}" }
                                    }
                                    span { "{playlist.song_count} songs" }
                                    span { "{format_duration(playlist.duration / 1000)}" }
                                    span { "{downloaded_song_count} downloaded" }
                                }
                                div { class: "flex gap-3 mt-6 flex-wrap justify-center md:justify-start",
                                    button {
                                        class: "px-8 py-3 rounded-full bg-emerald-500 hover:bg-emerald-400 text-white font-medium transition-colors flex items-center gap-2",
                                        onclick: on_play_all,
                                        Icon { name: "play".to_string(), class: "w-5 h-5".to_string() }
                                        "Play"
                                    }
                                    button {
                                        class: if download_busy() {
                                            "px-4 py-3 rounded-full border border-zinc-700 text-zinc-500 cursor-not-allowed text-sm"
                                        } else {
                                            "px-4 py-3 rounded-full border border-emerald-500/60 text-emerald-300 hover:text-white hover:border-emerald-400 transition-colors text-sm flex items-center gap-2"
                                        },
                                        disabled: download_busy(),
                                        onclick: on_download_playlist,
                                        Icon {
                                            name: if download_busy() { "loader".to_string() } else { "download".to_string() },
                                            class: "w-4 h-4".to_string(),
                                        }
                                        if download_busy() {
                                            "Downloading..."
                                        } else {
                                            "Download Playlist"
                                        }
                                    }
                                    button {
                                        class: "p-3 rounded-full border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors",
                                        onclick: {
                                            let playlist_data_ref = playlist_data.clone();
                                            move |_: MouseEvent| {
                                                if let Some(Some((_, songs))) = playlist_data_ref() {
                                                    if !songs.is_empty() {
                                                        let mut shuffled = songs.clone();
                                                        use rand::seq::SliceRandom;
                                                        shuffled.shuffle(&mut rand::thread_rng());
                                                        queue.set(shuffled.clone());
                                                        queue_index.set(0);
                                                        now_playing.set(Some(shuffled[0].clone()));
                                                        is_playing.set(true);
                                                    }
                                                }
                                            }
                                        },
                                        Icon { name: "shuffle".to_string(), class: "w-5 h-5".to_string() }
                                    }
                                    button {
                                        class: "p-3 rounded-full border border-zinc-700 text-zinc-400 hover:text-emerald-400 hover:border-emerald-500/50 transition-colors",
                                        onclick: on_favorite_toggle,
                                        Icon {
                                            name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                                            class: "w-5 h-5".to_string(),
                                        }
                                    }
                                    button {
                                        class: "p-3 rounded-full border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors",
                                        onclick: on_open_playlist_menu,
                                        Icon { name: "plus".to_string(), class: "w-5 h-5".to_string() }
                                    }
                                    if editing_allowed {
                                        button {
                                            class: "px-4 py-2 rounded-full border border-emerald-500/60 text-emerald-300 hover:text-white hover:bg-emerald-500/10 transition-colors text-sm",
                                            onclick: on_toggle_edit_mode,
                                            if edit_mode() {
                                                "Done editing"
                                            } else {
                                                "Edit playlist"
                                            }
                                        }
                                    }
                                    if editing_allowed {
                                        button {
                                            class: "px-4 py-2 rounded-full border border-red-500/60 text-red-300 hover:text-white hover:bg-red-500/10 transition-colors text-sm",
                                            onclick: on_delete_playlist,
                                            disabled: deleting_playlist(),
                                            if deleting_playlist() {
                                                "Deleting..."
                                            } else {
                                                "Delete playlist"
                                            }
                                        }
                                    }
                                }
                                if let Some(status) = download_status() {
                                    p { class: "text-xs text-zinc-500 mt-2", "{status}" }
                                }
                            }
                        }

                        div { class: "space-y-1",
                            for (index , song) in song_list().iter().enumerate() {
                                if edit_mode() {
                                    {
                                        let cover_url = servers()
                                            .iter()
                                            .find(|s| s.id == song.server_id)
                                            .and_then(|server| {
                                                let client = NavidromeClient::new(server.clone());
                                                song.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 80))
                                            });
                                        let can_move_up = index > 0;
                                        let can_move_down = index + 1 < song_list().len();
                                        rsx! {
                                            div {
                                                key: "{song.server_id}:{song.id}:{index}",
                                                class: "flex items-center gap-3 p-3 rounded-lg bg-zinc-900/60 border border-zinc-800 transition-all",
                                                div { class: "flex flex-col gap-1",
                                                    button {
                                                        r#type: "button",
                                                        class: if can_move_up {
                                                            "w-7 h-7 rounded-md border border-zinc-700/80 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors flex items-center justify-center"
                                                        } else {
                                                            "w-7 h-7 rounded-md border border-zinc-800 text-zinc-600 cursor-not-allowed flex items-center justify-center"
                                                        },
                                                        title: "Move up",
                                                        disabled: !can_move_up,
                                                        onclick: {
                                                            let on_reorder_song = on_reorder_song.clone();
                                                            let source_index = index;
                                                            move |evt: MouseEvent| {
                                                                evt.stop_propagation();
                                                                if !editing_allowed || !can_move_up {
                                                                    return;
                                                                }
                                                                on_reorder_song.borrow_mut()(
                                                                    source_index,
                                                                    source_index.saturating_sub(1),
                                                                );
                                                            }
                                                        },
                                                        Icon { name: "chevron-up".to_string(), class: "w-3.5 h-3.5".to_string() }
                                                    }
                                                    button {
                                                        r#type: "button",
                                                        class: if can_move_down {
                                                            "w-7 h-7 rounded-md border border-zinc-700/80 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors flex items-center justify-center"
                                                        } else {
                                                            "w-7 h-7 rounded-md border border-zinc-800 text-zinc-600 cursor-not-allowed flex items-center justify-center"
                                                        },
                                                        title: "Move down",
                                                        disabled: !can_move_down,
                                                        onclick: {
                                                            let on_reorder_song = on_reorder_song.clone();
                                                            let source_index = index;
                                                            move |evt: MouseEvent| {
                                                                evt.stop_propagation();
                                                                if !editing_allowed || !can_move_down {
                                                                    return;
                                                                }
                                                                on_reorder_song.borrow_mut()(
                                                                    source_index,
                                                                    source_index.saturating_add(1),
                                                                );
                                                            }
                                                        },
                                                        Icon { name: "chevron-down".to_string(), class: "w-3.5 h-3.5".to_string() }
                                                    }
                                                }
                                                div { class: "w-12 h-12 rounded bg-zinc-800 overflow-hidden flex-shrink-0",
                                                    match cover_url {
                                                        Some(url) => rsx! {
                                                            img { class: "w-full h-full object-cover", src: "{url}" }
                                                        },
                                                        None => rsx! {
                                                            div { class: "w-full h-full flex items-center justify-center bg-zinc-800",
                                                                Icon { name: "music".to_string(), class: "w-4 h-4 text-zinc-500".to_string() }
                                                            }
                                                        },
                                                    }
                                                }
                                                div { class: "min-w-0 flex-1",
                                                    p { class: "text-sm text-white truncate", "{song.title}" }
                                                    p { class: "text-xs text-zinc-500 truncate",
                                                        "{song.artist.clone().unwrap_or_default()} • {song.album.clone().unwrap_or_default()}"
                                                    }
                                                }
                                                if editing_allowed {
                                                    button {
                                                        class: "p-2 rounded-full bg-zinc-950/70 text-zinc-300 hover:text-red-300 hover:bg-red-500/20 transition-colors",
                                                        onclick: {
                                                            let remove_index = index;
                                                            move |evt: MouseEvent| {
                                                                evt.stop_propagation();
                                                                on_remove_song(remove_index);
                                                            }
                                                        },
                                                        Icon { name: "trash".to_string(), class: "w-4 h-4".to_string() }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    PlaylistSongRow {
                                        key: "{song.server_id}:{song.id}:{index}",
                                        song: song.clone(),
                                        display_index: index + 1,
                                        songs: songs.clone(),
                                        queue: queue.clone(),
                                        queue_index: queue_index.clone(),
                                        now_playing: now_playing.clone(),
                                        is_playing: is_playing.clone(),
                                        servers: servers.clone(),
                                        add_menu: add_menu.clone(),
                                        can_remove_from_playlist: editing_allowed,
                                        on_remove_from_playlist: move |remove_index| on_remove_song(remove_index),
                                    }
                                }
                            }
                        }

                        if editing_allowed && edit_mode() {
                            div { class: "mt-6 space-y-3 p-4 rounded-xl bg-zinc-900/60 border border-zinc-800",
                                h3 { class: "text-sm font-semibold text-white", "Add songs to this playlist" }
                                input {
                                    class: "w-full px-3 py-2 rounded-lg bg-zinc-950/60 border border-zinc-800 text-white placeholder:text-zinc-600 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                                    placeholder: "Search songs to add",
                                    value: song_search,
                                    oninput: move |e| song_search.set(e.value()),
                                }
                                div { class: "rounded-xl border border-zinc-800/70 bg-zinc-950/30 p-3 space-y-2",
                                    div { class: "flex items-center justify-between",
                                        p { class: "text-xs uppercase tracking-wide text-zinc-500", "Recommended" }
                                        p { class: "text-xs text-zinc-600", "first + last + recent (up to 25)" }
                                    }
                                    match auto_recommendations() {
                                        None => rsx! {
                                            div { class: "py-2 flex items-center gap-2 text-zinc-500 text-sm",
                                                Icon {
                                                    name: "loader".to_string(),
                                                    class: "w-4 h-4 animate-spin".to_string(),
                                                }
                                                "Finding recommendations..."
                                            }
                                        },
                                        Some(recommendations) => {
                                            if recommendations.is_empty() {
                                                rsx! {
                                                    p { class: "text-sm text-zinc-500", "No recommendations yet. Add a song to shape suggestions." }
                                                }
                                            } else {
                                                rsx! {
                                                    div { class: "space-y-2 max-h-64 overflow-y-auto pr-1",
                                                        for res in recommendations {
                                                            div {
                                                                key: "{res.server_id}:{res.id}:recommended",
                                                                class: "flex items-center justify-between gap-3 p-2 rounded-lg hover:bg-zinc-800/50 transition-colors",
                                                                {
                                                                    let cover_url = servers()
                                                                        .iter()
                                                                        .find(|s| s.id == res.server_id)
                                                                        .and_then(|server| {
                                                                            let client = NavidromeClient::new(server.clone());
                                                                            res.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 80))
                                                                        });
                                                                    let cover_album_id = res.album_id.clone();
                                                                    let cover_server_id = res.server_id.clone();
                                                                    let navigation_for_cover = navigation.clone();
                                                                    rsx! {
                                                                        if let Some(url) = cover_url {
                                                                            if let Some(album_id) = cover_album_id {
                                                                                button {
                                                                                    class: "w-10 h-10 rounded overflow-hidden border border-zinc-800/80 flex-shrink-0",
                                                                                    aria_label: "Open album",
                                                                                    onclick: {
                                                                                        let album_id = album_id.clone();
                                                                                        let server_id = cover_server_id.clone();
                                                                                        let navigation = navigation_for_cover.clone();
                                                                                        move |evt: MouseEvent| {
                                                                                            evt.stop_propagation();
                                                                                            navigation.navigate_to(AppView::AlbumDetailView {
                                                                                                album_id: album_id.clone(),
                                                                                                server_id: server_id.clone(),
                                                                                            });
                                                                                        }
                                                                                    },
                                                                                    img {
                                                                                        class: "w-full h-full object-cover",
                                                                                        src: "{url}",
                                                                                    }
                                                                                }
                                                                            } else {
                                                                                img {
                                                                                    class: "w-10 h-10 rounded object-cover border border-zinc-800/80",
                                                                                    src: "{url}",
                                                                                }
                                                                            }
                                                                        } else {
                                                                            div { class: "w-10 h-10 rounded bg-zinc-800 flex items-center justify-center border border-zinc-800/80",
                                                                                Icon {
                                                                                    name: "music".to_string(),
                                                                                    class: "w-4 h-4 text-zinc-500".to_string(),
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                                div { class: "min-w-0 flex-1",
                                                                    p { class: "text-sm text-white truncate", "{res.title}" }
                                                                    p { class: "text-xs text-zinc-500 truncate",
                                                                        "{res.artist.clone().unwrap_or_default()} • {res.album.clone().unwrap_or_default()}"
                                                                    }
                                                                }
                                                                div { class: "flex items-center gap-2",
                                                                    {
                                                                        let already_in_playlist = song_list()
                                                                            .iter()
                                                                            .any(|existing| same_song_identity(existing, &res));
                                                                        rsx! {
                                                                            button {
                                                                                class: if preview_song_key()
                                                                                    == Some(song_identity_key(&res))
                                                                                {
                                                                                    "px-3 py-1 rounded-lg border border-zinc-700 text-zinc-500 text-xs cursor-not-allowed"
                                                                                } else {
                                                                                    "px-3 py-1 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-xs"
                                                                                },
                                                                                title: "Play a short preview, then return to your current song",
                                                                                disabled: preview_song_key()
                                                                                    == Some(song_identity_key(&res)),
                                                                                onclick: {
                                                                                    let song = res.clone();
                                                                                    let on_preview_song = on_preview_song.clone();
                                                                                    move |evt: MouseEvent| {
                                                                                        evt.stop_propagation();
                                                                                        on_preview_song(song.clone());
                                                                                    }
                                                                                },
                                                                                if preview_song_key()
                                                                                    == Some(song_identity_key(&res))
                                                                                {
                                                                                    "Previewing..."
                                                                                } else {
                                                                                    "Preview"
                                                                                }
                                                                            }
                                                                            button {
                                                                                class: if already_in_playlist {
                                                                                    "px-3 py-1 rounded-lg border border-zinc-700 text-zinc-500 text-xs cursor-not-allowed"
                                                                                } else {
                                                                                    "px-3 py-1 rounded-lg border border-emerald-500/60 text-emerald-300 hover:text-white hover:bg-emerald-500/10 transition-colors text-xs"
                                                                                },
                                                                                disabled: already_in_playlist,
                                                                                onclick: {
                                                                                    let song = res.clone();
                                                                                    move |evt: MouseEvent| {
                                                                                        evt.stop_propagation();
                                                                                        on_add_song(song.clone());
                                                                                    }
                                                                                },
                                                                                if already_in_playlist {
                                                                                    "In Playlist"
                                                                                } else {
                                                                                    "Add"
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                    button {
                                                                        class: "w-7 h-7 rounded-full border border-zinc-700 text-zinc-500 hover:text-zinc-200 hover:border-zinc-500 transition-colors flex items-center justify-center",
                                                                        title: "Dismiss recommendation",
                                                                        onclick: {
                                                                            let mut dismissed_recommendations =
                                                                                dismissed_recommendations.clone();
                                                                            let recommendation_key = song_identity_key(&res);
                                                                            move |evt: MouseEvent| {
                                                                                evt.stop_propagation();
                                                                                dismissed_recommendations.with_mut(|dismissed| {
                                                                                    dismissed.insert(recommendation_key.clone());
                                                                                });
                                                                            }
                                                                        },
                                                                        Icon { name: "x".to_string(), class: "w-3 h-3".to_string() }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    div { class: "pt-1 flex justify-end",
                                        button {
                                            class: "px-3 py-1 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-xs",
                                            onclick: on_refresh_recommendations,
                                            "Refresh recommendations"
                                        }
                                    }
                                }
                                if song_search().trim().len() < 2 {
                                    p { class: "text-sm text-zinc-500", "Type at least 2 characters to search for additional songs." }
                                } else if let Some(results) = search_results() {
                                    if results.is_empty() {
                                        p { class: "text-sm text-zinc-500", "No songs found." }
                                    } else {
                                        p { class: "text-xs uppercase tracking-wide text-zinc-500", "Search Results" }
                                        div { class: "space-y-2 max-h-64 overflow-y-auto pr-1",
                                            for res in results {
                                                div {
                                                    key: "{res.server_id}:{res.id}:search",
                                                    class: "flex items-center justify-between gap-3 p-2 rounded-lg hover:bg-zinc-800/50 transition-colors",
                                                    {
                                                        let cover_url = servers()
                                                            .iter()
                                                            .find(|s| s.id == res.server_id)
                                                            .and_then(|server| {
                                                                let client = NavidromeClient::new(server.clone());
                                                                res.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 80))
                                                            });
                                                        let cover_album_id = res.album_id.clone();
                                                        let cover_server_id = res.server_id.clone();
                                                        let navigation_for_cover = navigation.clone();
                                                        rsx! {
                                                            if let Some(url) = cover_url {
                                                                if let Some(album_id) = cover_album_id {
                                                                    button {
                                                                        class: "w-10 h-10 rounded overflow-hidden border border-zinc-800/80 flex-shrink-0",
                                                                        aria_label: "Open album",
                                                                        onclick: {
                                                                            let album_id = album_id.clone();
                                                                            let server_id = cover_server_id.clone();
                                                                            let navigation = navigation_for_cover.clone();
                                                                            move |evt: MouseEvent| {
                                                                                evt.stop_propagation();
                                                                                navigation.navigate_to(AppView::AlbumDetailView {
                                                                                    album_id: album_id.clone(),
                                                                                    server_id: server_id.clone(),
                                                                                });
                                                                            }
                                                                        },
                                                                        img {
                                                                            class: "w-full h-full object-cover",
                                                                            src: "{url}",
                                                                        }
                                                                    }
                                                                } else {
                                                                    img {
                                                                        class: "w-10 h-10 rounded object-cover border border-zinc-800/80",
                                                                        src: "{url}",
                                                                    }
                                                                }
                                                            } else {
                                                                div { class: "w-10 h-10 rounded bg-zinc-800 flex items-center justify-center border border-zinc-800/80",
                                                                    Icon {
                                                                        name: "music".to_string(),
                                                                        class: "w-4 h-4 text-zinc-500".to_string(),
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    div { class: "min-w-0 flex-1",
                                                        p { class: "text-sm text-white truncate", "{res.title}" }
                                                        p { class: "text-xs text-zinc-500 truncate",
                                                            "{res.artist.clone().unwrap_or_default()} • {res.album.clone().unwrap_or_default()}"
                                                        }
                                                    }
                                                    {
                                                        let already_in_playlist = song_list()
                                                            .iter()
                                                            .any(|existing| same_song_identity(existing, &res));
                                                        rsx! {
                                                            div { class: "flex items-center gap-2",
                                                                button {
                                                                    class: if preview_song_key()
                                                                        == Some(song_identity_key(&res))
                                                                    {
                                                                        "px-3 py-1 rounded-lg border border-zinc-700 text-zinc-500 text-xs cursor-not-allowed"
                                                                    } else {
                                                                        "px-3 py-1 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-xs"
                                                                    },
                                                                    title: "Play a short preview, then return to your current song",
                                                                    disabled: preview_song_key()
                                                                        == Some(song_identity_key(&res)),
                                                                    onclick: {
                                                                        let song = res.clone();
                                                                        let on_preview_song = on_preview_song.clone();
                                                                        move |evt: MouseEvent| {
                                                                            evt.stop_propagation();
                                                                            on_preview_song(song.clone());
                                                                        }
                                                                    },
                                                                    if preview_song_key()
                                                                        == Some(song_identity_key(&res))
                                                                    {
                                                                        "Previewing..."
                                                                    } else {
                                                                        "Preview"
                                                                    }
                                                                }
                                                                button {
                                                                    class: if already_in_playlist {
                                                                        "px-3 py-1 rounded-lg border border-zinc-700 text-zinc-500 text-xs cursor-not-allowed"
                                                                    } else {
                                                                        "px-3 py-1 rounded-lg border border-emerald-500/60 text-emerald-300 hover:text-white hover:bg-emerald-500/10 transition-colors text-xs"
                                                                    },
                                                                    disabled: already_in_playlist,
                                                                    onclick: {
                                                                        let song = res.clone();
                                                                        move |evt: MouseEvent| {
                                                                            evt.stop_propagation();
                                                                            on_add_song(song.clone());
                                                                        }
                                                                    },
                                                                    if already_in_playlist {
                                                                        "In Playlist"
                                                                    } else {
                                                                        "Add"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Some(None) => rsx! {
                    div { class: "flex flex-col items-center justify-center py-20",
                        Icon {
                            name: "playlist".to_string(),
                            class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                        }
                        p { class: "text-zinc-400", "Playlist not found" }
                    }
                },
                None => rsx! {
                    div { class: "flex items-center justify-center py-20",
                        Icon {
                            name: "loader".to_string(),
                            class: "w-8 h-8 text-zinc-500".to_string(),
                        }
                    }
                },
            }
            if show_delete_confirm() {
                div { class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
                    div { class: "bg-zinc-900 border border-zinc-700 rounded-lg p-6 max-w-md w-full mx-4",
                        h2 { class: "text-xl font-bold text-white mb-4", "Delete Playlist" }
                        p { class: "text-zinc-300 mb-6",
                            "Are you sure you want to delete this playlist? This action cannot be undone."
                        }
                        div { class: "flex gap-3 justify-end",
                            button {
                                class: "px-4 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-zinc-500 transition-colors",
                                onclick: move |_| show_delete_confirm.set(false),
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 rounded-lg bg-red-600 hover:bg-red-500 text-white transition-colors",
                                onclick: on_confirm_delete,
                                "Delete"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn same_song_identity(left: &Song, right: &Song) -> bool {
    left.id == right.id && left.server_id == right.server_id
}

async fn search_playlist_add_candidates(server: Option<ServerConfig>, query: String) -> Vec<Song> {
    let total_start = PerfTimer::now();
    let normalized_query = query.trim().to_string();
    if normalized_query.len() < 2 {
        return Vec::new();
    }

    let Some(server) = server else {
        return Vec::new();
    };

    let server_name = server.name.clone();
    let cache_key = format!(
        "search:playlist_add:v1:{}:{}",
        server.id,
        normalized_query.to_lowercase()
    );
    if let Some(cached) = cache_get_json::<Vec<Song>>(&cache_key) {
        log_perf(
            "playlist.search.cache_hit",
            total_start,
            &format!(
                "server={} query={} results={}",
                server_name,
                normalized_query,
                cached.len()
            ),
        );
        return cached;
    }

    let client = NavidromeClient::new(server);
    let output = client
        .search(&normalized_query, 0, 0, 25)
        .await
        .map(|res| res.songs)
        .unwrap_or_default();
    let _ = cache_put_json(cache_key, &output, Some(4));

    log_perf(
        "playlist.search",
        total_start,
        &format!(
            "server={} query={} results={}",
            server_name,
            normalized_query,
            output.len()
        ),
    );

    output
}

fn song_identity_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
}

async fn fetch_similar_playlist_candidates(
    server: &ServerConfig,
    seed: &Song,
    count: usize,
) -> Vec<Song> {
    if count == 0 {
        return Vec::new();
    }

    let client = NavidromeClient::new(server.clone());
    let lookup_count = (count as u32).saturating_mul(4).max(count as u32);
    let mut similar = client
        .get_similar_songs(&seed.id, lookup_count)
        .await
        .unwrap_or_default();

    if similar.is_empty() {
        similar = client
            .get_similar_songs2(&seed.id, lookup_count)
            .await
            .unwrap_or_default();
    }

    if similar.is_empty() {
        similar = client
            .get_random_songs((count as u32).saturating_mul(6).max(24))
            .await
            .unwrap_or_default();
    }

    let seed_key = song_identity_key(seed);
    let mut dedup = HashSet::<String>::new();
    let mut output = Vec::<Song>::new();
    for song in similar {
        let key = song_identity_key(&song);
        if key == seed_key || !dedup.insert(key) {
            continue;
        }
        output.push(song);
        if output.len() >= count {
            break;
        }
    }
    output
}

async fn build_playlist_add_recommendations(
    server: Option<ServerConfig>,
    playlist_songs: Vec<Song>,
    recent_seed: Option<Song>,
    dismissed_keys: HashSet<String>,
) -> Vec<Song> {
    let Some(server) = server else {
        return Vec::new();
    };

    let first_seed = playlist_songs.first().cloned();
    let last_seed = playlist_songs.last().cloned();
    let mut seed_specs = Vec::<(Song, usize)>::new();
    if let Some(seed) = first_seed {
        seed_specs.push((seed, AUTO_RECOMMENDATION_FIRST_SEED_COUNT));
    }
    if let Some(seed) = last_seed {
        seed_specs.push((seed, AUTO_RECOMMENDATION_LAST_SEED_COUNT));
    }
    if let Some(seed) = recent_seed {
        seed_specs.push((seed, AUTO_RECOMMENDATION_RECENT_SEED_COUNT));
    }

    let mut excluded = HashSet::<String>::new();
    for song in &playlist_songs {
        excluded.insert(song_identity_key(song));
    }
    for key in dismissed_keys {
        excluded.insert(key);
    }

    let mut used_seed_keys = HashSet::<String>::new();
    let mut suggestions = Vec::<Song>::new();
    for (seed, count) in seed_specs {
        let seed_key = song_identity_key(&seed);
        if !used_seed_keys.insert(seed_key) {
            continue;
        }
        for candidate in fetch_similar_playlist_candidates(&server, &seed, count).await {
            let key = song_identity_key(&candidate);
            if excluded.insert(key) {
                suggestions.push(candidate);
                if suggestions.len() >= AUTO_RECOMMENDATION_LIMIT {
                    return suggestions;
                }
            }
        }
    }

    if suggestions.len() >= AUTO_RECOMMENDATION_LIMIT {
        return suggestions;
    }

    let client = NavidromeClient::new(server);
    let random = client.get_random_songs(40).await.unwrap_or_default();
    for candidate in random {
        let key = song_identity_key(&candidate);
        if excluded.insert(key) {
            suggestions.push(candidate);
            if suggestions.len() >= AUTO_RECOMMENDATION_LIMIT {
                break;
            }
        }
    }

    suggestions
}
