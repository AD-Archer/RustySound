// Shared queue reshuffle, stream URL resolution, and scrobble helper utilities.
fn normalize_song_for_manual_queue(mut song: Song) -> Song {
    song.queue_meta = None;
    song
}

pub(crate) fn normalize_manual_queue_songs(songs: Vec<Song>) -> Vec<Song> {
    songs
        .into_iter()
        .map(normalize_song_for_manual_queue)
        .collect()
}

fn queue_extension_song_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
}

fn extend_unique_queue_candidates(
    candidates: Vec<Song>,
    excluded: &mut std::collections::HashSet<String>,
    additions: &mut Vec<Song>,
    limit: usize,
) {
    for candidate in candidates {
        let key = queue_extension_song_key(&candidate);
        if !excluded.insert(key) {
            continue;
        }
        additions.push(candidate);
        if additions.len() >= limit {
            break;
        }
    }
}

pub(crate) async fn generate_queue_extension_from_seed(
    servers: Vec<ServerConfig>,
    seed_song: Song,
    existing_queue: Vec<Song>,
    desired_additions: usize,
) -> Vec<Song> {
    let limit = desired_additions.clamp(1, 80);
    let mut active_servers: Vec<ServerConfig> =
        servers.iter().filter(|server| server.active).cloned().collect();
    if active_servers.is_empty() {
        active_servers = servers;
    }
    if active_servers.is_empty() {
        return Vec::new();
    }

    let mut excluded = std::collections::HashSet::<String>::new();
    for song in &existing_queue {
        excluded.insert(queue_extension_song_key(song));
    }
    excluded.insert(queue_extension_song_key(&seed_song));

    let mut additions = Vec::<Song>::new();
    let lookup_count = ((limit as u32).saturating_mul(4)).clamp(24, 120);
    if let Some(seed_server) = active_servers
        .iter()
        .find(|server| server.id == seed_song.server_id)
        .cloned()
    {
        let client = NavidromeClient::new(seed_server);
        if let Ok(similar) = client.get_similar_songs2(&seed_song.id, lookup_count).await {
            extend_unique_queue_candidates(similar, &mut excluded, &mut additions, limit);
        }
        if additions.len() < limit {
            if let Ok(similar) = client.get_similar_songs(&seed_song.id, lookup_count).await {
                extend_unique_queue_candidates(similar, &mut excluded, &mut additions, limit);
            }
        }
    }

    if additions.len() < limit {
        let random_batch = ((limit as u32).saturating_mul(2)).clamp(30, 120);
        for _pass in 0..2 {
            for server in active_servers.iter().cloned() {
                let client = NavidromeClient::new(server);
                if let Ok(random_songs) = client.get_random_songs(random_batch).await {
                    extend_unique_queue_candidates(
                        random_songs,
                        &mut excluded,
                        &mut additions,
                        limit,
                    );
                }
                if additions.len() >= limit {
                    break;
                }
            }
            if additions.len() >= limit {
                break;
            }
        }
    }

    additions.truncate(limit);
    normalize_manual_queue_songs(additions)
}

pub(crate) fn assign_collection_queue_meta(
    songs: Vec<Song>,
    source_kind: QueueSourceKind,
    source_id: String,
) -> Vec<Song> {
    let group_id = format!(
        "{}:{}:{}",
        source_id,
        queue_source_kind_tag(&source_kind),
        uuid::Uuid::new_v4()
    );

    songs
        .into_iter()
        .enumerate()
        .map(|(source_position, mut song)| {
            song.queue_meta = Some(QueueSongMeta {
                group_id: group_id.clone(),
                source_kind: source_kind.clone(),
                source_id: source_id.clone(),
                source_position,
            });
            song
        })
        .collect()
}

pub(crate) fn queue_should_generate_similar_on_end(
    queue_snapshot: &[Song],
    current_song: Option<&Song>,
    shuffle_enabled: bool,
) -> bool {
    if !shuffle_enabled {
        return false;
    }
    if current_song
        .map(|song| song.server_name == "Radio")
        .unwrap_or(false)
    {
        return false;
    }
    if queue_snapshot.len() != 1 {
        return false;
    }
    queue_snapshot[0].queue_meta.is_none()
}

pub(crate) fn apply_collection_shuffle_mode(
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    now_playing: Signal<Option<Song>>,
    shuffle_enabled: bool,
) -> bool {
    eprintln!(
        "[queue.shuffle] request enabled={} queue_len={}",
        shuffle_enabled,
        queue().len()
    );
    let queue_snapshot = queue();
    if queue_snapshot.len() < 2 {
        return false;
    }

    let now_snapshot = now_playing();
    let active_group_id = now_snapshot
        .as_ref()
        .and_then(|song| song.queue_meta.as_ref().map(|meta| meta.group_id.clone()))
        .or_else(|| {
            queue_snapshot
                .iter()
                .find_map(|song| song.queue_meta.as_ref().map(|meta| meta.group_id.clone()))
        });
    let Some(active_group_id) = active_group_id else {
        return false;
    };

    let mut reorder_positions = Vec::<usize>::new();
    let mut reorder_songs = Vec::<Song>::new();
    for (index, song) in queue_snapshot.iter().enumerate() {
        let belongs_to_group = song
            .queue_meta
            .as_ref()
            .map(|meta| meta.group_id == active_group_id)
            .unwrap_or(false);
        if belongs_to_group {
            reorder_positions.push(index);
            reorder_songs.push(song.clone());
        }
    }
    if reorder_positions.len() < 2 {
        return false;
    }

    let previous_current_song = now_snapshot.clone();
    let current_slot = previous_current_song.as_ref().and_then(|current_song| {
        reorder_songs.iter().position(|entry| {
            entry.id == current_song.id
                && entry.server_id == current_song.server_id
                && entry
                    .queue_meta
                    .as_ref()
                    .zip(current_song.queue_meta.as_ref())
                    .map(|(left, right)| {
                        left.group_id == right.group_id
                            && left.source_position == right.source_position
                            && left.source_id == right.source_id
                    })
                    .unwrap_or(false)
        })
    });

    let mut reordered_group = reorder_songs.clone();
    if shuffle_enabled {
        if let Some(slot) = current_slot {
            let pinned = reordered_group.remove(slot);
            shuffle_songs_in_place(&mut reordered_group);
            reordered_group.insert(slot, pinned);
        } else {
            shuffle_songs_in_place(&mut reordered_group);
        }
    } else {
        reordered_group.sort_by(|left, right| {
            let left_key = left
                .queue_meta
                .as_ref()
                .map(|meta| (meta.source_position, meta.source_id.clone()))
                .unwrap_or((usize::MAX, String::new()));
            let right_key = right
                .queue_meta
                .as_ref()
                .map(|meta| (meta.source_position, meta.source_id.clone()))
                .unwrap_or((usize::MAX, String::new()));
            left_key.cmp(&right_key)
        });
    }

    if reordered_group == reorder_songs {
        eprintln!("[queue.shuffle] no-op (already in requested order)");
        return false;
    }

    let mut rebuilt_queue = queue_snapshot.clone();
    for (slot, target_index) in reorder_positions.iter().enumerate() {
        if let Some(song) = reordered_group.get(slot).cloned() {
            rebuilt_queue[*target_index] = song;
        }
    }

    let previous_index = queue_index();
    let next_index = previous_current_song
        .as_ref()
        .and_then(|song| find_song_instance_index(&rebuilt_queue, song))
        .unwrap_or_else(|| previous_index.min(rebuilt_queue.len().saturating_sub(1)));

    queue.set(rebuilt_queue);
    queue_index.set(next_index);
    eprintln!(
        "[queue.shuffle] applied enabled={} group_size={} queue_index={}",
        shuffle_enabled,
        reorder_positions.len(),
        next_index
    );
    true
}

pub(crate) fn find_song_instance_index(queue: &[Song], target: &Song) -> Option<usize> {
    if let Some(target_meta) = target.queue_meta.as_ref() {
        let with_meta = queue.iter().position(|song| {
            song.queue_meta
                .as_ref()
                .map(|meta| {
                    meta.group_id == target_meta.group_id
                        && meta.source_position == target_meta.source_position
                        && meta.source_id == target_meta.source_id
                })
                .unwrap_or(false)
        });
        if with_meta.is_some() {
            return with_meta;
        }
    }

    queue
        .iter()
        .position(|song| song.id == target.id && song.server_id == target.server_id)
}

fn queue_source_kind_tag(source_kind: &QueueSourceKind) -> &'static str {
    match source_kind {
        QueueSourceKind::Album => "album",
        QueueSourceKind::Playlist => "playlist",
        QueueSourceKind::Favorites => "favorites",
        QueueSourceKind::RandomMix => "random_mix",
        QueueSourceKind::Artist => "artist",
    }
}

#[cfg(target_arch = "wasm32")]
fn shuffle_songs_in_place(songs: &mut [Song]) {
    let len = songs.len();
    if len <= 1 {
        return;
    }
    for i in (1..len).rev() {
        let j = (js_sys::Math::random() * ((i + 1) as f64)) as usize;
        songs.swap(i, j);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn shuffle_songs_in_place(songs: &mut [Song]) {
    let mut rng = rand::thread_rng();
    songs.shuffle(&mut rng);
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn spawn_shuffle_queue(
    servers: Vec<ServerConfig>,
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    mut is_playing: Signal<bool>,
    audio_state: Signal<AudioState>,
    seed_song: Option<Song>,
    play_state: Option<bool>,
) {
    let active_servers: Vec<ServerConfig> = servers.into_iter().filter(|s| s.active).collect();
    if active_servers.is_empty() {
        return;
    }

    set_transport_loading(audio_state, true, Some("Generating similar queue..."));
    spawn(async move {
        let mut songs = Vec::new();
        if let Some(seed) = seed_song {
            if let Some(server) = active_servers
                .iter()
                .find(|s| s.id == seed.server_id)
                .cloned()
            {
                let client = NavidromeClient::new(server);
                if let Ok(similar) = client.get_similar_songs(&seed.id, 50).await {
                    songs.extend(similar);
                }
            }
        }

        if songs.is_empty() {
            for server in active_servers.iter().cloned() {
                let client = NavidromeClient::new(server.clone());
                if let Ok(server_songs) = client.get_random_songs(25).await {
                    songs.extend(server_songs);
                }
            }
        }

        if songs.is_empty() {
            set_transport_loading(audio_state, false, None);
            return;
        }

        shuffle_songs_in_place(&mut songs);
        songs.truncate(50);
        songs = normalize_manual_queue_songs(songs);

        let first = songs.get(0).cloned();
        let continue_loading_for_song = first.is_some() && play_state.unwrap_or(false);
        defer_signal_update(move || {
            queue.set(songs);
            queue_index.set(0);
            now_playing.set(first);
            if let Some(play_state) = play_state {
                is_playing.set(play_state);
            }
            if continue_loading_for_song {
                set_transport_loading(audio_state, true, Some("Loading song..."));
            } else {
                set_transport_loading(audio_state, false, None);
            }
        });
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn spawn_shuffle_queue(
    servers: Vec<ServerConfig>,
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    mut is_playing: Signal<bool>,
    audio_state: Signal<AudioState>,
    seed_song: Option<Song>,
    play_state: Option<bool>,
) {
    let active_servers: Vec<ServerConfig> = servers.into_iter().filter(|s| s.active).collect();
    if active_servers.is_empty() {
        return;
    }

    set_transport_loading(audio_state, true, Some("Generating similar queue..."));
    spawn(async move {
        let mut songs = Vec::new();
        if let Some(seed) = seed_song {
            if let Some(server) = active_servers
                .iter()
                .find(|s| s.id == seed.server_id)
                .cloned()
            {
                let client = NavidromeClient::new(server);
                if let Ok(similar) = client.get_similar_songs(&seed.id, 50).await {
                    songs.extend(similar);
                }
            }
        }

        if songs.is_empty() {
            for server in active_servers.iter().cloned() {
                let client = NavidromeClient::new(server);
                if let Ok(server_songs) = client.get_random_songs(25).await {
                    songs.extend(server_songs);
                }
            }
        }

        if songs.is_empty() {
            set_transport_loading(audio_state, false, None);
            return;
        }

        shuffle_songs_in_place(&mut songs);
        songs.truncate(50);
        songs = normalize_manual_queue_songs(songs);

        let first = songs.first().cloned();
        let continue_loading_for_song = first.is_some() && play_state.unwrap_or(false);
        queue.set(songs);
        queue_index.set(0);
        now_playing.set(first);
        if let Some(play_state) = play_state {
            is_playing.set(play_state);
        }
        if continue_loading_for_song {
            set_transport_loading(audio_state, true, Some("Loading song..."));
        } else {
            set_transport_loading(audio_state, false, None);
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn resolve_stream_url(song: &Song, servers: &[ServerConfig]) -> Option<String> {
    if song.server_name == "Radio" {
        return song
            .stream_url
            .clone()
            .filter(|value| !value.trim().is_empty());
    }

    let song_id = song.id.trim();
    if song_id.is_empty() {
        return None;
    }

    servers
        .iter()
        .find(|s| s.id == song.server_id)
        .map(|server| {
            let client = NavidromeClient::new(server.clone());
            client.get_stream_url(song_id)
        })
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_stream_url(song: &Song, servers: &[ServerConfig], offline_mode: bool) -> Option<String> {
    if let Some(cached_url) = cached_audio_url(song) {
        return Some(cached_url);
    }

    if offline_mode {
        return None;
    }

    if song.server_name == "Radio" {
        return song
            .stream_url
            .clone()
            .filter(|value| !value.trim().is_empty());
    }

    let song_id = song.id.trim();
    if song_id.is_empty() {
        return None;
    }

    servers
        .iter()
        .find(|s| s.id == song.server_id)
        .map(|server| {
            let client = NavidromeClient::new(server.clone());
            client.get_stream_url(song_id)
        })
}

fn can_save_server_bookmark(song: &Song) -> bool {
    song.server_name != "Radio" && !song.id.trim().is_empty() && !song.server_id.trim().is_empty()
}

#[cfg(target_arch = "wasm32")]
fn scrobble_song(servers: &[ServerConfig], song: &Song, finished: bool) {
    let server = servers.iter().find(|s| s.id == song.server_id).cloned();
    if let Some(server) = server {
        let song_id = song.id.clone();
        spawn(async move {
            let client = NavidromeClient::new(server);
            let _ = client.scrobble(&song_id, finished).await;
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn scrobble_song(servers: &[ServerConfig], song: &Song, finished: bool) {
    let server = servers.iter().find(|s| s.id == song.server_id).cloned();
    if let Some(server) = server {
        let song_id = song.id.clone();
        spawn(async move {
            let client = NavidromeClient::new(server);
            let _ = client.scrobble(&song_id, finished).await;
        });
    }
}
