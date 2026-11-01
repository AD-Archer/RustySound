//! A song row: number or art, title, subtitle, marks, duration, "…".

use crate::api::Song;
use crate::components::ui::env::{format_clock, sleep_ms};
use crate::components::ui::row_gestures::help_id;
use crate::components::ui::{
    Artwork, RowGestureListId, RowRemoveRequest, RowSwipe, TrackMenuController, TrackMenuRemove,
};
use crate::components::Icon;
use dioxus::prelude::*;

const LONG_PRESS_MS: u64 = 450;
const LONG_PRESS_SLOP_PX: f64 = 10.0;

/// One song in a list (`li`), for use inside an `InsetList`.
///
/// Tap/click/Enter/Space plays (`on_play`, through a real `<button>`). The
/// "…" button is its sibling, not its child. The track menu opens on a touch
/// long-press (450 ms, cancelled by moving more than 10px; the tap that ends
/// a long-press does not also play), on right-click (`contextmenu`), on the
/// "…" button, or with the context-menu key / Shift+F10.
/// * `song`: the row's song (the menu acts on it).
/// * `number`: show this track number instead of artwork (album lists).
/// * `subtitle`: second line; defaults to the artist. Ignored with `number`.
/// * `playing`: current track (accent title + equaliser in the number slot).
/// * `live`: animate the equaliser (playing and not paused).
/// * `downloaded`: show the downloaded mark (pass from a cached set, never
///   from a filesystem call in render).
/// * `show_duration`: trailing duration. `index`: written as data-index.
///
/// Inside a `RowGestureList` (see `row_gestures.rs`, which needs `index`):
/// * `handle`: a grip handle at the trailing edge; pressing it drags the row.
/// * `swipe`: the actions a left-to-right swipe reveals.
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
    on_play: EventHandler<()>,
) -> Element {
    let menu = use_context::<TrackMenuController>();
    let gesture_list = try_use_context::<RowGestureListId>();
    let key = format!("{}::{}", song.server_id, song.id);
    // The menu's "Remove from …" item, for rows of a list it applies to.
    let menu_remove = match (gesture_list, index, remove_label.clone()) {
        (Some(RowGestureListId(list)), Some(index), Some(label)) => Some(TrackMenuRemove {
            label,
            request: RowRemoveRequest {
                list,
                index,
                key: key.clone(),
            },
        }),
        _ => None,
    };
    let handle = handle && gesture_list.is_some() && index.is_some();
    let swipe = if gesture_list.is_some() && index.is_some() {
        swipe
    } else {
        RowSwipe::None
    };
    let swipe_kind = match swipe {
        RowSwipe::None => None,
        RowSwipe::Edit => Some("edit"),
        RowSwipe::Browse => Some("browse"),
    };
    // Gesture bookkeeping. None of these are read during render, so writing
    // them never re-renders the row.
    let mut press_generation = use_signal(|| 0u64);
    let mut press_origin = use_signal(|| None::<(f64, f64)>);
    let mut long_pressed = use_signal(|| false);

    let open_menu = {
        let song = song.clone();
        move |anchor: Option<(f64, f64)>| {
            let mut menu = menu;
            menu.open_with(song.clone(), anchor, menu_remove.clone());
        }
    };

    let starred = song.starred.is_some();
    let duration = format_clock(song.duration as f64);
    let numbered = number.is_some();
    let sub = if numbered {
        None
    } else {
        subtitle
            .clone()
            .or_else(|| song.artist.clone())
            .filter(|s| !s.trim().is_empty())
    };

    rsx! {
        li {
            "data-index": index.map(|i| i.to_string()),
            "data-key": if gesture_list.is_some() { Some(key.clone()) } else { None },
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
                        Icon { name: "playlist".to_string(), class: "rs-ic".to_string() }
                        span { "Add to…" }
                    }
                }
            }
            // The row is a plain container: the play button and the "…" button
            // sit side by side inside it (no interactive nesting). Pointer and
            // context-menu handlers live here so a long-press or right-click
            // anywhere on the row opens the menu.
            div {
                class: "rs-trow rs-trow--split",
                class: if !numbered { "rs-trow--art" },
                class: if playing { "rs-trow--playing" },
                "data-testid": "track-row",
                onkeydown: {
                    let open_menu = open_menu.clone();
                    move |evt: KeyboardEvent| {
                        let key = evt.key();
                        if key == Key::ContextMenu || (key == Key::F10 && evt.modifiers().shift()) {
                            evt.prevent_default();
                            open_menu(None);
                        }
                    }
                },
                oncontextmenu: {
                    let open_menu = open_menu.clone();
                    move |evt: MouseEvent| {
                        evt.prevent_default();
                        press_origin.set(None);
                        let point = evt.client_coordinates();
                        open_menu(Some((point.x, point.y)));
                    }
                },
                onpointerdown: {
                    let open_menu = open_menu.clone();
                    move |evt: PointerEvent| {
                        long_pressed.set(false);
                        if evt.pointer_type() == "mouse" {
                            return;
                        }
                        let point = evt.client_coordinates();
                        let generation = *press_generation.peek() + 1;
                        press_generation.set(generation);
                        press_origin.set(Some((point.x, point.y)));
                        let open_menu = open_menu.clone();
                        spawn(async move {
                            sleep_ms(LONG_PRESS_MS).await;
                            if *press_generation.peek() != generation {
                                return;
                            }
                            let origin = *press_origin.peek();
                            if let Some(origin) = origin {
                                press_origin.set(None);
                                long_pressed.set(true);
                                open_menu(Some(origin));
                            }
                        });
                    }
                },
                onpointermove: move |evt| {
                    let origin = *press_origin.peek();
                    if let Some((x0, y0)) = origin {
                        let point = evt.client_coordinates();
                        if (point.x - x0).hypot(point.y - y0) > LONG_PRESS_SLOP_PX {
                            press_origin.set(None);
                        }
                    }
                },
                onpointerup: move |_| press_origin.set(None),
                onpointercancel: move |_| press_origin.set(None),

                // Play. Its accessible name is its visible text (number or
                // "Now playing", title, artist, marks, duration).
                button {
                    r#type: "button",
                    class: "rs-trow__main",
                    aria_current: if playing { Some("true") } else { None },
                    onclick: move |_| {
                        if *long_pressed.peek() {
                            long_pressed.set(false);
                            return;
                        }
                        on_play.call(());
                    },

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
                    if starred || downloaded {
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
                        }
                    }
                    if show_duration && song.duration > 0 {
                        span { class: "rs-trow__dur", "{duration}" }
                    }
                }
                button {
                    r#type: "button",
                    class: "rs-trow__more",
                    aria_label: "More for {song.title}",
                    aria_haspopup: "menu",
                    onpointerdown: move |evt| evt.stop_propagation(),
                    onclick: {
                        let open_menu = open_menu.clone();
                        move |evt: MouseEvent| {
                            evt.stop_propagation();
                            let point = evt.client_coordinates();
                            open_menu(Some((point.x - 250.0, point.y + 12.0)));
                        }
                    },
                    Icon { name: "more-horizontal".to_string(), class: "rs-ic rs-ic--md".to_string() }
                }
                if handle {
                    // Pressing starts a drag at once (row_gestures.js), so it
                    // stops the row's long-press here.
                    button {
                        r#type: "button",
                        class: "rs-trow__handle",
                        "data-row-handle": "true",
                        "data-testid": "row-handle",
                        aria_label: "Reorder {song.title}",
                        aria_describedby: gesture_list.map(|RowGestureListId(list)| help_id(list)),
                        onpointerdown: move |evt| evt.stop_propagation(),
                        Icon { name: "grip".to_string(), class: "rs-ic rs-ic--md".to_string() }
                    }
                }
            }
        }
    }
}
