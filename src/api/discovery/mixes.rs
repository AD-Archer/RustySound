//! Mix generators. Each takes data already fetched from the server and returns
//! `None` when there isn't enough real data for an honest mix. Pure.

use super::library::ArtistSeed;
use super::reasons::{freshness_label, join_names, length_label, similar_reason};
use super::rewind::{cutoff_days_for_months, is_rewind_candidate, DEFAULT_REWIND_MONTHS, MIN_SONG_PLAYS};
use super::types::{CoverRef, DiscoverSong, Mix, MixKind, ReasonKind};
use super::util::{normalize_name, SeededRng};
use crate::api::models::{Album, Song};
use std::collections::{HashMap, HashSet};

/// A mix needs at least this many songs to be shown.
pub const MIN_MIX_SONGS: usize = 8;
/// Songs with this many plays or more are "played a lot" (excluded from Discovery).
pub const HEAVY_PLAYS: u32 = 5;
/// Target sizes.
pub const DISCOVERY_SIZE: usize = 25;
pub const DEEP_CUTS_SIZE: usize = 30;
pub const REWIND_SIZE: usize = 40;
pub const RADIO_SIZE: usize = 30;
/// Home's "Songs you might like" shelf.
pub const SONGS_LIKE_SIZE: usize = 20;

/// Inputs shared by every generator for one day.
#[derive(Debug, Clone)]
pub struct MixContext {
    /// Local date (YYYY-MM-DD) the mixes are for.
    pub date: String,
    /// Today's date, for the freshness label.
    pub today: String,
    /// Base seed for the day (date + refresh salt).
    pub seed: u64,
    /// Epoch seconds "now".
    pub now: i64,
}

impl MixContext {
    fn rng(&self, scope: &str) -> SeededRng {
        SeededRng::new(self.seed ^ super::util::fnv1a(scope))
    }
}

fn song_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
}

fn dedupe(songs: Vec<Song>) -> Vec<Song> {
    let mut seen = HashSet::new();
    songs.into_iter().filter(|s| seen.insert(song_key(s))).collect()
}

/// Keep at most `cap` songs per artist, preserving order.
fn cap_per_artist(songs: Vec<Song>, cap: usize) -> Vec<Song> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    songs
        .into_iter()
        .filter(|song| {
            let key = normalize_name(song.artist.as_deref().unwrap_or(""));
            let count = counts.entry(key).or_insert(0);
            *count += 1;
            *count <= cap
        })
        .collect()
}

/// Up to 4 distinct covers, one per album, in song order.
pub fn mosaic_covers(songs: &[Song]) -> Vec<CoverRef> {
    let mut seen_albums = HashSet::new();
    let mut covers = Vec::new();
    for song in songs {
        let Some(cover) = song.cover_art.as_ref().filter(|c| !c.is_empty()) else {
            continue;
        };
        let album_key = song.album_id.clone().unwrap_or_else(|| cover.clone());
        if !seen_albums.insert(format!("{}::{album_key}", song.server_id)) {
            continue;
        }
        covers.push(CoverRef {
            server_id: song.server_id.clone(),
            cover_art: cover.clone(),
        });
        if covers.len() == 4 {
            break;
        }
    }
    covers
}

/// Fill in the derived fields of a mix. `None` below [`MIN_MIX_SONGS`].
pub fn finish_mix(
    ctx: &MixContext,
    id: String,
    kind: MixKind,
    name: String,
    blurb: String,
    songs: Vec<Song>,
) -> Option<Mix> {
    if songs.len() < MIN_MIX_SONGS {
        return None;
    }
    let total_seconds: u64 = songs.iter().map(|s| u64::from(s.duration)).sum();
    Some(Mix {
        id,
        kind,
        name,
        blurb,
        covers: mosaic_covers(&songs),
        song_count: songs.len(),
        total_seconds,
        length_label: length_label(total_seconds),
        freshness: freshness_label(&ctx.date, &ctx.today),
        generated_on: ctx.date.clone(),
        songs,
    })
}

/// Discovery Mix: songs similar to the seed artists, without the seeds' own
/// songs (unless needed to reach the minimum) and without songs played
/// [`HEAVY_PLAYS`]+ times. ≤ 2 songs per artist.
pub fn build_discovery_mix(
    ctx: &MixContext,
    seeds: &[ArtistSeed],
    similar_per_seed: Vec<Vec<Song>>,
) -> Option<Mix> {
    let seed_names: HashSet<String> = seeds.iter().map(|s| normalize_name(&s.name)).collect();
    // Round-robin across seeds so one seed can't dominate.
    let mut merged = Vec::new();
    let longest = similar_per_seed.iter().map(Vec::len).max().unwrap_or(0);
    for i in 0..longest {
        for list in &similar_per_seed {
            if let Some(song) = list.get(i) {
                merged.push(song.clone());
            }
        }
    }
    let fresh: Vec<Song> = dedupe(merged)
        .into_iter()
        .filter(|s| s.play_count.unwrap_or(0) < HEAVY_PLAYS)
        .collect();
    let (others, by_seeds): (Vec<Song>, Vec<Song>) = fresh
        .into_iter()
        .partition(|s| !seed_names.contains(&normalize_name(s.artist.as_deref().unwrap_or(""))));
    let mut songs = cap_per_artist(others, 2);
    if songs.len() < MIN_MIX_SONGS {
        songs.extend(cap_per_artist(by_seeds, 2));
    }
    let mut rng = ctx.rng("discovery");
    rng.shuffle(&mut songs);
    songs.truncate(DISCOVERY_SIZE);

    let names: Vec<String> = seeds.iter().take(2).map(|s| s.name.clone()).collect();
    let blurb = format!("{} songs like {}", songs.len(), join_names(&names));
    finish_mix(
        ctx,
        "discovery".to_string(),
        MixKind::Discovery,
        "Discovery Mix".to_string(),
        blurb,
        songs,
    )
}

/// Deep Cuts: tracks from often-played albums that have far fewer plays than
/// the album's favourite track (≤ 1 play, or ≤ 25 % of the top track).
pub fn build_deep_cuts(ctx: &MixContext, albums: Vec<(Album, Vec<Song>)>) -> Option<Mix> {
    let mut per_album: Vec<Vec<Song>> = Vec::new();
    for (_album, songs) in albums {
        if songs.len() < 4 {
            continue;
        }
        let top = songs.iter().map(|s| s.play_count.unwrap_or(0)).max().unwrap_or(0);
        if top == 0 {
            continue; // No play data at all: can't tell a deep cut from a hit.
        }
        let threshold = (top / 4).max(1);
        let mut cuts: Vec<Song> = songs
            .into_iter()
            .filter(|s| s.play_count.unwrap_or(0) <= threshold)
            .collect();
        ctx.rng(&format!("deep-cuts:{}", per_album.len())).shuffle(&mut cuts);
        per_album.push(cuts);
    }
    let longest = per_album.iter().map(Vec::len).max().unwrap_or(0);
    let mut songs = Vec::new();
    for i in 0..longest {
        for list in &per_album {
            if let Some(song) = list.get(i) {
                songs.push(song.clone());
            }
        }
    }
    let mut songs = dedupe(songs);
    songs.truncate(DEEP_CUTS_SIZE);
    finish_mix(
        ctx,
        "deep-cuts".to_string(),
        MixKind::DeepCuts,
        "Deep Cuts".to_string(),
        "Rarely played tracks from albums you love".to_string(),
        songs,
    )
}

/// Rewind mix: most-played songs not played in 6+ months, shuffled by seed.
/// Always uses the default 6-month window, so "For You" doesn't change when the
/// user picks another window in the Rewind segment.
pub fn build_rewind_mix(ctx: &MixContext, top_songs: &[Song]) -> Option<Mix> {
    let cutoff_days = cutoff_days_for_months(DEFAULT_REWIND_MONTHS);
    let mut songs: Vec<Song> = top_songs
        .iter()
        .filter(|s| {
            is_rewind_candidate(s.played.as_deref(), s.play_count, MIN_SONG_PLAYS, ctx.now, cutoff_days)
        })
        .cloned()
        .collect();
    songs.sort_by_key(|s| std::cmp::Reverse(s.play_count.unwrap_or(0)));
    let mut songs = dedupe(songs);
    songs.truncate(REWIND_SIZE);
    ctx.rng("rewind").shuffle(&mut songs);
    finish_mix(
        ctx,
        "rewind".to_string(),
        MixKind::Rewind,
        "Rewind".to_string(),
        "Your most-played songs you haven't heard in 6+ months".to_string(),
        songs,
    )
}

/// "<Artist> Radio": the artist plus similar artists (server `getSimilarSongs2`).
pub fn build_artist_radio(ctx: &MixContext, seed: &ArtistSeed, songs: Vec<Song>) -> Option<Mix> {
    let mut songs = cap_per_artist(dedupe(songs), 6);
    songs.truncate(RADIO_SIZE);
    finish_mix(
        ctx,
        format!("radio:{}:{}", seed.server_id, seed.artist_id),
        MixKind::ArtistRadio,
        format!("{} Radio", seed.name),
        format!("{} and similar artists", seed.name),
        songs,
    )
}

/// "Songs you might like": songs from each seed's similar-songs list, minus
/// the seed artists' own songs and songs already played [`HEAVY_PLAYS`]+
/// times; ≤ 2 per artist, round-robin across seeds so each gets a turn, then
/// shuffled with the day's seed. Each song says which artist it came through.
pub fn select_songs_like(
    ctx: &MixContext,
    lists: &[(ArtistSeed, Vec<Song>)],
    limit: usize,
) -> Vec<DiscoverSong> {
    let seed_names: HashSet<String> = lists.iter().map(|(s, _)| normalize_name(&s.name)).collect();
    let longest = lists.iter().map(|(_, l)| l.len()).max().unwrap_or(0);
    let mut seen = HashSet::new();
    let mut per_artist: HashMap<String, usize> = HashMap::new();
    let mut picks = Vec::new();
    for i in 0..longest {
        for (seed, list) in lists {
            let Some(song) = list.get(i) else {
                continue;
            };
            let artist = normalize_name(song.artist.as_deref().unwrap_or(""));
            if song.play_count.unwrap_or(0) >= HEAVY_PLAYS
                || seed_names.contains(&artist)
                || !seen.insert(song_key(song))
            {
                continue;
            }
            let count = per_artist.entry(artist).or_insert(0);
            if *count >= 2 {
                continue;
            }
            *count += 1;
            picks.push(DiscoverSong {
                song: song.clone(),
                reason: similar_reason(&seed.name),
                kind: ReasonKind::SimilarTo,
                because_artist: Some(seed.name.clone()),
            });
        }
    }
    ctx.rng("songs-like").shuffle(&mut picks);
    picks.truncate(limit);
    picks
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;
    const NOW: i64 = 1_793_232_000;

    fn ctx(seed: u64) -> MixContext {
        MixContext {
            date: "2026-10-29".into(),
            today: "2026-10-29".into(),
            seed,
            now: NOW,
        }
    }

    fn song(id: &str, artist: &str, plays: u32) -> Song {
        Song {
            id: id.into(),
            title: format!("Song {id}"),
            artist: Some(artist.into()),
            album_id: Some(format!("al-{artist}")),
            cover_art: Some(format!("al-{artist}")),
            duration: 200,
            play_count: Some(plays),
            server_id: "s1".into(),
            ..Default::default()
        }
    }

    fn seed(name: &str) -> ArtistSeed {
        ArtistSeed {
            server_id: "s1".into(),
            artist_id: format!("ar-{name}"),
            name: name.into(),
            plays: 100,
        }
    }

    fn similar_lists() -> Vec<Vec<Song>> {
        let artists = ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J"];
        let list1 = (0..20)
            .map(|i| song(&format!("x{i}"), artists[i % 10], (i % 7) as u32))
            .collect();
        let list2 = (0..20)
            .map(|i| song(&format!("y{i}"), artists[(i + 3) % 10], 0))
            .chain(std::iter::once(song("seed-song", "Mira Okafor", 0)))
            .collect();
        vec![list1, list2]
    }

    #[test]
    fn discovery_mix_is_deterministic_and_honest() {
        let seeds = vec![seed("Mira Okafor"), seed("Junie Marlow")];
        let a = build_discovery_mix(&ctx(1), &seeds, similar_lists()).unwrap();
        let b = build_discovery_mix(&ctx(1), &seeds, similar_lists()).unwrap();
        assert_eq!(a, b, "same seed + data ⇒ same mix");
        let c = build_discovery_mix(&ctx(2), &seeds, similar_lists()).unwrap();
        assert_ne!(
            a.songs.iter().map(|s| &s.id).collect::<Vec<_>>(),
            c.songs.iter().map(|s| &s.id).collect::<Vec<_>>(),
            "a refresh (new salt) reshuffles"
        );
        assert!(a.songs.iter().all(|s| s.play_count.unwrap() < HEAVY_PLAYS));
        assert!(a.songs.iter().all(|s| s.artist.as_deref() != Some("Mira Okafor")));
        assert_eq!(a.blurb, format!("{} songs like Mira Okafor and Junie Marlow", a.song_count));
        assert_eq!(a.freshness, "Updated today");
        assert!(a.covers.len() <= 4 && !a.covers.is_empty());
        let mut counts = HashMap::new();
        for s in &a.songs {
            *counts.entry(s.artist.clone()).or_insert(0) += 1;
        }
        assert!(counts.values().all(|&n| n <= 2));
    }

    #[test]
    fn no_mix_without_enough_data() {
        let seeds = vec![seed("Mira Okafor")];
        let few = vec![vec![song("1", "A", 0), song("2", "B", 0)]];
        assert!(build_discovery_mix(&ctx(1), &seeds, few).is_none());
        assert!(build_rewind_mix(&ctx(1), &[]).is_none());
        assert!(build_deep_cuts(&ctx(1), vec![]).is_none());
    }

    #[test]
    fn deep_cuts_skip_the_hits() {
        let albums: Vec<(Album, Vec<Song>)> = (0..3)
            .map(|a| {
                let mut songs: Vec<Song> =
                    (0..6).map(|t| song(&format!("{a}-{t}"), &format!("Art{a}"), 0)).collect();
                songs[0].play_count = Some(80); // the hit
                songs[1].play_count = Some(40); // well-liked, above 25 %
                (Album::default(), songs)
            })
            .collect();
        let mix = build_deep_cuts(&ctx(3), albums).unwrap();
        assert_eq!(mix.song_count, 12);
        assert!(mix.songs.iter().all(|s| s.play_count.unwrap() == 0));
    }

    #[test]
    fn rewind_mix_uses_cutoff() {
        let iso = |d: i64| chrono::DateTime::from_timestamp(NOW - d * DAY, 0).unwrap().to_rfc3339();
        let songs: Vec<Song> = (0..12)
            .map(|i| {
                let mut s = song(&format!("r{i}"), "A", 10 + i);
                s.played = Some(iso(if i < 9 { 400 } else { 20 }));
                s
            })
            .collect();
        let mix = build_rewind_mix(&ctx(4), &songs).unwrap();
        assert_eq!(mix.song_count, 9);
        assert_eq!(mix.length_label, "30 min");
    }

    #[test]
    fn songs_like_skip_seeds_heavy_plays_and_repeats() {
        let seeds = [seed("Mira Okafor"), seed("Junie Marlow")];
        let lists: Vec<(ArtistSeed, Vec<Song>)> = seeds.iter().cloned().zip(similar_lists()).collect();
        let picks = select_songs_like(&ctx(1), &lists, 50);
        assert!(!picks.is_empty());
        assert!(picks.iter().all(|p| p.song.play_count.unwrap() < HEAVY_PLAYS));
        assert!(picks.iter().all(|p| p.song.artist.as_deref() != Some("Mira Okafor")));
        let mut counts = HashMap::new();
        for p in &picks {
            *counts.entry(p.song.artist.clone()).or_insert(0) += 1;
        }
        assert!(counts.values().all(|&n| n <= 2), "≤ 2 per artist");
        let ids: HashSet<&str> = picks.iter().map(|p| p.song.id.as_str()).collect();
        assert_eq!(ids.len(), picks.len(), "no repeats");
        // The reason names the seed the song came through.
        let first_x = picks.iter().find(|p| p.song.id.starts_with('x')).unwrap();
        assert_eq!(first_x.reason, "Similar to Mira Okafor");
        assert_eq!(first_x.because_artist.as_deref(), Some("Mira Okafor"));
        assert_eq!(first_x.kind, ReasonKind::SimilarTo);
        // Deterministic for a seed, capped by the limit.
        assert_eq!(picks, select_songs_like(&ctx(1), &lists, 50));
        assert_eq!(select_songs_like(&ctx(1), &lists, 5).len(), 5);
        assert!(select_songs_like(&ctx(1), &[], 5).is_empty());
    }

    #[test]
    fn artist_radio_names() {
        let songs: Vec<Song> = (0..10).map(|i| song(&format!("z{i}"), "B", 0)).collect();
        let radio = build_artist_radio(&ctx(5), &seed("Mira Okafor"), songs.clone());
        assert!(radio.is_none(), "6-per-artist cap leaves too few songs");
        let varied: Vec<Song> = (0..10)
            .map(|i| song(&format!("z{i}"), if i % 2 == 0 { "B" } else { "C" }, 0))
            .collect();
        let radio = build_artist_radio(&ctx(5), &seed("Mira Okafor"), varied).unwrap();
        assert_eq!(radio.name, "Mira Okafor Radio");
        assert_eq!(radio.id, "radio:s1:ar-Mira Okafor");
    }
}
