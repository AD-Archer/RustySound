//! Cover Flow of a song list (playlist and mix pages), shown in the stage
//! band above the list.
//!
//! One cover per run of consecutive songs that share artwork: a playlist made
//! of one album shows that cover once instead of ten times, while a mixed
//! playlist gets a cover per song. The caption names a song (the run's first,
//! or the playing one when it's inside the run), so tapping the centre cover
//! plays exactly what the caption says.
//!
//! Flipping marks that song in the list below (a tinted row) without
//! scrolling the page, so the flow never runs away from the finger; scrolling
//! the list never moves the flow. When a song from the list is playing, the
//! flow rests on it and follows each song change.

use crate::api::Song;
use crate::components::ui::{cover_art_src, CoverFlow, CoverFlowItem};
use crate::components::views::library::shared::{song_key, use_now_playing, use_servers};
use dioxus::prelude::*;
use std::collections::HashMap;

/// Cover edge in px: Home's phone size; a little under Home's Mac size so
/// the list still starts above the fold on a laptop.
const COVER_COMPACT: u32 = 148;
const COVER_WIDE: u32 = 184;

/// Where the flow rests when nothing from the list is playing: the third
/// cover, as on Home, so covers sit on both sides instead of the left half
/// of the stage standing empty. The caption names that song, so a tap plays
/// what it says.
fn resting_index(count: usize) -> usize {
    (count / 2).min(2)
}

/// Fewer distinct covers than this, and the band keeps its single artwork.
pub const MIN_FLOW_RUNS: usize = 3;

/// Consecutive songs that share artwork: `start..start + len` in the list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Run {
    pub start: usize,
    pub len: usize,
}

impl Run {
    fn contains(&self, index: usize) -> bool {
        index >= self.start && index < self.start + self.len
    }
}

/// What makes two neighbouring songs "the same cover": the artwork id, else
/// the album. Songs with neither never merge (each shows a blank cover).
fn art_key(song: &Song) -> Option<(&str, &str)> {
    let id = song
        .cover_art
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| song.album_id.as_deref().filter(|s| !s.trim().is_empty()))?;
    Some((song.server_id.as_str(), id))
}

/// Splits a song list into runs of shared artwork.
pub fn song_runs(songs: &[Song]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for (index, song) in songs.iter().enumerate() {
        let key = art_key(song);
        match runs.last_mut() {
            Some(run) if key.is_some() && art_key(&songs[run.start]) == key => run.len += 1,
            _ => runs.push(Run { start: index, len: 1 }),
        }
    }
    runs
}

/// Index of the run holding song `index`.
fn run_of(runs: &[Run], index: usize) -> Option<usize> {
    let at = runs.partition_point(|run| run.start + run.len <= index);
    runs.get(at).filter(|run| run.contains(index)).map(|_| at)
}

/// The song a run's cover stands for: the playing one when it's in the run,
/// otherwise the first.
fn lead_song(run: &Run, playing: Option<usize>) -> usize {
    playing.filter(|&i| run.contains(i)).unwrap_or(run.start)
}

fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Caption line 3: the album and where the song sits ("Glass Weather · 3 of 40",
/// or "… · 3–12 of 1,200" for a run).
fn position_line(song: &Song, run: &Run, total: usize) -> String {
    let place = if run.len > 1 {
        format!("{}–{} of {}", run.start + 1, run.start + run.len, grouped(total))
    } else {
        format!("{} of {}", run.start + 1, grouped(total))
    };
    match song.album.as_deref().filter(|a| !a.trim().is_empty()) {
        Some(album) => format!("{album} · {place}"),
        None => place,
    }
}

/// Runs of `songs`, recomputed only when the list changes.
pub fn use_song_runs(songs: Signal<Vec<Song>>) -> Memo<Vec<Run>> {
    use_memo(move || song_runs(&songs.read()))
}

/// Whether the band should show the flow: enough distinct covers, or, while
/// the songs are still arriving, a list long enough to probably have them.
pub fn wants_flow(runs: &[Run], loaded: bool, expected_songs: usize) -> bool {
    if loaded {
        runs.len() >= MIN_FLOW_RUNS
    } else {
        expected_songs >= MIN_FLOW_RUNS
    }
}

/// The flow for the stage band.
/// * `songs`, `runs`: the list and its [`use_song_runs`].
/// * `on_play`: the centre cover was tapped; plays song `index` of the list.
/// * `list_testid`: data-testid of the list below, whose row for the flipped
///   song gets a tint.
/// * `wide`: Mac size, with the scrubber. `label`: the carousel's name.
#[component]
pub fn SongFlow(
    songs: Signal<Vec<Song>>,
    runs: Memo<Vec<Run>>,
    on_play: EventHandler<usize>,
    list_testid: String,
    wide: bool,
    label: String,
) -> Element {
    let servers = use_servers();
    let (playing_key, _) = use_now_playing();
    let cover_px = if wide { COVER_WIDE } else { COVER_COMPACT };
    // The song the user flipped to (marked in the list). Only the small
    // highlight component reads it, so flipping doesn't re-render this one.
    let mut flipped = use_signal(|| None::<usize>);

    let playing_index = use_memo(move || {
        let key = playing_key()?;
        songs.read().iter().position(|song| song_key(song) == key)
    });
    let items = use_memo(move || {
        let songs = songs.read();
        let runs = runs.read();
        let servers = servers.read();
        let playing = playing_index();
        let total = songs.len();
        // One URL per distinct cover, however often it repeats.
        let mut urls: HashMap<(String, String), Option<String>> = HashMap::new();
        runs.iter()
            .map(|run| {
                let song = &songs[lead_song(run, playing)];
                let first = &songs[run.start];
                let art = art_key(first).and_then(|(server, id)| {
                    urls.entry((server.to_string(), id.to_string()))
                        .or_insert_with(|| {
                            let cover = first.cover_art.as_deref().or(first.album_id.as_deref());
                            cover_art_src(&servers, server, cover, cover_px * 2)
                        })
                        .clone()
                });
                CoverFlowItem {
                    key: format!("{}:{}", run.start, song_key(first)),
                    title: song.title.clone(),
                    subtitle: song.artist.clone().unwrap_or_default(),
                    reason: position_line(song, run, total),
                    art: art.into_iter().collect(),
                }
            })
            .collect::<Vec<_>>()
    });
    let focus = use_memo(move || playing_index().and_then(|index| run_of(&runs.read(), index)));
    // A song change moves the flow to the new song; the old mark would point
    // somewhere else, so it goes.
    use_effect(move || {
        let _ = focus();
        if flipped.peek().is_some() {
            flipped.set(None);
        }
    });

    let list = items();
    if list.is_empty() {
        return rsx! {
            div {
                class: "rs-songflow__skel",
                class: if wide { "rs-songflow__skel--wide" },
                style: "--cover:{cover_px}px",
                aria_busy: "true",
                aria_label: "Loading covers",
                div { class: "rs-songflow__skel-side" }
                div { class: "rs-songflow__skel-cover" }
                div { class: "rs-songflow__skel-side" }
            }
        };
    }
    let resting = focus();
    let count = list.len();

    rsx! {
        div { class: "rs-songflow", "data-testid": "song-flow",
            CoverFlow {
                items: list,
                initial: resting.unwrap_or_else(|| resting_index(count)),
                focus: resting,
                cover_px,
                wide,
                label,
                on_open: move |run_index: usize| {
                    let run = runs.peek().get(run_index).copied();
                    if let Some(run) = run {
                        let index = lead_song(&run, *playing_index.peek());
                        flipped.set(None);
                        on_play.call(index);
                    }
                },
                on_select: move |run_index: usize| {
                    let run = runs.peek().get(run_index).copied();
                    if let Some(run) = run {
                        flipped.set(Some(lead_song(&run, *playing_index.peek())));
                    }
                },
            }
            FlowMark { list_testid, index: flipped }
        }
    }
}

/// Tints the flipped song's row in the list below. A rule keyed on the row's
/// `data-index`, so it survives the windowed list mounting and unmounting
/// rows, and the list itself never re-renders for it.
#[component]
fn FlowMark(list_testid: String, index: Signal<Option<usize>>) -> Element {
    let Some(index) = index() else {
        return rsx! {};
    };
    let rule = format!(
        "[data-testid=\"{list_testid}\"] li[data-index=\"{index}\"] {{ background: var(--tint-strong); }}"
    );
    rsx! {
        style { "{rule}" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str, cover: Option<&str>, album: Option<&str>) -> Song {
        Song {
            id: id.to_string(),
            title: format!("Song {id}"),
            server_id: "s".to_string(),
            cover_art: cover.map(str::to_string),
            album_id: album.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn consecutive_shared_covers_collapse() {
        let songs = vec![
            song("1", Some("a"), None),
            song("2", Some("a"), None),
            song("3", Some("b"), None),
            song("4", None, Some("al-b")),
            song("5", None, None),
            song("6", None, None),
            song("7", Some("a"), None),
        ];
        let runs = song_runs(&songs);
        assert_eq!(
            runs,
            vec![
                Run { start: 0, len: 2 },
                Run { start: 2, len: 1 },
                Run { start: 3, len: 1 },
                Run { start: 4, len: 1 },
                Run { start: 5, len: 1 },
                Run { start: 6, len: 1 },
            ]
        );
        assert_eq!(run_of(&runs, 1), Some(0));
        assert_eq!(run_of(&runs, 6), Some(5));
        assert_eq!(run_of(&runs, 9), None);
    }

    #[test]
    fn one_album_playlist_is_one_cover() {
        let songs: Vec<Song> = (0..10).map(|i| song(&i.to_string(), Some("same"), None)).collect();
        assert_eq!(song_runs(&songs), vec![Run { start: 0, len: 10 }]);
        assert!(!wants_flow(&song_runs(&songs), true, 10));
    }

    #[test]
    fn lead_song_prefers_the_playing_one() {
        let run = Run { start: 4, len: 3 };
        assert_eq!(lead_song(&run, None), 4);
        assert_eq!(lead_song(&run, Some(5)), 5);
        assert_eq!(lead_song(&run, Some(9)), 4);
    }

    #[test]
    fn position_reads_naturally() {
        let mut s = song("1", Some("a"), None);
        s.album = Some("Glass Weather".to_string());
        assert_eq!(position_line(&s, &Run { start: 2, len: 1 }, 40), "Glass Weather · 3 of 40");
        assert_eq!(position_line(&s, &Run { start: 2, len: 10 }, 1200), "Glass Weather · 3–12 of 1,200");
        assert_eq!(grouped(1234567), "1,234,567");
    }
}
