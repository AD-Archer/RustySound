//! "Outside your server": picks that aren't in the library. Never playable
//! from the server; they offer a 30-second Preview and a "Find" link out.

use super::actions::Toast;
use super::preview::{Preview, PreviewEnd};
use super::DiscoverData;
use crate::api::discovery::{OffServerItem, OffServerKind};
use crate::components::ui::{Artwork, InsetList, ListInset, ListRow, SectionHeader, Skeleton};
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

/// Rows shown before "See All".
const COLLAPSED_ROWS: usize = 5;

/// Header plus the list (or its loading / empty state).
#[component]
pub fn OffServerList(wide: bool) -> Element {
    let data = use_context::<DiscoverData>();
    let navigation = use_context::<Navigation>();
    let mut expanded = use_signal(|| false);
    let offline = (data.offline)();
    let section = data.off_server.read();

    let total = section.as_ref().map(|s| s.items.len()).unwrap_or(0);
    let can_expand = total > COLLAPSED_ROWS;
    let shown = if expanded() { total } else { total.min(COLLAPSED_ROWS) };
    let action = can_expand.then(|| if expanded() { "Show Less" } else { "See All" }.to_string());

    rsx! {
        SectionHeader {
            title: "Outside your server".to_string(),
            action,
            on_action: if can_expand { Some(EventHandler::new(move |_: MouseEvent| expanded.toggle())) } else { None },
            testid: "offserver-see-all".to_string(),
        }
        match section.as_ref() {
            None => rsx! {
                Skeleton { rows: 3 }
            },
            Some(_) if offline => rsx! {
                InsetList { inset: ListInset::Text, label: "Outside your server".to_string(),
                    li { class: "rs-discover__quiet", "Picks from outside your library need a connection." }
                }
            },
            Some(found) if found.items.is_empty() => rsx! {
                InsetList { inset: ListInset::Icon, label: "Outside your server".to_string(),
                    ListRow {
                        title: "Connect Last.fm or ListenBrainz".to_string(),
                        subtitle: "for picks from outside your library".to_string(),
                        icon: "zap".to_string(),
                        chevron: true,
                        testid: "offserver-connect".to_string(),
                        onclick: move |_| navigation.navigate_to(AppView::SettingsView {}),
                    }
                }
            },
            Some(found) => rsx! {
                InsetList {
                    label: "Outside your server".to_string(),
                    testid: "offserver-list".to_string(),
                    for item in found.items.iter().take(shown) {
                        OffServerRow { key: "{item.id}", item: item.clone(), wide }
                    }
                }
            },
        }
    }
}

fn placeholder(kind: OffServerKind) -> &'static str {
    match kind {
        OffServerKind::Artist => "artist",
        OffServerKind::Album => "album",
        OffServerKind::Track => "music",
    }
}

/// One recommendation: art, title, artist, reason and source, Preview, Find.
#[component]
fn OffServerRow(item: OffServerItem, wide: bool) -> Element {
    let preview = use_context::<Preview>();
    let toast = use_context::<Toast>();
    let previewing = (preview.current)().as_deref() == Some(item.id.as_str());
    let source = item.source.label();
    let is_artist = item.kind == OffServerKind::Artist;
    let who = if is_artist {
        item.preview_title
            .as_ref()
            .map(|t| format!("Artist · Preview: {t}"))
            .unwrap_or_else(|| "Artist".to_string())
    } else {
        item.artist.clone()
    };

    let toggle_preview = {
        let id = item.id.clone();
        let url = item.preview_url.clone();
        let title = item.title.clone();
        move |_| {
            let Some(url) = url.clone() else {
                return;
            };
            let title = title.clone();
            preview.toggle(id.clone(), url, move |end| {
                if end == PreviewEnd::Failed {
                    toast.show(format!("The preview for “{title}” couldn't play."), true);
                }
            });
        }
    };

    rsx! {
        li { "data-testid": "offserver-row",
            div { class: "rs-row rs-discover-off",
                Artwork {
                    src: item.art_url.clone(),
                    size_px: 44,
                    round: is_artist,
                    placeholder: placeholder(item.kind).to_string(),
                }
                span { class: "rs-row__meta",
                    span { class: "rs-row__title",
                        span { "{item.title}" }
                    }
                    if wide {
                        span { class: "rs-row__sub", style: "display:block", "{who} · {item.reason}" }
                    } else {
                        span { class: "rs-row__sub", style: "display:block", "{who}" }
                        span { class: "rs-row__why", style: "display:block", "{item.reason} · {source}" }
                    }
                }
                if wide {
                    span { class: "rs-badge", "{source}" }
                }
                if item.preview_url.is_some() {
                    button {
                        r#type: "button",
                        class: "rs-pill rs-pill--mini",
                        aria_pressed: if previewing { "true" } else { "false" },
                        aria_label: if previewing { format!("Stop preview of {}", item.title) } else { format!("Preview {}", item.title) },
                        "data-testid": "offserver-preview",
                        onclick: toggle_preview,
                        if previewing {
                            Icon { name: "pause".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                            "Stop"
                        } else {
                            "Preview"
                        }
                    }
                }
                if let Some(url) = item.external_url.clone() {
                    if wide || item.preview_url.is_none() {
                        a {
                            class: "rs-pill rs-pill--mini",
                            href: "{url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            aria_label: "Find {item.title} on {source}",
                            "data-testid": "offserver-find",
                            "Find"
                        }
                    } else {
                        a {
                            class: "rs-iconbtn rs-iconbtn--sm rs-iconbtn--quiet",
                            href: "{url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            aria_label: "Find {item.title}",
                            title: "Find",
                            "data-testid": "offserver-find",
                            Icon { name: "external".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                        }
                    }
                }
            }
        }
    }
}
