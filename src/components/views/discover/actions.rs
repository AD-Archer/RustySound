//! Playback and feedback helpers shared by the Discover sections.

use crate::api::{QueueSourceKind, Song};
use crate::components::audio_manager::{assign_collection_queue_meta, normalize_manual_queue_songs};
use crate::components::ui::env::sleep_ms;
use crate::components::IsPlayingSignal;
use crate::db::AppSettings;
use dioxus::core::{current_scope_id, Runtime, Task};
use dioxus::prelude::*;
use rand::seq::SliceRandom;

/// Handles to the shell's queue signals. `Copy`, so closures take it freely.
#[derive(Clone, Copy)]
pub struct Player {
    queue: Signal<Vec<Song>>,
    queue_index: Signal<usize>,
    now_playing: Signal<Option<Song>>,
    is_playing: Signal<bool>,
    settings: Signal<AppSettings>,
}

pub fn use_player() -> Player {
    Player {
        queue: use_context::<Signal<Vec<Song>>>(),
        queue_index: use_context::<Signal<usize>>(),
        now_playing: use_context::<Signal<Option<Song>>>(),
        is_playing: use_context::<IsPlayingSignal>().0,
        settings: use_context::<Signal<AppSettings>>(),
    }
}

impl Player {
    /// In offline mode only downloaded songs can play. Call from handlers or
    /// tasks, never from render (it checks the filesystem).
    fn playable(&self, songs: Vec<Song>) -> Result<Vec<Song>, String> {
        let songs = if self.settings.peek().offline_mode {
            songs
                .into_iter()
                .filter(crate::offline_audio::is_song_downloaded)
                .collect()
        } else {
            songs
        };
        if songs.is_empty() {
            Err("None of these songs are downloaded, so they can't play offline.".to_string())
        } else {
            Ok(songs)
        }
    }

    /// Replace the queue with `songs` and start playing, optionally shuffled.
    pub fn play(
        &self,
        songs: Vec<Song>,
        kind: QueueSourceKind,
        source_id: String,
        shuffle: bool,
    ) -> Result<(), String> {
        let mut songs = self.playable(songs)?;
        if shuffle {
            songs.shuffle(&mut rand::thread_rng());
        }
        let songs = assign_collection_queue_meta(songs, kind, source_id);
        self.start(songs, 0);
        Ok(())
    }

    /// Replace the queue with `songs` and start at `index` (song lists).
    pub fn play_at(&self, songs: Vec<Song>, index: usize) -> Result<(), String> {
        let target = songs.get(index).cloned();
        let songs = self.playable(songs)?;
        let start = target
            .and_then(|t| songs.iter().position(|s| s.id == t.id && s.server_id == t.server_id))
            .unwrap_or(0);
        self.start(normalize_manual_queue_songs(songs), start);
        Ok(())
    }

    /// Append to the queue (starts playback when nothing is queued).
    pub fn enqueue(&self, songs: Vec<Song>) -> Result<usize, String> {
        let songs = normalize_manual_queue_songs(self.playable(songs)?);
        let count = songs.len();
        if self.now_playing.peek().is_none() && self.queue.peek().is_empty() {
            self.start(songs, 0);
        } else {
            let mut queue = self.queue;
            queue.with_mut(|items| items.extend(songs));
        }
        Ok(count)
    }

    fn start(&self, songs: Vec<Song>, index: usize) {
        let (mut queue, mut queue_index, mut now_playing, mut is_playing) =
            (self.queue, self.queue_index, self.now_playing, self.is_playing);
        let first = songs.get(index).cloned();
        queue.set(songs);
        queue_index.set(index);
        now_playing.set(first);
        is_playing.set(true);
    }
}

/// A short status message ("Saved “Deep Cuts”").
#[derive(Clone, PartialEq)]
pub struct Notice {
    pub text: String,
    pub error: bool,
}

/// The Discover toast. Shown by [`NoticeCapsule`]; hides itself after 3.2 s.
/// Also the handle for tasks that must outlive the row or menu that started
/// them (see [`Toast::spawn`]).
#[derive(Clone, Copy)]
pub struct Toast {
    notice: Signal<Option<Notice>>,
    generation: Signal<u64>,
    scope: ScopeId,
}

impl Toast {
    /// Create in the Discover view's scope (as a context provider's init).
    pub fn new() -> Self {
        Self {
            notice: Signal::new(None),
            generation: Signal::new(0),
            scope: current_scope_id(),
        }
    }

    /// Spawn on the Discover view's scope: the task survives the row or menu
    /// that started it unmounting, and still stops when Discover unmounts.
    pub fn spawn(&self, fut: impl std::future::Future<Output = ()> + 'static) -> Task {
        Runtime::current().spawn(self.scope, fut)
    }

    pub fn show(&self, text: impl Into<String>, error: bool) {
        let (mut notice, mut generation) = (self.notice, self.generation);
        let current = *generation.peek() + 1;
        generation.set(current);
        notice.set(Some(Notice {
            text: text.into(),
            error,
        }));
        self.spawn(async move {
            sleep_ms(3200).await;
            if *generation.peek() == current {
                notice.set(None);
            }
        });
    }
}

/// Renders the current toast, if any, as the kit's status capsule.
#[component]
pub fn NoticeCapsule() -> Element {
    let toast = use_context::<Toast>();
    let Some(notice) = (toast.notice)() else {
        return rsx! {};
    };
    rsx! {
        div {
            class: "rs-capsule",
            class: if notice.error { "rs-capsule--error" },
            role: "status",
            "data-testid": "discover-notice",
            "{notice.text}"
        }
    }
}
