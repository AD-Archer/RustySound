//! Playback, queue and library actions shared by Now Playing, Up Next and
//! the queue page.
//!
//! Transport logic mirrors the player bar's buttons (`player/controls.rs`) so
//! the sheet's own buttons (different ids, see `transport.rs`) behave exactly
//! like `play-pause-btn`, `prev-btn`, `next-btn`, `shuffle-btn` and
//! `repeat-btn`. Handlers read with `peek`, so calling them never subscribes a
//! component to the queue.

use crate::api::{NavidromeClient, ServerConfig, Song};
use crate::components::audio_manager::{
    apply_collection_shuffle_mode, queue_should_generate_similar_on_end, spawn_shuffle_queue,
};
use crate::components::ui::{SongChange, TrackMenuController};
use crate::components::{
    seek_to, AudioState, IsPlayingSignal, PlaybackPositionSignal, ShuffleEnabledSignal,
    VolumeSignal,
};
use crate::db::{AppSettings, RepeatMode};
use dioxus::prelude::*;

/// Every playback signal Now Playing needs, taken from context once.
#[derive(Clone, Copy)]
pub(crate) struct PlayerCtx {
    pub servers: Signal<Vec<ServerConfig>>,
    pub queue: Signal<Vec<Song>>,
    pub queue_index: Signal<usize>,
    pub now_playing: Signal<Option<Song>>,
    pub is_playing: Signal<bool>,
    pub repeat: Signal<RepeatMode>,
    pub shuffle: Signal<bool>,
    pub settings: Signal<AppSettings>,
    pub audio: Signal<AudioState>,
    pub volume: Signal<f64>,
    pub position: Signal<f64>,
    pub track_menu: TrackMenuController,
}

pub(crate) fn use_player() -> PlayerCtx {
    use_hook(|| PlayerCtx {
        servers: consume_context(),
        queue: consume_context(),
        queue_index: consume_context(),
        now_playing: consume_context(),
        is_playing: consume_context::<IsPlayingSignal>().0,
        repeat: consume_context(),
        shuffle: consume_context::<ShuffleEnabledSignal>().0,
        settings: consume_context(),
        audio: consume_context(),
        volume: consume_context::<VolumeSignal>().0,
        position: consume_context::<PlaybackPositionSignal>().0,
        track_menu: consume_context(),
    })
}

fn persist_settings(settings: AppSettings) {
    spawn(async move {
        let _ = crate::db::save_settings(settings).await;
    });
}

impl PlayerCtx {
    pub fn toggle_play(&mut self) {
        let next = !*self.is_playing.peek();
        self.is_playing.set(next);
    }

    /// Same as `prev-btn`: restart, or go back a song on a quick second
    /// press (`player/skip_back.rs`).
    pub fn previous(&mut self) {
        let current_time = self.audio.peek().current_time;
        crate::components::player::skip_back::press_skip_back(
            crate::components::player::skip_back::SkipBackSignals {
                queue: self.queue,
                queue_index: self.queue_index,
                now_playing: self.now_playing,
                is_playing: self.is_playing,
                repeat: self.repeat,
                position: self.position,
                current_time,
            },
            "now-playing.prev",
            false,
        );
    }

    /// Same as `next-btn`: repeat-one restarts, the end of the queue wraps
    /// (repeat all), grows a similar queue, or stops.
    pub fn next(&mut self) {
        let was_playing = *self.is_playing.peek();
        let repeat = *self.repeat.peek();
        if repeat == RepeatMode::One {
            seek_to(0.0);
            if was_playing {
                self.is_playing.set(true);
            }
            return;
        }
        let idx = *self.queue_index.peek();
        let len = self.queue.peek().len();
        let shuffle = *self.shuffle.peek();
        if idx + 1 < len {
            let song = self.queue.peek().get(idx + 1).cloned();
            self.queue_index.set(idx + 1);
            self.now_playing.set(song);
            if was_playing {
                self.is_playing.set(true);
            }
        } else if repeat == RepeatMode::All && len > 0 {
            let song = self.queue.peek().first().cloned();
            self.queue_index.set(0);
            self.now_playing.set(song);
            if was_playing {
                self.is_playing.set(true);
            }
        } else if repeat == RepeatMode::Off
            && queue_should_generate_similar_on_end(
                &self.queue.peek(),
                self.now_playing.peek().as_ref(),
                shuffle,
            )
        {
            spawn_shuffle_queue(
                self.servers.peek().clone(),
                self.queue,
                self.queue_index,
                self.now_playing,
                self.is_playing,
                self.audio,
                self.now_playing.peek().clone(),
                Some(was_playing),
            );
        } else {
            self.is_playing.set(false);
        }
    }

    pub fn toggle_shuffle(&mut self) {
        let next = !*self.shuffle.peek();
        self.shuffle.set(next);
        let _ = apply_collection_shuffle_mode(self.queue, self.queue_index, self.now_playing, next);
        self.settings.with_mut(|s| s.shuffle_enabled = next);
        persist_settings(self.settings.peek().clone());
    }

    /// Off → All → One → Off.
    pub fn cycle_repeat(&mut self) {
        let next = match *self.repeat.peek() {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        };
        self.repeat.set(next);
        self.settings.with_mut(|s| s.repeat_mode = next);
        persist_settings(self.settings.peek().clone());
    }

    pub fn set_volume(&mut self, value: f64) {
        self.volume.set(value.clamp(0.0, 1.0));
    }

    pub fn seek(&mut self, seconds: f64) {
        let target = seconds.max(0.0);
        self.position.set(target);
        let mut current = self.audio.peek().current_time;
        current.set(target);
        seek_to(target);
    }

    /// Jumps to queue entry `index` and plays it.
    pub fn play_index(&mut self, index: usize) {
        let song = self.queue.peek().get(index).cloned();
        if let Some(song) = song {
            self.queue_index.set(index);
            self.now_playing.set(Some(song));
            self.is_playing.set(true);
        }
    }

    /// Moves a queue entry; the playing entry keeps playing.
    pub fn move_entry(&mut self, from: usize, to: usize) {
        let len = self.queue.peek().len();
        if len < 2 || from >= len || to >= len || from == to {
            return;
        }
        let current = *self.queue_index.peek();
        self.queue.with_mut(|items| {
            let song = items.remove(from);
            items.insert(to, song);
        });
        let next_index = if from == current {
            to
        } else if from < current && to >= current {
            current - 1
        } else if from > current && to <= current {
            current + 1
        } else {
            current
        };
        if next_index != current {
            self.queue_index.set(next_index);
        }
    }

    /// Removes a queue entry. Removing the playing entry plays the next one.
    pub fn remove_entry(&mut self, index: usize) {
        let len = self.queue.peek().len();
        if index >= len {
            return;
        }
        let current = *self.queue_index.peek();
        let was_playing = *self.is_playing.peek();
        self.queue.with_mut(|items| {
            items.remove(index);
        });
        let remaining = len - 1;
        if remaining == 0 {
            self.queue_index.set(0);
            self.now_playing.set(None);
            self.is_playing.set(false);
            return;
        }
        if index < current {
            self.queue_index.set(current - 1);
        } else if index == current {
            let next = index.min(remaining - 1);
            let song = self.queue.peek().get(next).cloned();
            self.queue_index.set(next);
            self.now_playing.set(song);
            self.is_playing.set(was_playing);
        }
    }

    /// Clears the queue, keeping the playing song.
    pub fn clear_queue(&mut self) {
        let current = self.now_playing.peek().clone();
        match current {
            Some(song) => {
                self.queue.set(vec![song]);
                self.queue_index.set(0);
            }
            None => {
                self.queue.set(Vec::new());
                self.queue_index.set(0);
                self.is_playing.set(false);
            }
        }
    }

    fn server_for(&self, song: &Song) -> Option<ServerConfig> {
        self.servers
            .peek()
            .iter()
            .find(|server| server.id == song.server_id)
            .cloned()
    }

    fn update_song(&mut self, song: &Song, apply: impl Fn(&mut Song)) {
        let (id, server_id) = (song.id.clone(), song.server_id.clone());
        self.queue.with_mut(|items| {
            for item in items
                .iter_mut()
                .filter(|s| s.id == id && s.server_id == server_id)
            {
                apply(item);
            }
        });
        let matches = self
            .now_playing
            .peek()
            .as_ref()
            .map(|s| s.id == id && s.server_id == server_id)
            .unwrap_or(false);
        if matches {
            self.now_playing.with_mut(|current| {
                if let Some(s) = current.as_mut() {
                    apply(s);
                }
            });
        }
    }

    /// Stars or unstars `song` on its server and tells lists (same path as
    /// the track menu, so the two never disagree).
    pub fn toggle_favorite(&self, song: Song) {
        let Some(server) = self.server_for(&song) else {
            return;
        };
        let should_star = song.starred.is_none();
        let mut this = *self;
        spawn(async move {
            let client = NavidromeClient::new(server);
            let result = if should_star {
                client.star(&song.id, "song").await
            } else {
                client.unstar(&song.id, "song").await
            };
            if result.is_ok() {
                let marker = should_star.then(|| "local".to_string());
                this.update_song(&song, |s| s.starred = marker.clone());
                this.track_menu.last_change.set(Some(SongChange {
                    server_id: song.server_id.clone(),
                    song_id: song.id.clone(),
                    starred: Some(should_star),
                    rating: None,
                    downloaded: None,
                }));
            }
        });
    }

    /// Sets a 0–5 rating (0 clears).
    pub fn set_rating(&self, song: Song, value: u32) {
        let Some(server) = self.server_for(&song) else {
            return;
        };
        let value = value.min(5);
        let mut this = *self;
        spawn(async move {
            let client = NavidromeClient::new(server);
            if client.set_rating(&song.id, value).await.is_ok() {
                let stored = (value > 0).then_some(value);
                this.update_song(&song, |s| s.user_rating = stored);
                this.track_menu.last_change.set(Some(SongChange {
                    server_id: song.server_id.clone(),
                    song_id: song.id.clone(),
                    starred: None,
                    rating: Some(value),
                    downloaded: None,
                }));
            }
        });
    }
}

/// `server::id`, used for keys and dedup.
pub(crate) fn song_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
}

pub(crate) fn same_song(a: &Song, b: &Song) -> bool {
    a.id == b.id && a.server_id == b.server_id
}

/// Radio entries: no seeking, no queue actions.
pub(crate) fn is_live_song(song: &Song) -> bool {
    song.server_name == "Radio"
        || song
            .stream_url
            .as_ref()
            .map(|url| !url.trim().is_empty())
            .unwrap_or(false)
}

/// "FLAC 24/96"-style badge text when the format is known. Subsonic gives
/// the container suffix and bitrate; sample rate and depth are not in the
/// song model, so lossy files show the bitrate instead.
pub(crate) fn format_badge(song: &Song) -> Option<String> {
    let suffix = song
        .suffix
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_uppercase())
        .or_else(|| {
            song.content_type
                .as_deref()
                .and_then(|ct| ct.split('/').nth(1))
                .map(|s| s.trim_start_matches("x-").to_ascii_uppercase())
        })?;
    let lossless = matches!(suffix.as_str(), "FLAC" | "ALAC" | "WAV" | "AIFF" | "APE" | "WV" | "DSF");
    match song.bitrate.filter(|b| *b > 0) {
        Some(kbps) if !lossless => Some(format!("{suffix} {kbps}")),
        _ => Some(suffix),
    }
}
