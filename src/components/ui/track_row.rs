//! Song rows: `TrackRow` (number or art, title, subtitle, marks, duration,
//! "…", with the track menu and the row gestures) and `SongRow` (the same
//! look without them, for pickers and suggestion lists).

use crate::api::Song;
use crate::components::ui::env::format_clock;
use crate::components::ui::list_row::{is_menu_key, menu_point, more_button_point};

use crate::components::ui::row_gestures::help_id;
use crate::components::ui::song_drop::carry_song;
use crate::components::ui::{
    Artwork, RowGestureListId, RowRemoveRequest, RowSwipe, TrackMenuController, TrackMenuRemove,
};
use crate::components::{AddIntent, AddMenuController, Icon, PlayerCtx, QueuePosition};
use dioxus::prelude::*;

/// One song in a list (`li`), for use inside an `InsetList`.
///
/// Tap/click/Enter/Space plays (`on_play`, through a real `<button>`). The
/// "…" button is its sibling, not its child. The track menu opens on a touch
/// long-press (450 ms, cancelled by moving more than 10px; the tap that ends
/// a long-press does not also play), on right-click (`contextmenu`), on the
/// "…" button, or with the context-menu key / Shift+F10. The long-press is
/// tracked in `row_gestures.js`, which then raises the row's `contextmenu`,
/// so the row has no pointer handlers of its own. With the mouse the row can
/// also be carried onto a playlist or the queue (`song_drop.rs`).
/// * `song`: the row's song (the menu acts on it).
/// * `number`: show this track number instead of artwork (album lists).
/// * `subtitle`: second line; defaults to the artist. Ignored with `number`.
/// * `playing`: current track (accent title + equaliser in the number slot).
/// * `live`: animate the equaliser (playing and not paused).
/// * `downloaded`: show the downloaded mark (pass from a cached set, never
///   from a filesystem call in render).
///
/// Songs on the device carry a small "On This Device" mark; ones this device
/// can't decode say so in place of the subtitle and don't play. A song the
/// latest scan just found fades in the first time its row appears.
/// * `show_duration`: trailing duration. `index`: written as data-index.
/// * `trailing`: extra controls between the duration and "…" (buttons are
///   fine: they are siblings of the play button).
///
/// Inside a `RowGestureList` (see `row_gestures.rs`):
/// * `swipe`: the actions a left-to-right swipe reveals. Defaults to the
///   list's `swipe`, so `RowGestureList { swipe: RowSwipe::Browse }` alone
///   gives every row Play Next and Add to… (the row acts on its own song).
///   `RowSwipe::Edit` also needs `index` and the list's `use_row_gestures`.
/// * `handle`: a grip handle at the trailing edge; pressing it drags the row
///   (needs `index` and a `reorder` range on the list).
/// * `remove_label`: adds "Remove from …" with this label to the track menu.
#[component]
pub fn TrackRow(
    song: Song,
    #[props(default)] number: Option<u32>,
    #[props(default)] subtitle: Option<String>,
    #[props(default)] playing: bool,
    #[props(default)] live: bool,
    #[props(default)] downloaded: bool,
    #[props(default = true)] show_duration: bool,
    #[props(default)] index: Option<usize>,
    #[props(default)] handle: bool,
    #[props(default)] swipe: RowSwipe,
    #[props(default)] remove_label: Option<String>,
    #[props(default)] trailing: Option<Element>,
    on_play: EventHandler<()>,
) -> Element {
    let menu = use_context::<TrackMenuController>();
    let gesture_list = try_use_context::<RowGestureListId>();
    let key = format!("{}::{}", song.server_id, song.id);
    // The menu's "Remove from …" item, for rows of a list it applies to.
    let menu_remove = match (gesture_list, index, remove_label.clone()) {
        (Some(list), Some(index), Some(label)) => Some(TrackMenuRemove {
            label,
            request: RowRemoveRequest {
                list: list.id,
                index,
                key: key.clone(),
            },
        }),
        _ => None,
    };
    let handle = handle && gesture_list.is_some() && index.is_some();
    // The row's own swipe, else its list's. Removing needs the row's index.
    let swipe = match gesture_list {
        None => RowSwipe::None,
        Some(list) => match if swipe == RowSwipe::None {
            list.swipe
        } else {
            swipe
        } {
            RowSwipe::Edit if index.is_none() => RowSwipe::None,
            chosen => chosen,
        },
    };
    let swipe_kind = match swipe {
        RowSwipe::None => None,
        RowSwipe::Edit => Some("edit"),
        RowSwipe::Browse => Some("browse"),
    };

    let open_menu = {
        let song = song.clone();
        move |anchor: Option<(f64, f64)>| {
            let mut menu = menu;
            menu.open_with(song.clone(), anchor, menu_remove.clone());
        }
    };

    let playable = crate::local_music::is_playable(&song);
    let local = crate::local_music::is_local_song(&song);
    let fresh = use_hook(|| local && crate::local_music::take_fresh(&song.id));
    let numbered = number.is_some();

    rsx! {
        li {
            "data-index": index.map(|i| i.to_string()),
            "data-key": "{key}",
            "data-carry": "true",
            "data-rowg-swipe": swipe_kind,
            // What a left-to-right swipe reveals (row_gestures.js slides the
            // row over it). Touch only and hidden until then; the same
            // actions are in the track menu for every other input.
            if swipe != RowSwipe::None {
                div { class: "rs-rowg__actions", aria_hidden: "true",
                    if swipe == RowSwipe::Edit {
                        button {
                            r#type: "button",
                            class: "rs-rowg__action rs-rowg__action--remove",
                            tabindex: "-1",
                            "data-rowg-action": "remove",
                            "data-testid": "row-swipe-remove",
                            Icon { name: "trash".to_string(), class: "rs-ic".to_string() }
                            span { "Remove" }
                        }
                    } else {
                        button {
                            r#type: "button",
                            class: "rs-rowg__action rs-rowg__action--next",
                            tabindex: "-1",
                            "data-rowg-action": "next",
                            "data-testid": "row-swipe-next",
                            onclick: {
                                let song = song.clone();
                                move |_| {
                                    PlayerCtx::from_context().enqueue(vec![song.clone()], QueuePosition::Next);
                                }
                            },
                            Icon { name: "play-next".to_string(), class: "rs-ic".to_string() }
                            span { "Play Next" }
                        }
                    }
                    button {
                        r#type: "button",
                        class: "rs-rowg__action rs-rowg__action--add",
                        tabindex: "-1",
                        "data-rowg-action": "add",
                        "data-testid": "row-swipe-add",
                        onclick: {
                            let song = song.clone();
                            move |_| {
                                let mut add_menu = consume_context::<AddMenuController>();
                                add_menu.open(AddIntent::from_song(song.clone()));
                            }
                        },
                        Icon { name: "playlist".to_string(), class: "rs-ic".to_string() }
                        span { "Add to…" }
                    }
                }
            }
            // The row is a plain container: the play button and the "…" button
            // sit side by side inside it (no interactive nesting). The
            // context-menu handlers live here so a long-press or right-click
            // anywhere on the row opens the menu.
            div {
                class: "rs-trow rs-trow--split",
                class: if !numbered { "rs-trow--art" },
                class: if playing { "rs-trow--playing" },
                class: if !playable { "rs-trow--unplayable" },
                class: if fresh { "rs-fresh" },
                "data-testid": "track-row",
                "data-long-press": "true",
                onkeydown: {
                    let open_menu = open_menu.clone();
                    move |evt: KeyboardEvent| {
                        if is_menu_key(&evt) {
                            evt.prevent_default();
                            open_menu(None);
                        }
                    }
                },
                oncontextmenu: {
                    let open_menu = open_menu.clone();
                    move |evt: MouseEvent| {
                        evt.prevent_default();
                        open_menu(Some(menu_point(&evt)));
                    }
                },
                // Raised by row_gestures.js when the mouse starts carrying the row.
                ondragstart: {
                    let song = song.clone();
                    move |_| carry_song(&song)
                },

                // Play. Its accessible name is its visible text (number or
                // "Now playing", title, artist, marks, duration).
                button {
                    r#type: "button",
                    class: "rs-trow__main",
                    aria_current: if playing { Some("true") } else { None },
                    aria_disabled: if !playable { Some("true") } else { None },
                    onclick: move |_| {
                        if playable {
                            on_play.call(());
                        }
                    },
                    {song_row_body(&song, number, subtitle.clone(), playing, live, downloaded, show_duration)}
                }
                {trailing}
                button {
                    r#type: "button",
                    class: "rs-trow__more",
                    aria_label: "More for {song.title}",
                    aria_haspopup: "menu",
                    onclick: {
                        let open_menu = open_menu.clone();
                        move |evt: MouseEvent| {
                            evt.stop_propagation();
                            open_menu(Some(more_button_point(&evt)));
                        }
                    },
                    Icon { name: "more-horizontal".to_string(), class: "rs-ic rs-ic--md".to_string() }
                }
                if handle {
                    // Pressing starts a drag at once (row_gestures.js), and
                    // is never a long-press.
                    button {
                        r#type: "button",
                        class: "rs-trow__handle",
                        "data-row-handle": "true",
                        "data-testid": "row-handle",
                        aria_label: "Reorder {song.title}",
                        aria_describedby: gesture_list.map(|list| help_id(list.id)),
                        Icon { name: "grip".to_string(), class: "rs-ic rs-ic--md".to_string() }
                    }
                }
            }
        }
    }
}

/// A static song row (`li`): art, title, subtitle, duration and `trailing`
/// controls, with no menu and no gestures. For pickers and suggestion lists
/// (Add Songs, related songs, the add-to sheet's suggestions).
/// * `on_play`: makes the art and text one play button (none: plain text).
/// * `subtitle`: second line; defaults to the artist.
/// * `trailing`: buttons after the text (Add, Preview, Dismiss…).
/// * `class`: extra classes on the row (`rs-trow …`). `testid`: on the `li`.
#[component]
pub fn SongRow(
    song: Song,
    #[props(default)] on_play: Option<EventHandler<()>>,
    #[props(default)] subtitle: Option<String>,
    #[props(default)] show_duration: bool,
    #[props(default)] trailing: Option<Element>,
    #[props(default)] class: Option<String>,
    #[props(default)] testid: Option<String>,
) -> Element {
    let body = song_row_body(
        &song,
        None,
        subtitle.clone(),
        false,
        false,
        false,
        show_duration,
    );
    rsx! {
        li { "data-testid": testid.clone(),
            div { class: "rs-trow rs-trow--split rs-trow--art {class.clone().unwrap_or_default()}",
                if let Some(handler) = on_play {
                    button {
                        r#type: "button",
                        class: "rs-trow__main",
                        aria_label: "Play {song.title}",
                        onclick: move |_| handler.call(()),
                        {body}
                    }
                } else {
                    div { class: "rs-trow__main", {body} }
                }
                {trailing}
            }
        }
    }
}

/// The inside of a song row's play button: number (or the equaliser) or
/// art, title and subtitle, marks, duration. Shared by `TrackRow` and
/// `SongRow`.
fn song_row_body(
    song: &Song,
    number: Option<u32>,
    subtitle: Option<String>,
    playing: bool,
    live: bool,
    downloaded: bool,
    show_duration: bool,
) -> Element {
    let starred = song.starred.is_some();
    let local = crate::local_music::is_local_song(song);
    let playable = crate::local_music::is_playable(song);
    let numbered = number.is_some();
    // An album's own page says where it lives once, in its header.
    let local_mark = local && !numbered;
    let sub = if !playable {
        Some(crate::local_music::NOT_PLAYABLE.to_string())
    } else if numbered {
        None
    } else {
        subtitle
            .or_else(|| song.artist.clone())
            .filter(|s| !s.trim().is_empty())
    };
    let duration = format_clock(song.duration as f64);
    rsx! {
        if let Some(n) = number {
            if playing {
                span {
                    class: "rs-eq",
                    class: if live { "rs-eq--live" },
                    role: "img",
                    aria_label: "Now playing",
                    i {}
                    i {}
                    i {}
                }
            } else {
                span { class: "rs-trow__n", "{n}" }
            }
        } else {
            Artwork {
                cover_art: song.cover_art.clone(),
                server_id: song.server_id.clone(),
                size_px: 44,
            }
        }
        span { class: "rs-trow__meta",
            span { class: "rs-trow__title", "{song.title}" }
            if let Some(sub) = sub {
                span { class: "rs-trow__sub", "{sub}" }
            }
        }
        if starred || downloaded || local_mark {
            span { class: "rs-trow__marks",
                if starred {
                    span { class: "rs-trow__star", role: "img", aria_label: "Favorite",
                        Icon { name: "heart-filled".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                    }
                }
                if downloaded {
                    span { class: "rs-trow__dl", role: "img", aria_label: "Downloaded",
                        Icon { name: "downloaded".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                    }
                }
                if local_mark {
                    span {
                        class: "rs-trow__src",
                        role: "img",
                        aria_label: crate::local_music::LOCAL_SOURCE_NAME,
                        title: crate::local_music::LOCAL_SOURCE_NAME,
                        Icon { name: "hard-drive".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                    }
                }
            }
        }
        if show_duration && song.duration > 0 {
            span { class: "rs-trow__dur", "{duration}" }
        }
    }
}
