//! "Outside your server": the last shelf on Home, shown only when Settings →
//! Discovery allows music that isn't on the server. Cards are never playable
//! from the server: tapping one (or its play icon) plays a 30-second preview,
//! and "Find" (link out) lives in the card's "…" menu.

use super::actions::Toast;
use super::cards::CARD_ART_PX;
use super::data::HomeData;
use super::mixes::{HomeMenu, HomeMenus};
use super::preview::{Preview, PreviewEnd};
use super::shelf::{Card, Shelf, ShelfSkeleton};
use crate::api::discovery::{OffServerItem, OffServerKind};
use crate::components::ui::{Artwork, MenuPopover, MenuPopoverItem};
use crate::components::{AppView, Icon, Navigation};
use crate::db::AppSettings;
use dioxus::prelude::*;

const TITLE: &str = "Outside your server";
const TESTID: &str = "shelf-offserver";

/// The bottom shelf. Hidden when the setting is off or offline; when it comes
/// back empty and no Last.fm / ListenBrainz account is set, a quiet line
/// points to Settings instead (the only shelf with an empty note).
#[component]
pub fn OffServerShelf() -> Element {
    let data = use_context::<HomeData>();
    let settings = use_context::<Signal<AppSettings>>();
    let navigation = use_context::<Navigation>();
    if !(data.offserver_enabled)() || (data.offline)() {
        return rsx! {};
    }
    let section = data.off_server.read();
    let Some(section) = section.as_ref() else {
        return rsx! {
            ShelfSkeleton { title: TITLE.to_string(), testid: TESTID.to_string() }
        };
    };
    if section.items.is_empty() {
        let configured = {
            let s = settings.read();
            !s.lastfm_api_key.trim().is_empty() || !s.listenbrainz_username.trim().is_empty()
        };
        if configured {
            return rsx! {};
        }
        return rsx! {
            section { class: "rs-shelf", "data-testid": TESTID,
                div { class: "rs-shelf__hd",
                    h2 { class: "rs-shelf__title", "{TITLE}" }
                }
                p { class: "rs-home__note",
                    "Connect Last.fm or ListenBrainz for picks from outside your library. "
                    button {
                        r#type: "button",
                        class: "rs-home__link",
                        "data-testid": "offserver-connect",
                        onclick: move |_| navigation.navigate_to(AppView::SettingsView {}),
                        "Open Settings"
                    }
                }
            }
        };
    }
    rsx! {
        Shelf { title: TITLE.to_string(), testid: TESTID.to_string(),
            for item in section.items.iter() {
                OffServerCard { key: "{item.id}", item: item.clone() }
            }
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

/// "Find on Apple Music" etc., from the link's host.
pub fn find_label(url: &str) -> String {
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let site = if host.ends_with("apple.com") {
        "Apple Music"
    } else if host.ends_with("last.fm") {
        "Last.fm"
    } else if host.ends_with("musicbrainz.org") {
        "MusicBrainz"
    } else if host.ends_with("listenbrainz.org") {
        "ListenBrainz"
    } else {
        return "Find Online".to_string();
    };
    format!("Find on {site}")
}

/// Start or stop `item`'s preview (main playback pauses meanwhile and resumes after).
fn toggle_preview(preview: Preview, toast: Toast, item: &OffServerItem) {
    let Some(url) = item.preview_url.clone() else {
        return;
    };
    let title = item.title.clone();
    preview.toggle(item.id.clone(), url, move |end| {
        if end == PreviewEnd::Failed {
            toast.show(format!("The preview for “{title}” couldn't play."), true);
        }
    });
}

/// One recommendation: art, title, artist, reason and a source tag.
#[component]
fn OffServerCard(item: OffServerItem) -> Element {
    let preview = use_context::<Preview>();
    let toast = use_context::<Toast>();
    let menus = use_context::<HomeMenus>();
    let previewing = (preview.current)().as_deref() == Some(item.id.as_str());
    let is_artist = item.kind == OffServerKind::Artist;
    let has_preview = item.preview_url.is_some();
    let subtitle = if is_artist {
        "Artist".to_string()
    } else {
        item.artist.clone()
    };
    let main_item = item.clone();
    let menu_item = item.clone();
    let overlay_item = item.clone();
    let overlay = has_preview.then(|| {
        rsx! {
            button {
                r#type: "button",
                class: "rs-hcard__play",
                class: if previewing { "rs-hcard__play--on" },
                aria_pressed: if previewing { "true" } else { "false" },
                aria_label: if previewing { format!("Stop preview of {}", item.title) } else { format!("Preview {}", item.title) },
                title: if previewing { "Stop preview" } else { "Preview" },
                "data-testid": "offserver-preview",
                onclick: move |_| toggle_preview(preview, toast, &overlay_item),
                Icon {
                    name: if previewing { "pause".to_string() } else { "play".to_string() },
                    class: "rs-ic rs-ic--sm".to_string(),
                }
            }
        }
    });
    rsx! {
        Card {
            title: item.title.clone(),
            subtitle,
            reason: Some(item.reason.clone()),
            tag: Some(item.source.label().to_string()),
            testid: "offserver-card".to_string(),
            pressed: has_preview.then_some(previewing),
            round: is_artist,
            art: rsx! {
                Artwork {
                    src: item.art_url.clone(),
                    size_px: CARD_ART_PX,
                    fill: true,
                    round: is_artist,
                    placeholder: placeholder(item.kind).to_string(),
                }
            },
            overlay,
            onclick: move |evt: MouseEvent| {
                if main_item.preview_url.is_some() {
                    toggle_preview(preview, toast, &main_item);
                } else {
                    let point = evt.client_coordinates();
                    menus.open(HomeMenu::OffServer(main_item.clone()), Some((point.x, point.y)));
                }
            },
            on_menu: move |anchor| menus.open(HomeMenu::OffServer(menu_item.clone()), anchor),
        }
    }
}

/// The off-server card's "…" menu: Preview / Stop and Find (a link out).
#[component]
pub fn OffServerMenu(item: OffServerItem, anchor: Option<(f64, f64)>, on_close: EventHandler<()>) -> Element {
    let preview = use_context::<Preview>();
    let toast = use_context::<Toast>();
    let previewing = (preview.current)().as_deref() == Some(item.id.as_str());
    let find = item.external_url.as_deref().map(find_label).unwrap_or_default();
    let preview_item = item.clone();
    rsx! {
        MenuPopover {
            label: item.title.clone(),
            title: Some(format!("{} · {}", item.title, item.reason)),
            anchor,
            testid: "offserver-menu".to_string(),
            on_close: move |_| on_close.call(()),
            if item.preview_url.is_some() {
                MenuPopoverItem {
                    label: if previewing { "Stop Preview".to_string() } else { "Preview".to_string() },
                    icon: Some(if previewing { "pause".to_string() } else { "play".to_string() }),
                    testid: "offserver-menu-preview".to_string(),
                    onclick: move |_| {
                        toggle_preview(preview, toast, &preview_item);
                        on_close.call(());
                    },
                }
            }
            if let Some(url) = item.external_url.clone() {
                a {
                    class: "rs-menu__item",
                    role: "menuitem",
                    href: "{url}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "data-testid": "offserver-find",
                    onclick: move |_| on_close.call(()),
                    span { "{find}" }
                    Icon { name: "external".to_string(), class: "rs-ic".to_string() }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::find_label;

    #[test]
    fn find_labels_name_the_site() {
        assert_eq!(find_label("https://music.apple.com/us/album/x/1"), "Find on Apple Music");
        assert_eq!(find_label("https://www.last.fm/music/Ilse+Varga"), "Find on Last.fm");
        assert_eq!(find_label("https://musicbrainz.org/release-group/abc"), "Find on MusicBrainz");
        assert_eq!(find_label("https://example.com/x"), "Find Online");
        assert_eq!(find_label("not a url"), "Find Online");
    }
}
