//! The Discover stage: title row with the server pill, Cover Flow of picks,
//! and a quiet Play for the centred album.

use super::actions::{use_player, Toast};
use super::DiscoverData;
use crate::api::discovery::DiscoverAlbum;
use crate::api::{NavidromeClient, QueueSourceKind, ServerConfig};
use crate::components::ui::{
    cover_art_url, CoverFlow, CoverFlowItem, IconButton, IconButtonSize, PillButton, PillVariant,
    Stage,
};
use crate::components::{AppView, Icon, Navigation};
use crate::db::AppSettings;
use dioxus::prelude::*;

/// Cover edge in px: phone and Mac sizes from the mocks.
const COVER_COMPACT: u32 = 148;
const COVER_WIDE: u32 = 216;

/// Index Cover Flow rests on: the third pick, so covers sit on both sides.
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

/// The black stage at the top of Discover.
/// * `wide`: Mac toolbar row, larger covers and the scrubber.
/// * `pull`: pull-to-refresh distance (compact), drawn as an indicator.
#[component]
pub fn DiscoverStage(wide: bool, pull: Signal<f64>) -> Element {
    let data = use_context::<DiscoverData>();
    let navigation = use_context::<Navigation>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let toast = use_context::<Toast>();
    let player = use_player();
    let mut starting = use_signal(|| false);
    let cover_px = if wide { COVER_WIDE } else { COVER_COMPACT };

    let picks = use_memo(move || {
        data.stage
            .read()
            .as_ref()
            .map(|section| section.stage.clone())
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

    let play_centre = move |_| {
        if *starting.peek() {
            return;
        }
        let index = *data.centre.peek();
        let Some(pick) = picks.peek().as_ref().and_then(|list| list.get(index).cloned()) else {
            return;
        };
        let server = servers
            .peek()
            .iter()
            .find(|s| s.id == pick.album.server_id)
            .cloned();
        let Some(server) = server else {
            return;
        };
        starting.set(true);
        spawn(async move {
            let album = pick.album;
            match NavidromeClient::new(server).get_album(&album.id).await {
                Ok((_, songs)) if !songs.is_empty() => {
                    let source = format!("{}::{}", album.server_id, album.id);
                    if let Err(message) = player.play(songs, QueueSourceKind::Album, source, false) {
                        toast.show(message, true);
                    }
                }
                Ok(_) => toast.show(format!("“{}” has no playable songs.", album.name), true),
                Err(_) => toast.show(format!("Couldn't load “{}”.", album.name), true),
            }
            starting.set(false);
        });
    };

    let refreshing = (data.refreshing)();

    rsx! {
        Stage {
            class: if wide { "rs-discover__stage rs-discover__stage--wide".to_string() } else { "rs-discover__stage".to_string() },
            label: "Discover highlights".to_string(),
            if !wide {
                PullIndicator { pull, refreshing }
            }
            div { class: "rs-discover__bar",
                h1 { class: "rs-discover__title", "Discover" }
                if wide {
                    button {
                        r#type: "button",
                        class: "rs-discover__search",
                        onclick: move |_| navigation.navigate_to(AppView::SearchView {}),
                        Icon { name: "search".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                        span { "Search artists, albums or songs" }
                    }
                    IconButton {
                        icon: "refresh-cw".to_string(),
                        label: "Refresh Discover".to_string(),
                        quiet: true,
                        size: IconButtonSize::Small,
                        disabled: refreshing,
                        class: if refreshing { "rs-discover__spin".to_string() } else { String::new() },
                        testid: "discover-refresh".to_string(),
                        onclick: move |_| data.refresh(),
                    }
                }
                ServerPill {}
            }
            match items() {
                None => rsx! {
                    FlowSkeleton { cover_px, wide }
                },
                Some(list) if list.is_empty() => rsx! {
                    div { class: "rs-discover__stage-empty",
                        p { class: "rs-discover__stage-empty-title", "Nothing on the stage yet" }
                        p { "Play some albums and Discover fills up with picks from your library." }
                    }
                },
                Some(list) => rsx! {
                    CoverFlow {
                        key: "{data.flow_key}",
                        initial: initial_centre(list.len()),
                        items: list,
                        cover_px,
                        wide,
                        label: "Albums to revisit and try".to_string(),
                        on_open: open_album,
                        on_select: move |index| {
                            let mut centre = data.centre;
                            centre.set(index);
                        },
                    }
                    div { class: "rs-discover__stage-actions",
                        PillButton {
                            label: if starting() { "Starting…".to_string() } else { "Play".to_string() },
                            icon: "play".to_string(),
                            variant: PillVariant::Mini,
                            disabled: starting(),
                            testid: "coverflow-play".to_string(),
                            onclick: play_centre,
                        }
                    }
                },
            }
        }
    }
}

/// Status pill: the active server (count when several), opens Settings.
#[component]
fn ServerPill() -> Element {
    let navigation = use_context::<Navigation>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let settings = use_context::<Signal<AppSettings>>();
    let offline = settings.read().offline_mode;
    let list = servers.read();
    let active: Vec<&ServerConfig> = list.iter().filter(|s| s.active).collect();
    let name = active
        .first()
        .map(|s| {
            if s.name.trim().is_empty() {
                s.url
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_end_matches('/')
                    .to_string()
            } else {
                s.name.clone()
            }
        })
        .unwrap_or_else(|| "No server".to_string());
    let extra = active.len().saturating_sub(1);
    let (led, state) = if offline {
        ("rs-led rs-led--warn", "offline")
    } else if active.is_empty() {
        ("rs-led rs-led--off", "not connected")
    } else {
        ("rs-led", "connected")
    };
    let aria = if extra > 0 {
        format!("Servers: {name} and {extra} more, {state}. Open Settings")
    } else {
        format!("Server: {name}, {state}. Open Settings")
    };
    rsx! {
        button {
            r#type: "button",
            class: "rs-discover__server",
            aria_label: "{aria}",
            "data-testid": "settings-open",
            onclick: move |_| navigation.navigate_to(AppView::SettingsView {}),
            span { class: "{led}" }
            span { class: "rs-discover__server-name", "{name}" }
            if extra > 0 {
                span { class: "rs-discover__server-count", "+{extra}" }
            }
        }
    }
}

/// Dark placeholder with the flow's footprint, so nothing jumps on load.
#[component]
fn FlowSkeleton(cover_px: u32, wide: bool) -> Element {
    rsx! {
        div {
            class: "rs-discover__flow-skel",
            class: if wide { "rs-discover__flow-skel--wide" },
            style: "--cover:{cover_px}px",
            aria_busy: "true",
            aria_label: "Loading picks",
            div { class: "rs-discover__flow-skel-side" }
            div { class: "rs-discover__flow-skel-cover" }
            div { class: "rs-discover__flow-skel-side" }
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
            class: "rs-discover__pull",
            class: if refreshing { "rs-discover__pull--busy" },
            style: "{style}",
            role: "status",
            aria_label: if refreshing { "Refreshing" } else if progress >= 1.0 { "Release to refresh" } else { "Pull to refresh" },
            Icon { name: "refresh-cw".to_string(), class: "rs-ic rs-ic--sm".to_string() }
        }
    }
}
