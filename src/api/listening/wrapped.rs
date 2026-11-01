//! Wrapped: one year of listening, worked out from what the servers keep.
//! Pure; `history.rs` loads the input.
//!
//! # The approximation
//! Navidrome, other Subsonic servers, Jellyfin and the device's own index
//! keep two things per song: a lifetime play count and the last time it was
//! played. None of them keeps a date for every play. So a year here is "the
//! songs whose last play falls in that year", and they count with their
//! lifetime plays:
//! - Rankings (top songs, artists, albums, genres) and minutes use lifetime
//!   plays, so a song played this year also brings the plays it had before.
//! - A song played again later counts only in the later year, which makes
//!   past years look smaller than they were.
//! - The clock, busiest day and streak use one timestamp per song (its last
//!   play), so they show at least that much: the real streak is no shorter.
//! The UI says so where it matters ("songs played this year", "about").

use super::aggregate::Tally;
use crate::api::discovery::{normalize_name, parse_timestamp};
use crate::api::{Album, Song};
use chrono::{Datelike, NaiveDate, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Items in each top list.
pub const TOP_N: usize = 5;
/// Some servers report the epoch for "never"; plays before 2000 are unknown.
const EARLIEST_PLAY: i64 = 946_684_800;

/// One thing the server says was listened to: a song or, from servers
/// without song history, a whole album, with its lifetime plays and the
/// last time it was played.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Listen {
    /// The song itself (for playing it); `None` for an album.
    pub song: Option<Song>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_id: Option<String>,
    pub genre: Option<String>,
    pub cover_art: Option<String>,
    pub server_id: String,
    /// Seconds one play lasts: the song's length, or an album's average track.
    pub seconds: u32,
    /// Lifetime plays (at least 1: it has a last play).
    pub plays: u32,
    /// Epoch seconds.
    pub last_played: i64,
}

fn last_play(raw: Option<&str>) -> Option<i64> {
    raw.and_then(parse_timestamp).filter(|ts| *ts > EARLIEST_PLAY)
}

fn non_empty(value: Option<&String>) -> Option<String> {
    value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

impl Listen {
    /// `None` when the song has never been played (or the date is unknown).
    pub fn from_song(song: &Song) -> Option<Self> {
        let last_played = last_play(song.played.as_deref())?;
        Some(Self {
            title: song.title.trim().to_string(),
            artist: non_empty(song.artist.as_ref()).unwrap_or_default(),
            album: non_empty(song.album.as_ref()).unwrap_or_default(),
            album_id: non_empty(song.album_id.as_ref()),
            genre: non_empty(song.genre.as_ref()),
            cover_art: non_empty(song.cover_art.as_ref()),
            server_id: song.server_id.clone(),
            seconds: song.duration,
            plays: song.play_count.unwrap_or(0).max(1),
            last_played,
            song: Some(song.clone()),
        })
    }

    /// An album from a server without song history (`getAlbumList2`).
    pub fn from_album(album: &Album) -> Option<Self> {
        let last_played = last_play(album.played.as_deref())?;
        Some(Self {
            title: album.name.trim().to_string(),
            artist: album.artist.trim().to_string(),
            album: album.name.trim().to_string(),
            album_id: Some(album.id.clone()),
            genre: non_empty(album.genre.as_ref()),
            cover_art: non_empty(album.cover_art.as_ref()),
            server_id: album.server_id.clone(),
            seconds: album.duration / album.song_count.max(1),
            plays: album.play_count.unwrap_or(0).max(1),
            last_played,
            song: None,
        })
    }

    fn minutes(&self) -> u64 {
        u64::from(self.plays) * u64::from(self.seconds) / 60
    }

    fn album_key(&self) -> Option<String> {
        self.album_id.as_ref().map(|id| format!("{}::{id}", self.server_id))
    }
}

/// An artist, song, album or genre in a top list.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Ranked {
    pub name: String,
    /// The artist, for songs and albums.
    pub by: Option<String>,
    pub plays: u64,
    pub minutes: u64,
    /// Songs (or albums) counted in.
    pub items: usize,
    /// Artwork from its most played song.
    pub cover_art: Option<String>,
    pub server_id: String,
    /// The song, for songs (Quick Listen, play).
    pub song: Option<Song>,
}

impl Ranked {
    fn from_listen(listen: &Listen, name: &str, by: Option<&str>) -> Self {
        Self {
            name: name.to_string(),
            by: by.filter(|b| !b.is_empty()).map(str::to_string),
            cover_art: listen.cover_art.clone(),
            server_id: listen.server_id.clone(),
            ..Self::default()
        }
    }

    fn count(&mut self, listen: &Listen) {
        self.plays += u64::from(listen.plays);
        self.minutes += listen.minutes();
        self.items += 1;
    }
}

/// The longest run of days in a row with a (last) play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Streak {
    pub days: u32,
    pub start: NaiveDate,
    pub end: NaiveDate,
}

/// The busiest single day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusyDay {
    pub date: NaiveDate,
    pub count: u32,
}

/// Part of the day, in local time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DayPart {
    /// 5:00–11:59
    Morning,
    /// 12:00–16:59
    Afternoon,
    /// 17:00–21:59
    Evening,
    /// 22:00–4:59
    Night,
}

impl DayPart {
    pub fn of(hour: usize) -> Self {
        match hour {
            5..=11 => Self::Morning,
            12..=16 => Self::Afternoon,
            17..=21 => Self::Evening,
            _ => Self::Night,
        }
    }
}

/// A one-line read of the year. The UI words it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Personality {
    /// One artist had a big share of the year.
    Devoted { artist: String },
    NightOwl,
    EarlyBird,
    /// Lots of new artists.
    Explorer { new_artists: usize },
    /// Many genres, none dominant.
    Eclectic { genres: usize },
    /// Plays gathered on a few albums.
    AlbumListener,
    Steady,
}

/// Everything the Wrapped cards show for one year.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WrappedYear {
    pub year: i32,
    /// The current year: "so far".
    pub so_far: bool,
    /// Songs (or albums) last played this year.
    pub listens: usize,
    pub plays: u64,
    pub minutes: u64,
    /// The year before, on the same basis, when it has any listening.
    pub previous_minutes: Option<u64>,
    pub artist_count: usize,
    pub top_artists: Vec<Ranked>,
    /// Empty when the servers only report albums.
    pub top_songs: Vec<Ranked>,
    pub top_albums: Vec<Ranked>,
    pub top_genres: Vec<Ranked>,
    pub genre_count: usize,
    /// Last plays per local hour.
    pub hours: [u32; 24],
    pub busiest_day: Option<BusyDay>,
    /// 0 = Monday.
    pub busiest_weekday: Option<u32>,
    pub streak: Option<Streak>,
    /// Artists whose music joined the library this year, most played first.
    pub new_artists: Vec<Ranked>,
    pub new_artist_count: usize,
    pub personality: Personality,
}

impl WrappedYear {
    pub fn is_empty(&self) -> bool {
        self.listens == 0
    }

    /// The part of the day with the most plays.
    pub fn peak_part(&self) -> Option<DayPart> {
        let mut parts: Vec<(DayPart, u32)> = Vec::new();
        for (hour, count) in self.hours.iter().enumerate() {
            let part = DayPart::of(hour);
            match parts.iter_mut().find(|(p, _)| *p == part) {
                Some(slot) => slot.1 += count,
                None => parts.push((part, *count)),
            }
        }
        parts.into_iter().filter(|(_, c)| *c > 0).max_by_key(|(_, c)| *c).map(|(p, _)| p)
    }

    /// The share of plays (0–1) that went to the top artist.
    pub fn top_artist_share(&self) -> f64 {
        share(self.top_artists.first().map(|a| a.plays), self.plays)
    }
}

fn share(part: Option<u64>, total: u64) -> f64 {
    match (part, total) {
        (Some(part), total) if total > 0 => part as f64 / total as f64,
        _ => 0.0,
    }
}

/// Epoch seconds of the start of `year` and of the next one, in `tz`.
pub fn year_bounds<Tz: TimeZone>(year: i32, tz: &Tz) -> (i64, i64) {
    let start = |y: i32| {
        tz.with_ymd_and_hms(y, 1, 1, 0, 0, 0)
            .earliest()
            .map(|t| t.timestamp())
            .unwrap_or(i64::MAX)
    };
    (start(year), start(year + 1))
}

/// Years with any listening, newest first.
pub fn years_with_data<Tz: TimeZone>(listens: &[Listen], tz: &Tz) -> Vec<i32> {
    let mut years: Vec<i32> = listens
        .iter()
        .filter_map(|l| tz.timestamp_opt(l.last_played, 0).single())
        .map(|t| t.year())
        .collect();
    years.sort_unstable_by(|a, b| b.cmp(a));
    years.dedup();
    years
}

/// The longest run of consecutive days. `dates` must be sorted and unique.
pub fn longest_streak(dates: &[NaiveDate]) -> Option<Streak> {
    let first = *dates.first()?;
    let mut best = Streak { days: 1, start: first, end: first };
    let mut current = best;
    for pair in dates.windows(2) {
        if pair[0].succ_opt() == Some(pair[1]) {
            current.days += 1;
            current.end = pair[1];
        } else {
            current = Streak { days: 1, start: pair[1], end: pair[1] };
        }
        if current.days > best.days {
            best = current;
        }
    }
    Some(best)
}

fn artist_key(name: &str) -> Option<String> {
    let key = normalize_name(name);
    (!key.is_empty() && !matches!(key.as_str(), "unknownartist" | "unknown")).then_some(key)
}

/// Works out one year. `added_albums` holds `server::album` keys of albums
/// added to the library that year (for new artists); `now` decides "so far".
pub fn compute_year<Tz: TimeZone>(
    listens: &[Listen],
    year: i32,
    tz: &Tz,
    now: i64,
    added_albums: &HashSet<String>,
) -> WrappedYear {
    let (start, end) = year_bounds(year, tz);
    let mut picked: Vec<&Listen> =
        listens.iter().filter(|l| l.last_played >= start && l.last_played < end).collect();
    // Most played first, so each tally's first item is its best one.
    picked.sort_by(|a, b| b.plays.cmp(&a.plays).then(b.last_played.cmp(&a.last_played)));
    let heard_before: HashSet<String> = listens
        .iter()
        .filter(|l| l.last_played < start)
        .filter_map(|l| artist_key(&l.artist))
        .collect();

    let mut artists: Tally<Ranked> = Tally::default();
    let mut albums: Tally<Ranked> = Tally::default();
    let mut genres: Tally<Ranked> = Tally::default();
    // Per artist: is every album of theirs heard this year a new one?
    let mut all_new: HashMap<String, bool> = HashMap::new();
    let mut hours = [0u32; 24];
    let mut per_day: HashMap<NaiveDate, u32> = HashMap::new();
    let mut weekdays = [0u32; 7];
    let mut plays = 0u64;
    let mut minutes = 0u64;

    for listen in &picked {
        plays += u64::from(listen.plays);
        minutes += listen.minutes();
        if let Some(key) = artist_key(&listen.artist) {
            artists.add(key.clone(), u64::from(listen.plays), || Ranked::from_listen(listen, &listen.artist, None)).count(listen);
            let added = listen.album_key().is_some_and(|k| added_albums.contains(&k));
            let slot = all_new.entry(key).or_insert(true);
            *slot = *slot && added;
        }
        if !listen.album.is_empty() {
            let key = format!("{}|{}", normalize_name(&listen.album), normalize_name(&listen.artist));
            albums
                .add(key, u64::from(listen.plays), || Ranked::from_listen(listen, &listen.album, Some(&listen.artist)))
                .count(listen);
        }
        if let Some(genre) = &listen.genre {
            let key = normalize_name(genre);
            if !key.is_empty() {
                genres.add(key, u64::from(listen.plays), || Ranked::from_listen(listen, genre, None)).count(listen);
            }
        }
        if let Some(local) = tz.timestamp_opt(listen.last_played, 0).single() {
            hours[local.hour() as usize] += 1;
            *per_day.entry(local.date_naive()).or_insert(0) += 1;
            weekdays[local.weekday().num_days_from_monday() as usize] += 1;
        }
    }

    let top_songs: Vec<Ranked> = picked
        .iter()
        .filter(|l| l.song.is_some())
        .take(TOP_N)
        .map(|l| {
            let mut ranked = Ranked::from_listen(l, &l.title, Some(&l.artist));
            ranked.count(l);
            ranked.song = l.song.clone();
            ranked
        })
        .collect();

    let artist_count = artists.len();
    let genre_count = genres.len();
    let ranked_artists: Vec<Ranked> = artists.ranked().into_iter().map(|(a, _)| a).collect();
    let new_artists: Vec<Ranked> = ranked_artists
        .iter()
        .filter(|a| {
            artist_key(&a.name).is_some_and(|key| {
                !heard_before.contains(&key) && all_new.get(&key).copied().unwrap_or(false)
            })
        })
        .cloned()
        .collect();
    let mut dates: Vec<NaiveDate> = per_day.keys().copied().collect();
    dates.sort_unstable();
    let busiest_day = dates
        .iter()
        .map(|d| BusyDay { date: *d, count: per_day[d] })
        .fold(None::<BusyDay>, |best, day| match best {
            Some(b) if b.count >= day.count => Some(b),
            _ => Some(day),
        });
    let busiest_weekday = weekdays
        .iter()
        .enumerate()
        .filter(|(_, c)| **c > 0)
        .fold(None::<(usize, u32)>, |best, (i, c)| match best {
            Some((_, b)) if b >= *c => best,
            _ => Some((i, *c)),
        })
        .map(|(i, _)| i as u32);
    let now_year = tz.timestamp_opt(now, 0).single().map(|t| t.year());

    let mut wrapped = WrappedYear {
        year,
        so_far: now_year == Some(year),
        listens: picked.len(),
        plays,
        minutes,
        previous_minutes: None,
        artist_count,
        top_artists: ranked_artists.into_iter().take(TOP_N).collect(),
        top_songs,
        top_albums: albums.ranked().into_iter().map(|(a, _)| a).take(TOP_N).collect(),
        top_genres: genres.ranked().into_iter().map(|(g, _)| g).take(TOP_N).collect(),
        genre_count,
        hours,
        busiest_day,
        busiest_weekday,
        streak: longest_streak(&dates),
        new_artist_count: new_artists.len(),
        new_artists: new_artists.into_iter().take(TOP_N).collect(),
        personality: Personality::Steady,
    };
    wrapped.personality = personality(&wrapped);
    wrapped
}

/// The first reading that fits, most distinctive first.
pub fn personality(year: &WrappedYear) -> Personality {
    let hours_total: u32 = year.hours.iter().sum();
    let part_share = |part: DayPart| {
        let count: u32 = year.hours.iter().enumerate().filter(|(h, _)| DayPart::of(*h) == part).map(|(_, c)| c).sum();
        share(Some(u64::from(count)), u64::from(hours_total))
    };
    let album_share = share(Some(year.top_albums.iter().map(|a| a.plays).sum()), year.plays);
    let top_genre_share = share(year.top_genres.first().map(|g| g.plays), year.plays);
    match year.top_artists.first() {
        Some(top) if year.artist_count >= 3 && year.top_artist_share() >= 0.3 => {
            return Personality::Devoted { artist: top.name.clone() }
        }
        _ => {}
    }
    if hours_total >= 10 && part_share(DayPart::Night) >= 0.35 {
        Personality::NightOwl
    } else if hours_total >= 10 && part_share(DayPart::Morning) >= 0.4 {
        Personality::EarlyBird
    } else if year.new_artist_count >= 5 && year.new_artist_count * 4 >= year.artist_count {
        Personality::Explorer { new_artists: year.new_artist_count }
    } else if year.genre_count >= 5 && top_genre_share < 0.3 {
        Personality::Eclectic { genres: year.genre_count }
    } else if year.top_albums.len() == TOP_N && album_share >= 0.5 {
        Personality::AlbumListener
    } else {
        Personality::Steady
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    const HOUR: i64 = 3_600;
    const DAY: i64 = 86_400;

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    /// Epoch seconds for a UTC date and hour.
    fn at(y: i32, m: u32, d: u32, h: u32) -> i64 {
        utc().with_ymd_and_hms(y, m, d, h, 0, 0).unwrap().timestamp()
    }

    fn iso(ts: i64) -> String {
        chrono::DateTime::from_timestamp(ts, 0).unwrap().to_rfc3339()
    }

    fn song(id: &str, artist: &str, album: &str, plays: u32, played: Option<i64>) -> Song {
        Song {
            id: id.into(),
            title: format!("Song {id}"),
            artist: Some(artist.into()),
            album: Some(album.into()),
            album_id: Some(format!("al-{album}")),
            genre: Some("Folk".into()),
            duration: 240,
            play_count: Some(plays),
            played: played.map(iso),
            server_id: "s1".into(),
            ..Default::default()
        }
    }

    fn listens(songs: &[Song]) -> Vec<Listen> {
        songs.iter().filter_map(Listen::from_song).collect()
    }

    fn year(listens: &[Listen], y: i32) -> WrappedYear {
        compute_year(listens, y, &utc(), at(2026, 11, 1, 12), &HashSet::new())
    }

    #[test]
    fn listens_need_a_real_last_play() {
        assert!(Listen::from_song(&song("1", "A", "X", 5, None)).is_none());
        let mut epoch = song("2", "A", "X", 5, None);
        epoch.played = Some("1970-01-01T00:00:00Z".into());
        assert!(Listen::from_song(&epoch).is_none());
        // A last play means at least one play, even if the count says 0.
        let zero = Listen::from_song(&song("3", "A", "X", 0, Some(at(2026, 3, 1, 9)))).unwrap();
        assert_eq!(zero.plays, 1);
    }

    #[test]
    fn a_year_is_the_songs_last_played_in_it() {
        let all = listens(&[
            song("new-year", "A", "X", 3, Some(at(2026, 1, 1, 0))),
            song("eve", "A", "X", 4, Some(at(2025, 12, 31, 23))),
            song("summer", "B", "Y", 2, Some(at(2026, 7, 4, 15))),
            song("next", "B", "Y", 9, Some(at(2027, 1, 1, 0))),
        ]);
        let y2026 = year(&all, 2026);
        assert_eq!(y2026.listens, 2);
        assert!(y2026.so_far);
        let y2025 = year(&all, 2025);
        assert_eq!((y2025.listens, y2025.so_far), (1, false));
        assert_eq!(years_with_data(&all, &utc()), vec![2027, 2026, 2025]);
    }

    #[test]
    fn year_bounds_follow_the_time_zone() {
        let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
        let (start, end) = year_bounds(2026, &tokyo);
        assert_eq!(start, at(2025, 12, 31, 15));
        assert_eq!(end - start, 365 * DAY);
        // 2025-12-31 16:00 UTC is already 2026 in Tokyo.
        let late = listens(&[song("1", "A", "X", 1, Some(at(2025, 12, 31, 16)))]);
        assert_eq!(compute_year(&late, 2026, &tokyo, 0, &HashSet::new()).listens, 1);
        assert_eq!(compute_year(&late, 2026, &utc(), 0, &HashSet::new()).listens, 0);
    }

    #[test]
    fn minutes_count_lifetime_plays_of_this_years_songs() {
        let all = listens(&[
            song("1", "A", "X", 10, Some(at(2026, 2, 1, 9))), // 10 × 4 min
            song("2", "A", "X", 5, Some(at(2026, 2, 2, 9))),  // 5 × 4 min
            song("old", "A", "X", 50, Some(at(2024, 2, 2, 9))),
        ]);
        let y = year(&all, 2026);
        assert_eq!(y.plays, 15);
        assert_eq!(y.minutes, 60);
    }

    #[test]
    fn top_lists_rank_by_plays_and_merge_names() {
        let all = listens(&[
            song("1", "Mira Okafor", "Tide", 25, Some(at(2026, 2, 1, 9))),
            song("2", "Junie Marlow", "Lanterns", 30, Some(at(2026, 2, 1, 10))),
            song("3", "mira okafor", "Tide", 10, Some(at(2026, 3, 1, 9))),
            song("4", "Pell Avenue", "Rooms", 1, Some(at(2026, 4, 1, 9))),
            song("5", "Ash Corley", "Dust", 2, Some(at(2026, 4, 2, 9))),
            song("6", "Bo Lind", "Sea", 3, Some(at(2026, 4, 3, 9))),
            song("7", "Cy Vale", "Glass", 4, Some(at(2026, 4, 4, 9))),
        ]);
        let y = year(&all, 2026);
        let names: Vec<&str> = y.top_artists.iter().map(|a| a.name.as_str()).collect();
        // The first-seen spelling of the most played song names the artist.
        assert_eq!(names, vec!["Mira Okafor", "Junie Marlow", "Cy Vale", "Bo Lind", "Ash Corley"]);
        assert_eq!(y.top_artists.len(), TOP_N);
        assert_eq!(y.artist_count, 6);
        assert_eq!((y.top_artists[0].plays, y.top_artists[0].items), (35, 2));
        assert_eq!(y.top_artists[0].minutes, 35 * 4);
        let songs: Vec<&str> = y.top_songs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(songs, vec!["Song 2", "Song 1", "Song 3", "Song 7", "Song 6"]);
        assert_eq!(y.top_songs[0].by.as_deref(), Some("Junie Marlow"));
        assert!(y.top_songs[0].song.is_some());
        assert_eq!(y.top_albums[0].name, "Tide");
        assert_eq!(y.top_albums[0].plays, 35);
        assert_eq!(y.top_genres[0].name, "Folk");
        assert_eq!(y.genre_count, 1);
    }

    #[test]
    fn albums_only_servers_have_no_top_songs() {
        let album = Album {
            id: "al1".into(),
            name: "Tide".into(),
            artist: "Mira Okafor".into(),
            song_count: 10,
            duration: 2_400,
            play_count: Some(12),
            played: Some(iso(at(2026, 5, 1, 20))),
            server_id: "s1".into(),
            ..Default::default()
        };
        let all = vec![Listen::from_album(&album).unwrap()];
        let y = year(&all, 2026);
        assert!(y.top_songs.is_empty());
        assert_eq!(y.top_albums[0].name, "Tide");
        // 12 plays of an average 4-minute track.
        assert_eq!(y.minutes, 48);
    }

    #[test]
    fn the_clock_buckets_last_plays_by_local_hour() {
        let all = listens(&[
            song("1", "A", "X", 9, Some(at(2026, 6, 1, 23))),
            song("2", "A", "X", 1, Some(at(2026, 6, 2, 23))),
            song("3", "A", "X", 1, Some(at(2026, 6, 3, 1))),
            song("4", "A", "X", 1, Some(at(2026, 6, 3, 14))),
        ]);
        let y = year(&all, 2026);
        // One per song, not weighted by plays.
        assert_eq!(y.hours[23], 2);
        assert_eq!(y.hours[1], 1);
        assert_eq!(y.hours[14], 1);
        assert_eq!(y.peak_part(), Some(DayPart::Night));
        // Two hours east, 23:00 UTC is 1:00 the next day.
        let east = FixedOffset::east_opt(2 * 3600).unwrap();
        let shifted = compute_year(&all, 2026, &east, 0, &HashSet::new());
        assert_eq!(shifted.hours[1], 2);
        assert_eq!(shifted.hours[3], 1);
        assert_eq!(DayPart::of(5), DayPart::Morning);
        assert_eq!(DayPart::of(12), DayPart::Afternoon);
        assert_eq!(DayPart::of(21), DayPart::Evening);
        assert_eq!(DayPart::of(4), DayPart::Night);
    }

    #[test]
    fn busiest_day_and_weekday() {
        let all = listens(&[
            song("1", "A", "X", 1, Some(at(2026, 3, 14, 9))),
            song("2", "A", "X", 1, Some(at(2026, 3, 14, 10))),
            song("3", "A", "X", 1, Some(at(2026, 3, 14, 11))),
            song("4", "A", "X", 1, Some(at(2026, 3, 2, 9))),
        ]);
        let y = year(&all, 2026);
        let day = y.busiest_day.unwrap();
        assert_eq!((day.date, day.count), (NaiveDate::from_ymd_opt(2026, 3, 14).unwrap(), 3));
        assert_eq!(y.busiest_weekday, Some(5), "2026-03-14 is a Saturday");
    }

    #[test]
    fn streaks_find_the_longest_run() {
        let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        assert_eq!(longest_streak(&[]), None);
        assert_eq!(longest_streak(&[d(1, 5)]).unwrap().days, 1);
        let dates = [d(1, 1), d(1, 2), d(1, 4), d(1, 5), d(1, 6), d(2, 27), d(2, 28), d(3, 1), d(3, 2)];
        let best = longest_streak(&dates).unwrap();
        // Across a month end; the later, longer run wins.
        assert_eq!((best.days, best.start, best.end), (4, d(2, 27), d(3, 2)));
        // A tie keeps the first.
        let tie = longest_streak(&[d(1, 1), d(1, 2), d(1, 9), d(1, 10)]).unwrap();
        assert_eq!(tie.start, d(1, 1));
    }

    #[test]
    fn streaks_use_local_dates() {
        let all = listens(&[
            song("1", "A", "X", 1, Some(at(2026, 5, 1, 22))),
            song("2", "A", "X", 1, Some(at(2026, 5, 2, 22) + HOUR)),
            song("3", "A", "X", 1, Some(at(2026, 5, 4, 1))),
        ]);
        assert_eq!(year(&all, 2026).streak.unwrap().days, 2);
        // At UTC+3 those are May 2, 3 and 4.
        let east = FixedOffset::east_opt(3 * 3600).unwrap();
        assert_eq!(compute_year(&all, 2026, &east, 0, &HashSet::new()).streak.unwrap().days, 3);
    }

    #[test]
    fn new_artists_joined_the_library_this_year() {
        let all = listens(&[
            song("1", "Fresh Face", "Debut", 4, Some(at(2026, 5, 1, 9))),
            song("2", "Old Friend", "Comeback", 9, Some(at(2026, 5, 1, 9))),
            song("3", "Old Friend", "Classic", 2, Some(at(2023, 5, 1, 9))),
            song("4", "Mixed Bag", "New One", 3, Some(at(2026, 5, 1, 9))),
            song("5", "Mixed Bag", "Old One", 3, Some(at(2026, 5, 2, 9))),
        ]);
        let added: HashSet<String> =
            ["s1::al-Debut", "s1::al-Comeback", "s1::al-New One"].iter().map(|s| s.to_string()).collect();
        let y = compute_year(&all, 2026, &utc(), 0, &added);
        // Old Friend was heard before 2026; Mixed Bag has an older album too.
        let names: Vec<&str> = y.new_artists.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["Fresh Face"]);
        assert_eq!(y.new_artist_count, 1);
        // Without knowing what was added, nobody is new.
        assert_eq!(year(&all, 2026).new_artist_count, 0);
    }

    #[test]
    fn personality_readings() {
        let devoted = listens(&[
            song("1", "Mira", "X", 50, Some(at(2026, 5, 1, 15))),
            song("2", "Bo", "Y", 5, Some(at(2026, 5, 1, 15))),
            song("3", "Cy", "Z", 5, Some(at(2026, 5, 1, 15))),
        ]);
        assert_eq!(year(&devoted, 2026).personality, Personality::Devoted { artist: "Mira".into() });

        let nights: Vec<Song> = (0..12)
            .map(|i| song(&i.to_string(), &format!("Artist {i}"), &format!("Al {i}"), 1, Some(at(2026, 5, 1 + i, 23))))
            .collect();
        assert_eq!(year(&listens(&nights), 2026).personality, Personality::NightOwl);

        let afternoons: Vec<Song> = (0..12)
            .map(|i| song(&i.to_string(), &format!("Artist {i}"), &format!("Al {i}"), 1, Some(at(2026, 5, 1 + i, 15))))
            .collect();
        let mut y = year(&listens(&afternoons), 2026);
        assert_eq!(y.personality, Personality::Steady);
        y.new_artist_count = 6;
        assert_eq!(personality(&y), Personality::Explorer { new_artists: 6 });
    }
}
