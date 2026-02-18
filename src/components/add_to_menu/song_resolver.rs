// Server fetch helpers for resolving target songs and suggestion seeds.

async fn resolve_target_songs(
    servers: &[ServerConfig],
    target: &AddTarget,
) -> Result<Vec<Song>, String> {
    match target {
        AddTarget::Song(song) => Ok(vec![song.clone()]),
        AddTarget::Songs(songs) => Ok(songs.clone()),
        AddTarget::Album {
            album_id,
            server_id,
            ..
        } => {
            let Some(server) = servers.iter().find(|s| s.id == *server_id).cloned() else {
                return Err("Server is not available for this album.".to_string());
            };
            let client = NavidromeClient::new(server);
            client
                .get_album(album_id)
                .await
                .map(|(_, songs)| songs)
                .map_err(|err| format!("Failed to load album: {err}"))
        }
        AddTarget::Playlist {
            playlist_id,
            server_id,
            ..
        } => {
            let Some(server) = servers.iter().find(|s| s.id == *server_id).cloned() else {
                return Err("Server is not available for this playlist.".to_string());
            };
            let client = NavidromeClient::new(server);
            client
                .get_playlist(playlist_id)
                .await
                .map(|(_, songs)| songs)
                .map_err(|err| format!("Failed to load playlist: {err}"))
        }
    }
}

async fn fetch_similar_songs_for_seed(
    servers: &[ServerConfig],
    seed: &Song,
    count: usize,
) -> Vec<Song> {
    if count == 0 {
        return Vec::new();
    }

    let Some(server) = servers.iter().find(|s| s.id == seed.server_id).cloned() else {
        return Vec::new();
    };

    let client = NavidromeClient::new(server);
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
            .get_random_songs((count as u32).saturating_mul(6).max(20))
            .await
            .unwrap_or_default();
    }

    let seed_key = song_key(seed);
    let mut seen = HashSet::<String>::new();
    let mut output = Vec::<Song>::new();
    for song in similar {
        let key = song_key(&song);
        if key == seed_key {
            continue;
        }
        if seen.insert(key) {
            output.push(song);
        }
        if output.len() >= count {
            break;
        }
    }

    output
}

async fn build_dual_seed_suggestions(
    servers: &[ServerConfig],
    first_seed: Option<Song>,
    recent_seed: Option<Song>,
) -> Vec<Song> {
    let mut suggestions = Vec::<Song>::new();
    let mut seen = HashSet::<String>::new();

    if let Some(seed) = first_seed {
        for song in fetch_similar_songs_for_seed(servers, &seed, 4).await {
            let key = song_key(&song);
            if seen.insert(key) {
                suggestions.push(song);
            }
        }
    }

    if let Some(seed) = recent_seed {
        for song in fetch_similar_songs_for_seed(servers, &seed, 4).await {
            let key = song_key(&song);
            if seen.insert(key) {
                suggestions.push(song);
            }
        }
    }

    suggestions.truncate(8);
    suggestions
}
