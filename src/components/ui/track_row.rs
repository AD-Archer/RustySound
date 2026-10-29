//! A song row: number or art, title, subtitle, marks, duration, "…".

use crate::api::Song;
use crate::components::ui::env::{format_clock, sleep_ms};
use crate::components::ui::{Artwork, TrackMenuController};
use crate::components::Icon;
use dioxus::prelude::*;

const LONG_PRESS_MS: u64 = 450;
const LONG_PRESS_SLOP_PX: f64 = 10.0;

/// One song in a list (`li`), for use inside an `InsetList`.
///
/// Tap/click/Enter plays (`on_play`). The track menu opens on a touch
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
    on_play: EventHandler<()>,
) -> Element {
    let menu = use_context::<TrackMenuController>();
    // Gesture bookkeeping. None of these are read during render, so writing
    // them never re-renders the row.
    let mut press_generation = use_signal(|| 0u64);
    let mut press_origin = use_signal(|| None::<(f64, f64)>);
    let mut long_pressed = use_signal(|| false);

    let open_menu = {
        let song = song.clone();
        move |anchor: Option<(f64, f64)>| {
            let mut menu = menu;
            menu.open(song.clone(), anchor);
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
        li { "data-index": index.map(|i| i.to_string()),
            div {
                class: "rs-trow",
                class: if !numbered { "rs-trow--art" },
                class: if playing { "rs-trow--playing" },
                role: "button",
                tabindex: "0",
                aria_label: "{song.title}",
                aria_current: if playing { Some("true") } else { None },
                "data-testid": "track-row",
                onclick: move |_| {
                    if *long_pressed.peek() {
                        long_pressed.set(false);
                        return;
                    }
                    on_play.call(());
                },
                onkeydown: {
                    let open_menu = open_menu.clone();
                    move |evt: KeyboardEvent| {
                        let key = evt.key();
                        if key == Key::Enter || key == Key::Character(" ".to_string()) {
                            evt.prevent_default();
                            on_play.call(());
                        } else if key == Key::ContextMenu
                            || (key == Key::F10 && evt.modifiers().shift())
                        {
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
                div { class: "rs-trow__meta",
                    div { class: "rs-trow__title", "{song.title}" }
                    if let Some(sub) = sub {
                        div { class: "rs-trow__sub", "{sub}" }
                    }
                }
                if starred || downloaded {
                    span { class: "rs-trow__marks",
                        if starred {
                            span { class: "rs-trow__star", role: "img", aria_label: "Favorite",
                                Icon { name: "star-filled".to_string(), class: "rs-ic rs-ic--xs".to_string() }
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
            }
        }
    }
}
