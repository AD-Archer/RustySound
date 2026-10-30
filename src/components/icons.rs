//! The icon set: inline SVG on a 24px grid, one 1.6 stroke weight, round caps
//! and joins (SF Symbols-like). Transport glyphs (play, pause, prev, next) and
//! the "-filled" variants are solid. Size comes from the `class` (for kit code
//! use `rs-ic`, `rs-ic--sm`, `rs-ic--md`, …).
//!
//! Names are stable: legacy views and the kit share them. Unknown names render
//! a plain circle so a typo is visible rather than blank.

use dioxus::prelude::*;

#[derive(Clone, Copy)]
enum Paint {
    /// Outline drawn with the shared stroke.
    Stroke,
    /// Solid glyph.
    Fill,
}

fn icon_body(name: &str) -> (Paint, &'static str) {
    use Paint::*;
    match name {
        // ---- Navigation -------------------------------------------------
        "discover" => (Stroke, r#"<rect x="8" y="4.5" width="8" height="11.5" rx="1.2" fill="currentColor" fill-opacity=".18"/><path d="M2.8 6.6 6 5.3v11.4l-3.2-1.3Z"/><path d="M21.2 6.6 18 5.3v11.4l3.2-1.3Z"/><path d="M7 20h10" opacity=".55"/>"#),
        "library" => (Stroke, r#"<rect x="4" y="8.5" width="16" height="12" rx="2"/><path d="M6.5 5.5h11M9 2.8h6"/><path d="M13.6 11.3v5"/><circle cx="11.8" cy="16.3" r="1.8"/><path d="m13.6 11.3 2.4.8"/>"#),
        "search" => (Stroke, r#"<circle cx="10.5" cy="10.5" r="6.5"/><path d="m20 20-4.8-4.8"/>"#),
        "home" => (Stroke, r#"<path d="M4 10.5 12 4l8 6.5V19a1.5 1.5 0 0 1-1.5 1.5H15v-6H9v6H5.5A1.5 1.5 0 0 1 4 19Z"/>"#),
        "playlist" => (Stroke, r#"<path d="M3.5 6h12M3.5 11.5h12M3.5 17h7"/><path d="M18 13.5v7M14.5 17h7"/>"#),
        "artist" => (Stroke, r#"<circle cx="12" cy="8" r="3.9"/><path d="M4.5 20.5c.8-3.9 3.8-6.3 7.5-6.3s6.7 2.4 7.5 6.3"/>"#),
        "album" => (Stroke, r#"<rect x="4" y="8" width="16" height="13" rx="2"/><path d="M6.5 5h11M9 2.5h6"/>"#),
        "disc" => (Stroke, r#"<circle cx="12" cy="12" r="9.3"/><circle cx="12" cy="12" r="2.6"/>"#),
        "music" => (Stroke, r#"<path d="M9 18V5.5l11-2.2V16"/><circle cx="6.5" cy="18" r="2.6"/><circle cx="17.5" cy="16" r="2.6"/>"#),
        "radio" => (Stroke, r#"<circle cx="12" cy="12" r="1.9" fill="currentColor"/><path d="M8.3 8.3a5.2 5.2 0 0 0 0 7.4M15.7 8.3a5.2 5.2 0 0 1 0 7.4M5.3 5.3a9.5 9.5 0 0 0 0 13.4M18.7 5.3a9.5 9.5 0 0 1 0 13.4"/>"#),
        "stats" | "bars" => (Stroke, r#"<path d="M5 20.5V13M10 20.5V6.5M15 20.5V10M20 20.5V3.5"/>"#),
        "settings" => (Stroke, r#"<path d="M9.87 5.02 L9.89 2.84 L14.11 2.84 L14.13 5.02 L15.43 5.55 L16.98 4.03 L19.97 7.02 L18.45 8.57 L18.98 9.87 L21.16 9.89 L21.16 14.11 L18.98 14.13 L18.45 15.43 L19.97 16.98 L16.98 19.97 L15.43 18.45 L14.13 18.98 L14.11 21.16 L9.89 21.16 L9.87 18.98 L8.57 18.45 L7.02 19.97 L4.03 16.98 L5.55 15.43 L5.02 14.13 L2.84 14.11 L2.84 9.89 L5.02 9.87 L5.55 8.57 L4.03 7.02 L7.02 4.03 L8.57 5.55Z"/><circle cx="12" cy="12" r="3"/>"#),
        "bookmark" => (Stroke, r#"<path d="M6.5 3.5h11v17L12 16.5l-5.5 4Z"/>"#),
        "queue" | "list" => (Stroke, r#"<path d="M8.5 6h12M8.5 12h12M8.5 18h12"/><circle cx="4.2" cy="6" r=".9" fill="currentColor"/><circle cx="4.2" cy="12" r=".9" fill="currentColor"/><circle cx="4.2" cy="18" r=".9" fill="currentColor"/>"#),
        "menu" => (Stroke, r#"<path d="M4 7h16M4 12h16M4 17h16"/>"#),
        "server" => (Stroke, r#"<rect x="3.5" y="4" width="17" height="7" rx="2"/><rect x="3.5" y="13" width="17" height="7" rx="2"/><path d="M7 7.5h.01M7 16.5h.01"/>"#),
        "hard-drive" => (Stroke, r#"<rect x="3.5" y="13" width="17" height="7" rx="2"/><path d="M5 13 8 5h8l3 8"/><path d="M7.5 16.5h.01"/>"#),
        "offline" => (Stroke, r#"<path d="m3.5 3.5 17 17"/><path d="M9.2 5.3A6 6 0 0 1 17.6 10a4 4 0 0 1 2.4 6.6"/><path d="M6.4 8.4A4.5 4.5 0 0 0 7 17.5h10"/>"#),

        // ---- Transport (solid) -----------------------------------------
        "play" => (Fill, r#"<path d="M6 3.8v16.4c0 .8.9 1.3 1.6.9l13-8.2c.6-.4.6-1.4 0-1.8l-13-8.2C6.9 2.5 6 3 6 3.8Z"/>"#),
        "pause" => (Fill, r#"<rect x="5.5" y="4" width="4.6" height="16" rx="1.3"/><rect x="13.9" y="4" width="4.6" height="16" rx="1.3"/>"#),
        "prev" => (Fill, r#"<path d="M21.5 7.3v9.4c0 .7-.8 1.1-1.4.7l-6.8-4.7c-.5-.4-.5-1.1 0-1.4l6.8-4.7c.6-.4 1.4 0 1.4.7Z"/><path d="M12.5 7.3v9.4c0 .7-.8 1.1-1.4.7l-6.8-4.7c-.5-.4-.5-1.1 0-1.4l6.8-4.7c.6-.4 1.4 0 1.4.7Z"/>"#),
        "next" => (Fill, r#"<path d="M2.5 7.3v9.4c0 .7.8 1.1 1.4.7l6.8-4.7c.5-.4.5-1.1 0-1.4L3.9 6.6c-.6-.4-1.4 0-1.4.7Z"/><path d="M11.5 7.3v9.4c0 .7.8 1.1 1.4.7l6.8-4.7c.5-.4.5-1.1 0-1.4l-6.8-4.7c-.6-.4-1.4 0-1.4.7Z"/>"#),
        "shuffle" => (Stroke, r#"<path d="M3 7h2.8c4.6 0 6.8 10 11.4 10H21"/><path d="M3 17h2.8c1.6 0 2.8-1.2 3.8-2.9"/><path d="M13.4 9.9C14.4 8.2 15.6 7 17.2 7H21"/><path d="m18.5 4.5 2.5 2.5-2.5 2.5"/><path d="m18.5 14.5 2.5 2.5-2.5 2.5"/>"#),
        "repeat" => (Stroke, r#"<path d="M4 11.5V10a3 3 0 0 1 3-3h13"/><path d="m17 4 3 3-3 3"/><path d="M20 12.5V14a3 3 0 0 1-3 3H4"/><path d="m7 20-3-3 3-3"/>"#),
        "repeat-1" => (Stroke, r#"<path d="M4 11.5V10a3 3 0 0 1 3-3h13"/><path d="m17 4 3 3-3 3"/><path d="M20 12.5V14a3 3 0 0 1-3 3H4"/><path d="m7 20-3-3 3-3"/><path d="M11.3 10.6 12.6 10v4.6"/>"#),
        "volume" | "volume-2" => (Stroke, r#"<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4Z"/><path d="M15.5 9.5a3.5 3.5 0 0 1 0 5"/><path d="M18.2 7a7 7 0 0 1 0 10"/>"#),
        "volume-1" => (Stroke, r#"<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4Z"/><path d="M15.5 9.5a3.5 3.5 0 0 1 0 5"/>"#),
        "volume-x" => (Stroke, r#"<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4Z"/><path d="m16 9.5 5 5M21 9.5l-5 5"/>"#),

        // ---- Song actions ----------------------------------------------
        "play-next" => (Stroke, r#"<path d="M3.5 4.5v6l5-3Z" fill="currentColor"/><path d="M12 7.5h9M3.5 13.5h17.5M3.5 19h17.5"/>"#),
        "add-to-queue" => (Stroke, r#"<path d="M3.5 5h17.5M3.5 10.5h17.5M12 16.5h9"/><path d="M3.5 13.5v6l5-3Z" fill="currentColor"/>"#),
        "star" => (Stroke, r#"<path d="m12 3.6 2.6 5.3 5.8.8-4.2 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.2-4.1 5.8-.8Z"/>"#),
        "rating" => (Stroke, r#"<path d="m6 8.2 1.1 2.2 2.4.3-1.8 1.7.5 2.4L6 13.6l-2.2 1.2.5-2.4-1.8-1.7 2.4-.3Z"/><path d="m18 8.2 1.1 2.2 2.4.3-1.8 1.7.5 2.4-2.2-1.2-2.2 1.2.5-2.4-1.8-1.7 2.4-.3Z"/><path d="m12 4.2 1.1 2.2 2.4.3-1.8 1.7.5 2.4L12 9.6l-2.2 1.2.5-2.4-1.8-1.7 2.4-.3Z"/><path d="M8 19h8"/>"#),
        "star-filled" => (Stroke, r#"<path d="m12 3.6 2.6 5.3 5.8.8-4.2 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.2-4.1 5.8-.8Z" fill="currentColor"/>"#),
        "heart" => (Stroke, r#"<path d="M12 20s-7.5-4.6-7.5-10.1A4.3 4.3 0 0 1 12 7.4a4.3 4.3 0 0 1 7.5 2.5C19.5 15.4 12 20 12 20Z"/>"#),
        "heart-filled" => (Stroke, r#"<path d="M12 20s-7.5-4.6-7.5-10.1A4.3 4.3 0 0 1 12 7.4a4.3 4.3 0 0 1 7.5 2.5C19.5 15.4 12 20 12 20Z" fill="currentColor"/>"#),
        "download" => (Stroke, r#"<circle cx="12" cy="12" r="9.3"/><path d="M12 7.5v8.5M8.3 12.5 12 16.2l3.7-3.7"/>"#),
        "downloaded" => (Stroke, r#"<circle cx="12" cy="12" r="9.3"/><path d="m8 12.3 2.7 2.7L16 9.7"/>"#),
        "lyrics" => (Stroke, r#"<path d="M5 4.5h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-7.5L7 21v-3.5H5a2 2 0 0 1-2-2v-9a2 2 0 0 1 2-2Z"/><path d="M8 9.5h8M8 12.8h5"/>"#),
        "info" => (Stroke, r#"<circle cx="12" cy="12" r="9.3"/><path d="M12 11v5.5"/><circle cx="12" cy="7.8" r=".9" fill="currentColor"/>"#),
        "more" => (Stroke, r#"<circle cx="12" cy="12" r="9.3"/><circle cx="7.8" cy="12" r="1.15" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="1.15" fill="currentColor" stroke="none"/><circle cx="16.2" cy="12" r="1.15" fill="currentColor" stroke="none"/>"#),
        "more-horizontal" => (Fill, r#"<circle cx="5.5" cy="12" r="1.6"/><circle cx="12" cy="12" r="1.6"/><circle cx="18.5" cy="12" r="1.6"/>"#),
        "plus" => (Stroke, r#"<path d="M12 5v14M5 12h14"/>"#),
        "check" => (Stroke, r#"<path d="m5 12.5 4.5 4.5L19 7.5"/>"#),
        "x" => (Stroke, r#"<path d="M6 6l12 12M18 6 6 18"/>"#),
        "edit" => (Stroke, r#"<path d="M14.5 5.5l4 4L8.5 19.5h-4v-4Z"/><path d="m12.5 7.5 4 4"/>"#),
        "trash" => (Stroke, r#"<path d="M4.5 6.5h15"/><path d="M9.5 6.5V4.8c0-.7.5-1.3 1.2-1.3h2.6c.7 0 1.2.6 1.2 1.3v1.7"/><path d="m6.5 6.5.9 12.6c.1 1 .9 1.9 2 1.9h5.2c1.1 0 1.9-.9 2-1.9l.9-12.6"/><path d="M10 10.5v6M14 10.5v6"/>"#),
        "external" => (Stroke, r#"<path d="M14 4.5h5.5V10"/><path d="M19.5 4.5 11 13"/><path d="M17.5 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8.5a2 2 0 0 1 2-2h4"/>"#),

        // ---- Chevrons and arrows ---------------------------------------
        "chevron-left" | "back" => (Stroke, r#"<path d="m15 5-7 7 7 7"/>"#),
        "chevron-right" => (Stroke, r#"<path d="m9 5 7 7-7 7"/>"#),
        "chevron-up" => (Stroke, r#"<path d="m5 15 7-7 7 7"/>"#),
        "chevron-down" => (Stroke, r#"<path d="m5 9 7 7 7-7"/>"#),
        "arrow-left" => (Stroke, r#"<path d="M20 12H5M11 5l-7 7 7 7"/>"#),
        "arrow-right" => (Stroke, r#"<path d="M4 12h15M13 5l7 7-7 7"/>"#),
        "arrow-down" => (Stroke, r#"<path d="M12 4v15M5 13l7 7 7-7"/>"#),

        // ---- Misc ------------------------------------------------------
        "loader" => (Stroke, r#"<path d="M12 3a9 9 0 1 0 9 9"/>"#),
        "clock" => (Stroke, r#"<circle cx="12" cy="12" r="9.3"/><path d="M12 7v5.2l3.4 2"/>"#),
        "eye" => (Stroke, r#"<path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z"/><circle cx="12" cy="12" r="3"/>"#),
        "refresh-cw" => (Stroke, r#"<path d="M20 11a8 8 0 0 0-14.3-4.9L4 8"/><path d="M4 3.5V8h4.5"/><path d="M4 13a8 8 0 0 0 14.3 4.9L20 16"/><path d="M20 20.5V16h-4.5"/>"#),
        "file-text" => (Stroke, r#"<path d="M14 3.5H7a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-10Z"/><path d="M14 3.5v5h5"/><path d="M8.5 13h7M8.5 16.5h5"/>"#),
        "zap" => (Stroke, r#"<path d="M13 2.5 4.5 13.5H12l-1 8 8.5-11H12Z"/>"#),
        "swatch" => (Stroke, r#"<rect x="3" y="3" width="7.5" height="7.5" rx="1.5"/><rect x="13.5" y="3" width="7.5" height="7.5" rx="1.5"/><rect x="3" y="13.5" width="7.5" height="7.5" rx="1.5"/><rect x="13.5" y="13.5" width="7.5" height="7.5" rx="1.5"/>"#),
        _ => (Stroke, r#"<circle cx="12" cy="12" r="9"/>"#),
    }
}

/// An icon from the shared set. `name` picks the glyph, `class` sizes and
/// colours it (the glyph uses `currentColor`).
#[component]
pub fn Icon(name: String, class: String) -> Element {
    let (paint, body) = icon_body(name.as_str());
    match paint {
        Paint::Stroke => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.6",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                "aria-hidden": "true",
                dangerous_inner_html: body,
            }
        },
        Paint::Fill => rsx! {
            svg {
                class: "{class}",
                view_box: "0 0 24 24",
                fill: "currentColor",
                "aria-hidden": "true",
                dangerous_inner_html: body,
            }
        },
    }
}
