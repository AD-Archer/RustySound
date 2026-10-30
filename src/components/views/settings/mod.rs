//! Settings, as iOS-style inset grouped lists on the page material.
//!
//! Compact: a list of sections that pushes to a detail page per section.
//! Wide: two panes, the section list on the left and the detail on the right.
//! One file per section; `shared.rs` holds the rows and the save context.

mod about;
mod advanced;
mod appearance;
mod discovery;
mod discovery_check;
mod first_run;
mod lyrics;
mod playback;
mod server_form;
mod servers;
mod shared;
mod smart_cache;
mod storage;
mod storage_downloads;

pub use first_run::FirstRun;

use crate::api::ServerConfig;
use crate::components::ui::use_layout;
use crate::components::Icon;
use dioxus::prelude::*;
use shared::SettingsCtx;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Section {
    Servers,
    Appearance,
    Discovery,
    Playback,
    Storage,
    Lyrics,
    Advanced,
    About,
}

impl Section {
    const ALL: [Section; 8] = [
        Section::Servers,
        Section::Appearance,
        Section::Discovery,
        Section::Playback,
        Section::Storage,
        Section::Lyrics,
        Section::Advanced,
        Section::About,
    ];

    /// The list groups, iOS style.
    const GROUPS: [&'static [Section]; 3] = [
        &[Section::Appearance, Section::Discovery],
        &[Section::Playback, Section::Storage, Section::Lyrics],
        &[Section::Advanced, Section::About],
    ];

    fn title(self) -> &'static str {
        match self {
            Section::Servers => "Servers",
            Section::Appearance => "Appearance",
            Section::Discovery => "Discovery",
            Section::Playback => "Playback",
            Section::Storage => "Storage & Downloads",
            Section::Lyrics => "Lyrics",
            Section::Advanced => "Advanced",
            Section::About => "About",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Section::Servers => "server",
            Section::Appearance => "swatch",
            Section::Discovery => "discover",
            Section::Playback => "play",
            Section::Storage => "hard-drive",
            Section::Lyrics => "lyrics",
            Section::Advanced => "zap",
            Section::About => "info",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Section::Servers => "servers",
            Section::Appearance => "appearance",
            Section::Discovery => "discovery",
            Section::Playback => "playback",
            Section::Storage => "storage",
            Section::Lyrics => "lyrics",
            Section::Advanced => "advanced",
            Section::About => "about",
        }
    }
}

#[component]
pub fn SettingsView() -> Element {
    let ctx = SettingsCtx::provide();
    let layout = use_layout();
    let mut selected = use_signal(|| None::<Section>);
    let servers = use_context::<Signal<Vec<ServerConfig>>>();

    let toast = ctx.toast_text();
    let compact = layout.is_compact();

    // A pushed page (or another section on wide) starts at the top.
    use_effect(move || {
        let _ = selected();
        let _ = document::eval("document.querySelector('.rs-main')?.scrollTo(0, 0)");
    });

    let body = if compact {
        match selected() {
            None => rsx! {
                div { class: "rs-page-head",
                    h1 { class: "rs-large-title", "Settings" }
                }
                SectionNav { current: None, servers: servers(), on_pick: move |s| selected.set(Some(s)) }
            },
            Some(section) => rsx! {
                div { class: "rs-settings-back",
                    button {
                        r#type: "button",
                        class: "rs-settings-back__btn",
                        "data-testid": "settings-back",
                        onclick: move |_| selected.set(None),
                        Icon { name: "chevron-left".to_string(), class: "rs-ic".to_string() }
                        "Settings"
                    }
                }
                h1 { class: "rs-large-title rs-settings-detail-title", "{section.title()}" }
                SectionBody { section }
            },
        }
    } else {
        let section = selected().unwrap_or(Section::Servers);
        rsx! {
            div { class: "rs-settings-split",
                nav { class: "rs-settings-split__nav", aria_label: "Settings sections",
                    h1 { class: "rs-large-title rs-settings-split__title", "Settings" }
                    SectionNav { current: Some(section), servers: servers(), on_pick: move |s| selected.set(Some(s)) }
                }
                section {
                    class: "rs-settings-split__detail",
                    aria_labelledby: "settings-detail-title",
                    h2 { class: "rs-large-title rs-settings-detail-title", id: "settings-detail-title",
                        "{section.title()}"
                    }
                    SectionBody { key: "{section.key()}", section }
                }
            }
        }
    };

    rsx! {
        div {
            class: "rs-view rs-settings",
            class: if compact { "rs-settings--compact" } else { "rs-settings--wide" },
            "data-testid": "settings",
            {body}
            div { class: "rs-settings-toast-host", role: "status", aria_live: "polite",
                if let Some(message) = toast {
                    span { class: "rs-settings-toast", "{message}" }
                }
            }
        }
    }
}

/// The section list. On wide it marks the current section and supports
/// Up/Down arrows; on compact every row pushes a detail page.
#[component]
fn SectionNav(
    current: Option<Section>,
    servers: Vec<ServerConfig>,
    on_pick: EventHandler<Section>,
) -> Element {
    let active = servers.iter().filter(|s| s.active).count();
    let server_summary = match (servers.len(), active) {
        (0, _) => "No server yet".to_string(),
        (_, 0) => "All servers are off".to_string(),
        (1, _) => "1 server".to_string(),
        (n, a) => format!("{a} of {n} on"),
    };
    let primary = servers
        .iter()
        .find(|s| s.active)
        .or_else(|| servers.first())
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "Servers".to_string());
    let led = if active > 0 { "rs-led" } else { "rs-led rs-led--off" };

    let pick = move |section: Section| {
        move |_| {
            on_pick.call(section);
        }
    };
    let on_key = move |evt: KeyboardEvent| {
        let Some(now) = current else { return };
        let index = Section::ALL.iter().position(|s| *s == now).unwrap_or(0);
        let next = match evt.key() {
            Key::ArrowDown => Section::ALL.get(index + 1).copied(),
            Key::ArrowUp => index.checked_sub(1).and_then(|i| Section::ALL.get(i).copied()),
            Key::Home => Section::ALL.first().copied(),
            Key::End => Section::ALL.last().copied(),
            _ => None,
        };
        if let Some(next) = next {
            evt.prevent_default();
            on_pick.call(next);
            let _ = document::eval(&format!(
                "document.getElementById('settings-nav-{}')?.focus()",
                next.key()
            ));
        }
    };

    rsx! {
        div { class: "rs-settings-nav", onkeydown: on_key,
            ul { class: "rs-list rs-list--icons", aria_label: "Servers",
                NavRow {
                    section: Section::Servers,
                    current,
                    title: primary,
                    subtitle: Some(server_summary),
                    led: Some(led.to_string()),
                    onclick: pick(Section::Servers),
                }
            }
            for (index, group) in Section::GROUPS.iter().enumerate() {
                ul { key: "{index}", class: "rs-list rs-list--icons",
                    for section in group.iter().copied() {
                        NavRow {
                            key: "{section.key()}",
                            section,
                            current,
                            title: section.title().to_string(),
                            onclick: pick(section),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NavRow(
    section: Section,
    current: Option<Section>,
    title: String,
    #[props(default)] subtitle: Option<String>,
    #[props(default)] led: Option<String>,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let is_current = current == Some(section);
    let wide = current.is_some();
    rsx! {
        li {
            button {
                r#type: "button",
                id: "settings-nav-{section.key()}",
                class: "rs-row rs-row--compact rs-settings-nav__row",
                class: if is_current { "rs-settings-nav__row--on" },
                "data-testid": "settings-nav-{section.key()}",
                aria_current: if is_current { Some("page") } else { None },
                // Roving focus on wide: only the current row is in the tab order;
                // arrows move between rows.
                tabindex: if wide && !is_current { "-1" } else { "0" },
                onclick: move |evt| onclick.call(evt),
                span { class: "rs-row__lead",
                    Icon { name: section.icon().to_string(), class: "rs-ic rs-ic--md".to_string() }
                }
                span { class: "rs-row__meta",
                    span { class: "rs-row__title",
                        span { "{title}" }
                    }
                    if let Some(sub) = subtitle {
                        span { class: "rs-row__sub rs-settings-nav__sub",
                            if let Some(led) = led {
                                span { class: "{led}", aria_hidden: "true" }
                            }
                            "{sub}"
                        }
                    }
                }
                if !wide {
                    span { class: "rs-row__chev",
                        Icon { name: "chevron-right".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                    }
                }
            }
        }
    }
}

#[component]
fn SectionBody(section: Section) -> Element {
    match section {
        Section::Servers => rsx! { servers::ServersSection {} },
        Section::Appearance => rsx! { appearance::AppearanceSection {} },
        Section::Discovery => rsx! { discovery::DiscoverySection {} },
        Section::Playback => rsx! { playback::PlaybackSection {} },
        Section::Storage => rsx! { storage::StorageSection {} },
        Section::Lyrics => rsx! { lyrics::LyricsSection {} },
        Section::Advanced => rsx! { advanced::AdvancedSection {} },
        Section::About => rsx! { about::AboutSection {} },
    }
}
