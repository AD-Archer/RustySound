use dioxus::prelude::*;

#[component]
pub fn Icon(name: String, class: String) -> Element {
    let svg_content = match name.as_str() {
        "home" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                path { d: "M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" }
                polyline { points: "9 22 9 12 15 12 15 22" }
            }
        },
        "search" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                circle { cx: "11", cy: "11", r: "8" }
                path { d: "M21 21l-4.35-4.35" }
            }
        },
        "album" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                rect {
                    x: "3",
                    y: "3",
                    width: "18",
                    height: "18",
                    rx: "2",
                    ry: "2",
                }
                circle { cx: "12", cy: "12", r: "5" }
                circle { cx: "12", cy: "12", r: "1" }
            }
        },
        "artist" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                circle { cx: "12", cy: "7", r: "4" }
            }
        },
        "playlist" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                path { d: "M21 15V6" }
                path { d: "M18.5 18a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Z" }
                path { d: "M12 12H3" }
                path { d: "M16 6H3" }
                path { d: "M12 18H3" }
            }
        },
        "radio" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                path { d: "M4.9 19.1C1 15.2 1 8.8 4.9 4.9" }
                path { d: "M7.8 16.2c-2.3-2.3-2.3-6.1 0-8.5" }
                circle { cx: "12", cy: "12", r: "2" }
                path { d: "M16.2 7.8c2.3 2.3 2.3 6.1 0 8.5" }
                path { d: "M19.1 4.9C23 8.8 23 15.1 19.1 19" }
            }
        },
        "heart" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                path { d: "M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" }
            }
        },
        "heart-filled" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "currentColor",
                stroke: "currentColor",
                stroke_width: "2",
                path { d: "M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" }
            }
        },
        "settings" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                circle { cx: "12", cy: "12", r: "3" }
                path { d: "M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" }
            }
        },
        "shuffle" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                polyline { points: "16 3 21 3 21 8" }
                line {
                    x1: "4",
                    y1: "20",
                    x2: "21",
                    y2: "3",
                }
                polyline { points: "21 16 21 21 16 21" }
                line {
                    x1: "15",
                    y1: "15",
                    x2: "21",
                    y2: "21",
                }
                line {
                    x1: "4",
                    y1: "4",
                    x2: "9",
                    y2: "9",
                }
            }
        },
        "play" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "currentColor",
                polygon { points: "5 3 19 12 5 21 5 3" }
            }
        },
        "pause" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "currentColor",
                rect {
                    x: "6",
                    y: "4",
                    width: "4",
                    height: "16",
                }
                rect {
                    x: "14",
                    y: "4",
                    width: "4",
                    height: "16",
                }
            }
        },
        "prev" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "currentColor",
                polygon { points: "19 20 9 12 19 4 19 20" }
                line {
                    x1: "5",
                    y1: "19",
                    x2: "5",
                    y2: "5",
                    stroke: "currentColor",
                    stroke_width: "2",
                }
            }
        },
        "next" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "currentColor",
                polygon { points: "5 4 15 12 5 20 5 4" }
                line {
                    x1: "19",
                    y1: "5",
                    x2: "19",
                    y2: "19",
                    stroke: "currentColor",
                    stroke_width: "2",
                }
            }
        },
        "repeat" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                polyline { points: "17 1 21 5 17 9" }
                path { d: "M3 11V9a4 4 0 0 1 4-4h14" }
                polyline { points: "7 23 3 19 7 15" }
                path { d: "M21 13v2a4 4 0 0 1-4 4H3" }
            }
        },
        "volume" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                polygon { points: "11 5 6 9 2 9 2 15 6 15 11 19 11 5" }
                path { d: "M15.54 8.46a5 5 0 0 1 0 7.07" }
                path { d: "M19.07 4.93a10 10 0 0 1 0 14.14" }
            }
        },
        "queue" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                path { d: "M8 6h13" }
                path { d: "M8 12h13" }
                path { d: "M8 18h13" }
                path { d: "M3 6h.01" }
                path { d: "M3 12h.01" }
                path { d: "M3 18h.01" }
            }
        },
        "music" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                path { d: "M9 18V5l12-2v13" }
                circle { cx: "6", cy: "18", r: "3" }
                circle { cx: "18", cy: "16", r: "3" }
            }
        },
        "plus" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                line {
                    x1: "12",
                    y1: "5",
                    x2: "12",
                    y2: "19",
                }
                line {
                    x1: "5",
                    y1: "12",
                    x2: "19",
                    y2: "12",
                }
            }
        },
        "server" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                rect {
                    x: "2",
                    y: "2",
                    width: "20",
                    height: "8",
                    rx: "2",
                    ry: "2",
                }
                rect {
                    x: "2",
                    y: "14",
                    width: "20",
                    height: "8",
                    rx: "2",
                    ry: "2",
                }
                line {
                    x1: "6",
                    y1: "6",
                    x2: "6.01",
                    y2: "6",
                }
                line {
                    x1: "6",
                    y1: "18",
                    x2: "6.01",
                    y2: "18",
                }
            }
        },
        "check" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                polyline { points: "20 6 9 17 4 12" }
            }
        },
        "x" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                line {
                    x1: "18",
                    y1: "6",
                    x2: "6",
                    y2: "18",
                }
                line {
                    x1: "6",
                    y1: "6",
                    x2: "18",
                    y2: "18",
                }
            }
        },
        "loader" => rsx! {
            svg {
                class: "{class} animate-spin",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                circle {
                    cx: "12",
                    cy: "12",
                    r: "10",
                    opacity: "0.25",
                }
                path { d: "M12 2a10 10 0 0 1 10 10", opacity: "0.75" }
            }
        },
        "clock" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                circle { cx: "12", cy: "12", r: "10" }
                polyline { points: "12 6 12 12 16 14" }
            }
        },
        "trash" => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                polyline { points: "3 6 5 6 21 6" }
                path { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" }
            }
        },
        _ => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                circle { cx: "12", cy: "12", r: "10" }
            }
        },
    };

    svg_content
}
