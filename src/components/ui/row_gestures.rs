//! Drag-to-reorder and swipe actions for editable song lists (the queue page,
//! Up Next, the user's own playlists).
//!
//! * Every row of a reorderable list shows a grip handle at its trailing
//!   edge (`TrackRow { handle: true }`). Pressing it lifts the row at once;
//!   no Edit mode, no long-press. Keyboard: Space picks up, arrows move,
//!   Space drops, Escape cancels.
//! * Swiping a row from left to right reveals its actions
//!   (`TrackRow { swipe }`): Remove and Add to… in editable lists, Play Next
//!   and Add to… elsewhere. A full swipe removes the song directly.
//!
//! The tracking is JavaScript (`row_gestures.js`), like the edge swipe and
//! the Now Playing gestures, so it runs in the iOS WKWebView too and does no
//! Rust work per move. Rust gets one message per finished gesture, as a
//! [`RowGesture`], carrying the row's index and song key so a list can refuse
//! a stale one. The track menu's "Remove from …" item arrives the same way.
//!
//! Use: call [`use_row_gestures`] once in the list's owner, and wrap each
//! list of rows in a [`RowGestureList`] with the same id.

use crate::components::ui::TrackMenuController;
use dioxus::prelude::*;
use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};

const ROW_GESTURES_JS: &str = include_str!("row_gestures.js");

/// A finished gesture on row `index` (whose song key is `key`).
#[derive(Clone, Debug, PartialEq)]
pub enum RowGesture {
    /// Drop the row at `to` (handle drag or keyboard).
    Move { from: usize, to: usize, key: String },
    /// Remove the song from the list (swipe action, full swipe or the track
    /// menu). The list shows the Undo toast.
    Remove { index: usize, key: String },
    /// Open the add-to sheet for the song.
    AddTo { index: usize, key: String },
    /// Play the song next.
    PlayNext { index: usize, key: String },
}

/// Which actions sit behind a row.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum RowSwipe {
    /// No swipe.
    #[default]
    None,
    /// Remove (red, the full-swipe action) and Add to….
    Edit,
    /// Play Next and Add to… (lists the user can't edit).
    Browse,
}

/// The id of the enclosing [`RowGestureList`], for rows (handle help text,
/// track-menu removes).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RowGestureListId(pub &'static str);

/// A "Remove from …" picked in the track menu, waiting for its list.
#[derive(Clone, PartialEq, Debug)]
pub struct RowRemoveRequest {
    pub list: &'static str,
    pub index: usize,
    pub key: String,
}

#[derive(Deserialize)]
struct Message {
    kind: String,
    #[serde(default)]
    index: usize,
    #[serde(default)]
    from: usize,
    #[serde(default)]
    to: usize,
    #[serde(default)]
    key: String,
}

impl Message {
    fn into_gesture(self) -> Option<RowGesture> {
        let Message {
            kind,
            index,
            from,
            to,
            key,
        } = self;
        Some(match kind.as_str() {
            "move" => RowGesture::Move { from, to, key },
            "remove" => RowGesture::Remove { index, key },
            "add" => RowGesture::AddTo { index, key },
            "next" => RowGesture::PlayNext { index, key },
            _ => return None,
        })
    }
}

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

/// Installs the gesture script (once per page) and routes list `id`'s
/// gestures, and its track-menu removes, to `on_gesture`. Call once per list
/// id, in the component that owns the list's data.
pub fn use_row_gestures(id: &'static str, on_gesture: Callback<RowGesture>) {
    let token = use_hook(|| NEXT_TOKEN.fetch_add(1, Ordering::Relaxed));
    use_hook(move || {
        let script = ROW_GESTURES_JS
            .replace("__LIST__", &format!("{id:?}"))
            .replace("__TOKEN__", &token.to_string());
        let mut channel = document::eval(&script);
        spawn(async move {
            while let Ok(message) = channel.recv::<Message>().await {
                if let Some(gesture) = message.into_gesture() {
                    on_gesture.call(gesture);
                }
            }
        });
    });
    use_drop(move || {
        let _ = document::eval(&format!(
            "window.__rsRowG && window.__rsRowG.unregister({id:?}, {token});"
        ));
    });

    // "Remove from …" in the track menu.
    let mut menu = use_context::<TrackMenuController>();
    use_effect(move || {
        let pending = (menu.remove)();
        let Some(request) = pending else {
            return;
        };
        if request.list != id {
            return;
        }
        menu.remove.set(None);
        on_gesture.call(RowGesture::Remove {
            index: request.index,
            key: request.key,
        });
    });
}

/// Wraps one list of rows (an `InsetList` or a `VirtualList`) for the
/// gesture script. Layout-neutral (`display: contents`).
/// * `id`: the list id given to [`use_row_gestures`]. Several wrappers may
///   share one id (Up Next's Played and Up Next sections).
/// * `reorder`: the inclusive range of row indexes the handle may move rows
///   within. None: no reordering (rows show no handle).
#[component]
pub fn RowGestureList(
    id: &'static str,
    #[props(default)] reorder: Option<(usize, usize)>,
    children: Element,
) -> Element {
    use_context_provider(|| RowGestureListId(id));
    let (min, max) = match reorder {
        Some((min, max)) => (Some(min.to_string()), Some(max.to_string())),
        None => (None, None),
    };
    rsx! {
        div {
            class: "rs-rowg",
            "data-rowg": id,
            "data-rowg-min": min,
            "data-rowg-max": max,
            if reorder.is_some() {
                p { class: "rs-sr-only", id: "{help_id(id)}",
                    "To move a song, press Space on its handle, then use the up and down arrow keys, and press Space again to drop it. Escape cancels."
                }
            }
            {children}
        }
    }
}

/// The id of a list's hidden reorder instructions (the handles'
/// `aria-describedby`).
pub fn help_id(list: &str) -> String {
    format!("rs-rowg-help-{list}")
}
