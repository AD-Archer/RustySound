//! Home's stage: the title row (with the server status, which opens
//! Settings) and Cover Flow of recently played albums. Tapping the centre
//! cover opens the album; the round play button beside the caption plays it.

use super::actions::{play_album, use_player, Toast};
use super::data::HomeData;
use crate::api::discovery::DiscoverAlbum;
use crate::api::ServerConfig;
use crate::components::ui::{cover_art_url, CoverFlow, CoverFlowItem, IconButton, IconButtonSize, Stage};
use crate::components::{AppView, Icon, Navigation};
use crate::db::AppSettings;
use dioxus::prelude::*;

/// Cover edge in px: phone and Mac sizes from the mocks.
const COVER_COMPACT: u32 = 148;
const COVER_WIDE: u32 = 216;

/// Index Cover Flow rests on: the third most recent, so covers sit on both
/// sides (starting on the newest leaves the left half of the stage empty).
/// The newest is two flips to the left.
pub fn initial_centre(count: usize) -> usize {
    (count / 2).min(2)
}

fn flow_items(albums: &[DiscoverAlbum], servers: &[ServerConfig], cover_px: u32) -> Vec<CoverFlowItem> {
    albums
        .iter()
        .map(|pick| {
            let album = &pick.album;
            CoverFlowItem {
                key: format!("{}::{}", album.server_id, album.id),
                title: album.name.clone(),
                subtitle: album.artist.clone(),
                reason: pick.reason.clone(),
                art: cover_art_url(servers, &album.server_id, album.cover_art.as_deref(), cover_px * 2)
                    .into_iter()
                    .collect(),
            }
        })
        .collect()
}

/// The black stage at the top of Home.
/// * `wide`: Mac toolbar row, larger covers and the scrubber.
/// * `pull`: pull-to-refresh distance (compact), drawn as an indicator.
#[component]
pub fn HomeStage(wide: bool, pull: Signal<f64>) -> Element {
    let data = use_context::<HomeData>();
    let navigation = use_context::<Navigation>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let cover_px = if wide { COVER_WIDE } else { COVER_COMPACT };

    // Recently played; a library nobody has played yet shows its newest albums.
    let picks = use_memo(move || {
        let recent = data.recent.read();
        let recent = recent.as_ref()?;
        if recent.is_empty() {
            return data.new_albums.read().clone();
        }
        Some(recent.clone())
    });
    let items = use_memo(move || {
        picks
            .read()
            .as_deref()
            .map(|albums| flow_items(albums, &servers.read(), cover_px))
    });

    let open_album = move |index: usize| {
        let picked = picks.peek().as_ref().and_then(|list| list.get(index).cloned());
        if let Some(pick) = picked {
            navigation.navigate_to(AppView::AlbumDetailView {
                album_id: pick.album.id,
                server_id: pick.album.server_id,
            });
        }
    };
    let refreshing = (data.refreshing)();

    rsx! {
        Stage {
            class: if wide { "rs-home__stage rs-home__stage--wide".to_string() } else { "rs-home__stage".to_string() },
            label: "Recently played".to_string(),
            if !wide {
                PullIndicator { pull, refreshing }
            }
            div { class: "rs-home__bar",
                h1 { class: "rs-home__title", "Home" }
                if wide {
                    IconButton {
                        icon: "refresh-cw".to_string(),
                        label: "Refresh Home".to_string(),
                        quiet: true,
                        size: IconButtonSize::Small,
                        disabled: refreshing,
                        class: if refreshing { "rs-home__spin".to_string() } else { String::new() },
                        testid: "home-refresh".to_string(),
                        onclick: move |_| data.refresh(),
                    }
                }
                SettingsButton { wide }
            }
            match items() {
                None => rsx! {
                    FlowSkeleton { cover_px, wide }
                },
                Some(list) if list.is_empty() => rsx! {
                    div { class: "rs-home__stage-empty",
                        p { class: "rs-home__stage-empty-title", "Nothing played yet" }
                        p { "Albums you play show up here." }
                    }
                },
                Some(list) => rsx! {
                    div { class: "rs-home__flow",
                        CoverFlow {
                            initial: initial_centre(list.len()),
                            items: list,
                            cover_px,
                            wide,
                            label: "Recently played albums".to_string(),
                            on_open: open_album,
                            on_select: move |index| {
                                let mut centre = data.centre;
                                centre.set(index);
                            },
                        }
                        CentrePlay { picks }
                    }
                },
            }
        }
    }
}

/// The round play button beside the caption: plays the centred album.
/// Its own component, so flipping covers re-renders only this button.
#[component]
fn CentrePlay(picks: Memo<Option<Vec<DiscoverAlbum>>>) -> Element {
    let data = use_context::<HomeData>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let toast = use_context::<Toast>();
    let player = use_player();
    let name = picks
        .read()
        .as_ref()
        .and_then(|list| list.get((data.centre)()).map(|pick| pick.album.name.clone()))
        .unwrap_or_default();
    rsx! {
        button {
            r#type: "button",
            class: "rs-home__play",
            aria_label: "Play {name}",
            title: "Play",
            "data-testid": "coverflow-play",
            onclick: move |_| {
                let index = *data.centre.peek();
                if let Some(pick) = picks.peek().as_ref().and_then(|list| list.get(index).cloned()) {
                    play_album(player, toast, servers, pick.album);
                }
            },
            Icon { name: "play".to_string(), class: "rs-ic".to_string() }
        }
    }
}

/// Opens Settings: a gear, with a small dot on it when no server is reachable
/// (offline mode, or every server switched off).
#[component]
fn SettingsButton(wide: bool) -> Element {
    let navigation = use_context::<Navigation>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let settings = use_context::<Signal<AppSettings>>();
    let problem = if settings.read().offline_mode {
        Some(("rs-led rs-led--warn", "offline"))
    } else if !servers.read().iter().any(|s| s.active) {
        Some(("rs-led rs-led--off", "no server on"))
    } else {
        None
    };
    let label = match problem {
        Some((_, state)) => format!("Settings, {state}"),
        None => "Settings".to_string(),
    };
    rsx! {
        span { class: "rs-home__settings",
            IconButton {
                icon: "settings".to_string(),
                label,
                quiet: true,
                size: if wide { IconButtonSize::Small } else { IconButtonSize::Medium },
                testid: "settings-open".to_string(),
                onclick: move |_| navigation.navigate_to(AppView::SettingsView {}),
            }
            if let Some((led, _)) = problem {
                span { class: "{led} rs-home__settings-dot", aria_hidden: "true" }
            }
        }
    }
}

/// Dark placeholder with the flow's footprint, so nothing jumps on load.
#[component]
fn FlowSkeleton(cover_px: u32, wide: bool) -> Element {
    rsx! {
        div {
            class: "rs-home__flow-skel",
            class: if wide { "rs-home__flow-skel--wide" },
            style: "--cover:{cover_px}px",
            aria_busy: "true",
            aria_label: "Loading recently played",
            div { class: "rs-home__flow-skel-side" }
            div { class: "rs-home__flow-skel-cover" }
            div { class: "rs-home__flow-skel-side" }
        }
    }
}

/// Pull-to-refresh hint at the top of the stage (compact).
#[component]
fn PullIndicator(pull: Signal<f64>, refreshing: bool) -> Element {
    let distance = pull();
    if distance <= 0.0 && !refreshing {
        return rsx! {};
    }
    let progress = (distance / super::PULL_TRIGGER_PX).min(1.0);
    let style = if refreshing {
        "opacity:1".to_string()
    } else {
        format!(
            "opacity:{progress:.2};transform:translateX(-50%) rotate({:.0}deg)",
            progress * 270.0
        )
    };
    rsx! {
        div {
            class: "rs-home__pull",
            class: if refreshing { "rs-home__pull--busy" },
            style: "{style}",
            role: "status",
            aria_label: if refreshing { "Refreshing" } else if progress >= 1.0 { "Release to refresh" } else { "Pull to refresh" },
            Icon { name: "refresh-cw".to_string(), class: "rs-ic rs-ic--sm".to_string() }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::initial_centre;

    #[test]
    fn flow_rests_with_covers_on_both_sides() {
        assert_eq!(initial_centre(0), 0);
        assert_eq!(initial_centre(1), 0);
        assert_eq!(initial_centre(3), 1);
        assert_eq!(initial_centre(24), 2);
    }
}
