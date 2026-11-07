//! Queue row gestures shared by the queue page and Up Next: drag a row by
//! its handle to reorder, swipe it for Remove / Add to…, and Undo a remove.
//! The gesture tracking itself is the kit's (`ui/row_gestures.rs`).

use super::actions::{song_key, use_player};
use crate::api::Song;
use crate::components::ui::{use_row_gestures, RowGesture, UndoToast};
use dioxus::prelude::*;

/// The song a remove took out of the queue, and where it was.
#[derive(Clone, PartialEq)]
pub(crate) struct QueueUndo {
    pub song: Song,
    pub index: usize,
    /// Restarts the toast's timer for each new remove.
    pub generation: u64,
}

/// The queue's "Remove from Queue" label, for rows' track menus.
pub(crate) const REMOVE_FROM_QUEUE: &str = "Remove from Queue";

/// Routes list `id`'s row gestures to the queue. Returns the Undo state for
/// [`QueueUndoToast`].
pub(crate) fn use_queue_gestures(id: &'static str) -> Signal<Option<QueueUndo>> {
    let mut player = use_player();
    let mut undo
 = use_signal(|| None::<QueueUndo>);
    let mut generation = use_signal(|| 0u64);
    let on_gesture = use_callback(move |gesture: RowGesture| {
        // The entry the script saw, if it is still at that index.
        let entry = |index: usize, key: &str| -> Option<Song> {
            player
                .queue
                .peek()
                .get(index)
                .filter(|song| song_key(song) == key)
                .cloned()
        };
        match gesture {
            RowGesture::Move { from, to, key } => {
                if entry(from, &key).is_some() {
                    player.move_entry(from, to);
                }
            }
            RowGesture::Remove { index, key } => {
                let Some(song) = entry(index, &key) else {
                    return;
                };
                player.remove_entry(index);
                let next = *generation.peek() + 1;
                generation.set(next);
                undo.set(Some(QueueUndo {
                    song,
                    index,
                    generation: next,
                }));
            }
        }
    });
    use_row_gestures(id, on_gesture);
    undo
}

/// "Removed “Song”  Undo" for about five seconds after a queue remove.
#[component]
pub(crate) fn QueueUndoToast(undo: Signal<Option<QueueUndo>>) -> Element {
    let mut player = use_player();
    let mut undo = undo;
    let Some(entry) = undo() else {
        return rsx! {};
    };
    rsx! {
        UndoToast {
            message: format!("Removed “{}”", entry.song.title),
            generation: entry.generation,
            on_undo: move |_| {
                if let Some(entry) = undo.take() {
                    player.insert_entry(entry.index, entry.song);
                }
            },
            on_close: move |_| undo.set(None),
        }
    }
}
