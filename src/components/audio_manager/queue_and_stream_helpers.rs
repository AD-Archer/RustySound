// Shared queue reshuffle, stream URL resolution, and scrobble helper utilities.
#[cfg(target_arch = "wasm32")]
pub(crate) fn spawn_shuffle_queue(
    servers: Vec<ServerConfig>,
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    mut is_playing: Signal<bool>,
    seed_song: Option<Song>,
    play_state: Option<bool>,
) {
    let active_servers: Vec<ServerConfig> = servers.into_iter().filter(|s| s.active).collect();
    if active_servers.is_empty() {
        return;
    }

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
            return;
        }

        let len = songs.len();
        for i in (1..len).rev() {
            let j = (js_sys::Math::random() * ((i + 1) as f64)) as usize;
            songs.swap(i, j);
        }
        songs.truncate(50);

        let first = songs.get(0).cloned();
        defer_signal_update(move || {
            queue.set(songs);
            queue_index.set(0);
            now_playing.set(first);
            if let Some(play_state) = play_state {
                is_playing.set(play_state);
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
    seed_song: Option<Song>,
    play_state: Option<bool>,
) {
    let active_servers: Vec<ServerConfig> = servers.into_iter().filter(|s| s.active).collect();
    if active_servers.is_empty() {
        return;
    }

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
            return;
        }

        let mut rng = rand::thread_rng();
        songs.shuffle(&mut rng);
        songs.truncate(50);

        let first = songs.first().cloned();
        queue.set(songs);
        queue_index.set(0);
        now_playing.set(first);
        if let Some(play_state) = play_state {
            is_playing.set(play_state);
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

