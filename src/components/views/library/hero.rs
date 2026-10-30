//! The stage band at the top of album, artist and playlist pages.

use crate::components::ui::{IconButton, IconButtonSize, Stage};
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

/// Black stage band: back link and "…" on top, then artwork with a subtle
/// reflection and the title block beside it (phone-album mock).
/// * `art`: the artwork element (sized by the band's CSS).
/// * `subtitle`: accent line under the title (artist links, owner).
/// * `meta`: "Genre · Year · 8 songs, 32 min". `badge`: e.g. "Downloaded".
/// * `back_label` / `fallback`: back link text and where it goes when there
///   is no history. `on_more`: shows the "…" button.
/// * `round`: artist variant (round art).
/// * `eyebrow`: a small line above the title (a mix's "Made by RustySound").
#[component]
pub fn DetailHero(
    title: String,
    art: Element,
    back_label: String,
    fallback: AppView,
    #[props(default)] eyebrow: Option<Element>,
    #[props(default)] subtitle: Option<Element>,
    #[props(default)] meta: Option<String>,
    #[props(default)] badge: Option<String>,
    #[props(default)] note: Option<String>,
    #[props(default)] on_more: Option<EventHandler<MouseEvent>>,
    #[props(default)] more_testid: Option<String>,
    #[props(default)] round: bool,
) -> Element {
    let navigation = use_context::<Navigation>();
    rsx! {
        Stage { class: "rs-library-hero".to_string(), label: title.clone(),
            div { class: "rs-library-hero__nav",
                button {
                    r#type: "button",
                    class: "rs-library-back",
                    onclick: move |_| {
                        if navigation.can_go_back() {
                            navigation.go_back();
                        } else {
                            navigation.navigate_to(fallback.clone());
                        }
                    },
                    Icon { name: "chevron-left".to_string(), class: "rs-ic rs-ic--md".to_string() }
                    "{back_label}"
                }
                if let Some(handler) = on_more {
                    IconButton {
                        icon: "more".to_string(),
                        label: "More actions".to_string(),
                        size: IconButtonSize::Medium,
                        icon_class: "rs-ic rs-ic--md".to_string(),
                        testid: more_testid.clone(),
                        class: "rs-library-hero__more".to_string(),
                        onclick: move |evt| handler.call(evt),
                    }
                }
            }
            div { class: "rs-library-hero__body",
                div { class: "rs-library-hero__art", class: if round { "rs-library-hero__art--round" }, {art} }
                div { class: "rs-library-hero__text",
                    if let Some(eyebrow) = eyebrow {
                        div { class: "rs-library-hero__eyebrow", {eyebrow} }
                    }
                    h1 { class: "rs-library-hero__title", "{title}" }
                    if let Some(sub) = subtitle {
                        div { class: "rs-library-hero__sub", {sub} }
                    }
                    if let Some(meta) = meta.clone() {
                        div { class: "rs-library-hero__meta", "{meta}" }
                    }
                    if let Some(badge) = badge.clone() {
                        div { class: "rs-library-hero__badge",
                            Icon { name: "downloaded".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                            "{badge}"
                        }
                    }
                    if let Some(note) = note.clone() {
                        p { class: "rs-library-hero__note", "{note}" }
                    }
                }
            }
        }
    }
}

/// Stage band placeholder while a detail page loads.
#[component]
pub fn DetailHeroSkeleton(back_label: String, fallback: AppView, #[props(default)] round: bool) -> Element {
    let navigation = use_context::<Navigation>();
    rsx! {
        Stage { class: "rs-library-hero".to_string(),
            div { class: "rs-library-hero__nav",
                button {
                    r#type: "button",
                    class: "rs-library-back",
                    onclick: move |_| {
                        if navigation.can_go_back() {
                            navigation.go_back();
                        } else {
                            navigation.navigate_to(fallback.clone());
                        }
                    },
                    Icon { name: "chevron-left".to_string(), class: "rs-ic rs-ic--md".to_string() }
                    "{back_label}"
                }
            }
            div { class: "rs-library-hero__body", aria_busy: "true",
                div { class: "rs-library-hero__art", class: if round { "rs-library-hero__art--round" },
                    div { class: "rs-library-hero__ph" }
                }
                div { class: "rs-library-hero__text",
                    div { class: "rs-library-hero__ph-line", style: "width:70%" }
                    div { class: "rs-library-hero__ph-line", style: "width:45%" }
                    div { class: "rs-library-hero__ph-line rs-library-hero__ph-line--sm", style: "width:55%" }
                }
            }
        }
    }
}
