// Song metadata and transport controls panel split into setup and RSX chunks.
#[derive(Props, Clone, PartialEq)]
struct DetailsPanelProps {
    song: Song,
    cover_url: Option<String>,
}

#[component]
fn DetailsPanel(props: DetailsPanelProps) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let add_menu = use_context::<AddMenuController>();
    let navigation = use_context::<Navigation>();
    let controller = use_context::<SongDetailsController>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<Signal<bool>>();
    let repeat_mode = use_context::<Signal<RepeatMode>>();
    let volume = use_context::<VolumeSignal>().0;
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let audio_state = use_context::<Signal<AudioState>>();
    let mut rating_open = use_signal(|| false);

    let now_playing_song = now_playing();
    let queue_snapshot = queue();
    let is_selected_song_now_playing = now_playing_song
        .as_ref()
        .map(|current| current.id == props.song.id && current.server_id == props.song.server_id)
        .unwrap_or(false);
    let is_selected_song_favorited = queue_snapshot
        .iter()
        .find(|entry| entry.id == props.song.id && entry.server_id == props.song.server_id)
        .map(|song| song.starred.is_some())
        .or_else(|| {
            now_playing_song
                .as_ref()
                .filter(|song| song.id == props.song.id && song.server_id == props.song.server_id)
                .map(|song| song.starred.is_some())
        })
        .unwrap_or(props.song.starred.is_some());
    let is_live_stream = is_live_song(&props.song);
    let currently_playing = is_playing();
    let current_repeat_mode = repeat_mode();
    let queue_len = queue_snapshot.len();
    let can_prev = !is_live_stream && queue_index() > 0;
    let can_next = !is_live_stream
        && (queue_index().saturating_add(1) < queue_len
            || (current_repeat_mode == RepeatMode::All && queue_len > 0)
            || current_repeat_mode == RepeatMode::Off
            || (current_repeat_mode == RepeatMode::One && now_playing_song.is_some()));
    let now_playing_rating = now_playing_song
        .as_ref()
        .and_then(|song| song.user_rating)
        .unwrap_or(0)
        .min(5);
    let current_time = (audio_state().current_time)();
    let duration = (audio_state().duration)();
    let display_duration = if duration > 0.0 {
        duration
    } else {
        now_playing_song
            .as_ref()
            .map(|song| song.duration as f64)
            .unwrap_or(0.0)
    };
    let playback_percent = if display_duration > 0.0 {
        ((current_time / display_duration) * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    let song_artist = props
        .song
        .artist
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Unknown Artist".to_string());
    let song_album = props
        .song
        .album
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Unknown Album".to_string());

    let on_open_artist = {
        let mut controller = controller.clone();
        let navigation = navigation.clone();
        let artist_id = props.song.artist_id.clone();
        let server_id = props.song.server_id.clone();
        move |_| {
            if let Some(artist_id) = artist_id.clone() {
                controller.close();
                navigation.navigate_to(AppView::ArtistDetailView {
                    artist_id,
                    server_id: server_id.clone(),
                });
            }
        }
    };

    let on_open_album = {
        let mut controller = controller.clone();
        let navigation = navigation.clone();
        let album_id = props.song.album_id.clone();
        let server_id = props.song.server_id.clone();
        move |_| {
            if let Some(album_id) = album_id.clone() {
                controller.close();
                navigation.navigate_to(AppView::AlbumDetailView {
                    album_id,
                    server_id: server_id.clone(),
                });
            }
        }
    };

    let on_open_album_cover = {
        let mut controller = controller.clone();
        let navigation = navigation.clone();
        let album_id = props.song.album_id.clone();
        let server_id = props.song.server_id.clone();
        move |_| {
            if let Some(album_id) = album_id.clone() {
                controller.close();
                navigation.navigate_to(AppView::AlbumDetailView {
                    album_id,
                    server_id: server_id.clone(),
                });
            }
        }
    };

    let on_toggle_selected_playback = {
        let song = props.song.clone();
        let mut queue = queue.clone();
        let mut queue_index = queue_index.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let is_selected_song_now_playing = is_selected_song_now_playing;
        move |_| {
            if is_selected_song_now_playing {
                is_playing.set(!is_playing());
                return;
            }
            let song_for_queue = song.clone();
            let mut found_index = None;
            queue.with_mut(|items| {
                if let Some(existing_index) = items.iter().position(|entry| {
                    entry.id == song_for_queue.id && entry.server_id == song_for_queue.server_id
                }) {
                    found_index = Some(existing_index);
                } else {
                    items.push(song_for_queue.clone());
                    found_index = Some(items.len().saturating_sub(1));
                }
            });

            if let Some(index) = found_index {
                queue_index.set(index);
                now_playing.set(Some(song.clone()));
                is_playing.set(true);
            }
        }
    };

    let on_prev_song = {
        let mut queue_index = queue_index.clone();
        let queue = queue.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let is_live_stream = is_live_stream;
        move |_| {
            if is_live_stream {
                return;
            }
            let idx = queue_index();
            if idx == 0 {
                return;
            }
            let next_idx = idx - 1;
            if let Some(song) = queue().get(next_idx).cloned() {
                queue_index.set(next_idx);
                now_playing.set(Some(song));
                is_playing.set(true);
            }
        }
    };

    let on_next_song = {
        let servers = servers.clone();
        let mut queue_index = queue_index.clone();
        let queue = queue.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let repeat_mode = repeat_mode.clone();
        let seed_song = props.song.clone();
        let is_live_stream = is_live_stream;
        move |_| {
            if is_live_stream {
                return;
            }
            let was_playing = is_playing();
            let repeat = repeat_mode();
            if repeat == RepeatMode::One {
                seek_to(0.0);
                if was_playing {
                    is_playing.set(true);
                }
                return;
            }
            let idx = queue_index();
            let next_idx = idx.saturating_add(1);
            let queue_list = queue();
            if repeat == RepeatMode::Off
                && (queue_list.is_empty() || idx >= queue_list.len().saturating_sub(1))
            {
                let seed = now_playing().or(Some(seed_song.clone()));
                spawn_shuffle_queue(
                    servers(),
                    queue.clone(),
                    queue_index.clone(),
                    now_playing.clone(),
                    is_playing.clone(),
                    seed,
                    Some(was_playing),
                );
                return;
            }
            if let Some(song) = queue_list.get(next_idx).cloned() {
                queue_index.set(next_idx);
                now_playing.set(Some(song));
                is_playing.set(true);
            } else if repeat == RepeatMode::All && !queue_list.is_empty() {
                if let Some(song) = queue_list.first().cloned() {
                    queue_index.set(0);
                    now_playing.set(Some(song));
                    is_playing.set(true);
                }
            }
        }
    };

    let on_seek_now_playing = {
        let mut playback_position = playback_position.clone();
        let mut audio_state = audio_state.clone();
        move |evt: Event<FormData>| {
            let Ok(value) = evt.value().parse::<f64>() else {
                return;
            };
            let duration = (audio_state().duration)();
            if duration <= 0.0 {
                return;
            }
            let target = (value.clamp(0.0, 100.0) / 100.0) * duration;
            playback_position.set(target);
            audio_state.write().current_time.set(target);
            seek_to(target);
        }
    };

    let on_volume_change = {
        let mut volume = volume.clone();
        move |evt: Event<FormData>| {
            if let Ok(value) = evt.value().parse::<f64>() {
                volume.set((value / 100.0).clamp(0.0, 1.0));
            }
        }
    };

    let on_toggle_song_favorite = {
        let song = props.song.clone();
        let servers = servers.clone();
        let now_playing = now_playing.clone();
        let queue = queue.clone();
        let should_star = !is_selected_song_favorited;
        move |_| {
            toggle_song_favorite(
                song.clone(),
                should_star,
                servers.clone(),
                now_playing.clone(),
                queue.clone(),
            );
        }
    };

    let on_add_to_playlist = {
        let mut add_menu = add_menu.clone();
        let song = props.song.clone();
        move |_| {
            add_menu.open(AddIntent::from_song(song.clone()));
        }
    };
    let on_cycle_loop = {
        let mut repeat_mode = repeat_mode.clone();
        move |_| {
            let next = match repeat_mode() {
                RepeatMode::One => RepeatMode::Off,
                RepeatMode::Off | RepeatMode::All => RepeatMode::One,
            };
            repeat_mode.set(next);
        }
    };
    let on_set_now_playing_rating = {
        let servers = servers.clone();
        let now_playing = now_playing.clone();
        let queue = queue.clone();
        let mut rating_open = rating_open.clone();
        move |rating: u32| {
            set_now_playing_rating(servers.clone(), now_playing.clone(), queue.clone(), rating);
            rating_open.set(false);
        }
    };


    include!("details_panel_view.rs")
}
