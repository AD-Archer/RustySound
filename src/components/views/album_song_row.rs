//! An album's track list: numbered `TrackRow`s in inset lists, split under
//! "Disc N" headers when the album has more than one disc.

use crate::api::Song;
use crate::components::ui::{InsetList, ListInset, SectionHeader, TrackRow};
use crate::components::views::library::shared::{song_key, use_downloaded_keys, use_now_playing};
use dioxus::prelude::*;

/// Splits `songs` into discs. The Song model carries no disc number, so a
/// new disc starts where the track number falls back (1..12, 1..9). Servers
/// list album songs by disc then track, so this matches the tags.
fn disc_ranges(songs: &[Song]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut last_track = 0u32;
    for (index, song) in songs.iter().enumerate() {
        let track = song.track.unwrap_or(0);
        if index > 0 && track > 0 && last_track > 0 && track < last_track {
            ranges.push((start, index));
            start = index;
        }
        if track > 0 {
            last_track = track;
        }
    }
    if !songs.is_empty() {
        ranges.push((start, songs.len()));
    }
    ranges
}

/// The tracks of one album. Tap plays the album from that track; the track
/// menu opens on long-press, right-click or "…".
#[component]
pub fn AlbumTracks(songs: Signal<Vec<Song>>, on_play: EventHandler<usize>) -> Element {
    let (playing_key, live) = use_now_playing();
    let downloaded = use_downloaded_keys(songs);
    let list = songs.read();
    let ranges = disc_ranges(&list);
    let multi_disc = ranges.len() > 1;
    let playing_key = playing_key();
    let live = live();
    let downloaded = downloaded.read();

    rsx! {
        for (disc, (start, end)) in ranges.into_iter().enumerate() {
            Fragment { key: "disc-{disc}",
            if multi_disc {
                SectionHeader { title: format!("Disc {}", disc + 1) }
            }
            InsetList {
                inset: ListInset::Track,
                label: if multi_disc { format!("Disc {}", disc + 1) } else { "Tracks".to_string() },
                class: "rs-library-tracks".to_string(),
                for index in start..end {
                    {
                        let song = list[index].clone();
                        let key = song_key(&song);
                        let playing = playing_key.as_deref() == Some(key.as_str());
                        let number = song.track.filter(|t| *t > 0).unwrap_or((index - start + 1) as u32);
                        rsx! {
                            TrackRow {
                                key: "{key}:{index}",
                                number,
                                playing,
                                live: playing && live,
                                downloaded: downloaded.contains(&key),
                                index,
                                song,
                                on_play: move |_| on_play.call(index),
                            }
                        }
                    }
                }
            }
            }
        }
    }
}
