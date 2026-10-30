//! Discover: the app's home at `/` (the route still calls it `HomeView`).
//!
//! A black stage with Cover Flow of picks, then `For You | New | Rewind`:
//! generated mixes, new albums, rewind shelves and picks from outside the
//! library. Data comes from `api::discovery`, loaded progressively (stage,
//! then mixes, then off-server) once per server set, settings change or
//! refresh, and kept in signals so renders never refetch.

mod actions;
mod mixes;
mod offserver;
mod preview;
mod saved;
mod shelves;
mod stage;

use crate::api::discovery::{
    load_discover_mixes, load_discover_off_server, load_discover_rewind, load_discover_stage,
    refresh_discover, DiscoveryConfig, MixesSection, OffServerSection, RewindSection, StageSection,
    DEFAULT_REWIND_MONTHS, REWIND_MONTH_CHOICES,
};
use crate::api::ServerConfig;
use crate::components::ui::{
    use_layout, MainScroll, SectionHeader, Segmented,
};
use crate::db::AppSettings;
use actions::{NoticeCapsule, Toast};
use dioxus::core::Task;
use dioxus::prelude::*;
use mixes::MixList;
use offserver::OffServerList;
use preview::use_preview_provider;
use shelves::{NewShelf, RewindPanel};
use stage::DiscoverStage;

/// Pull distance (after damping) that triggers a refresh on compact.
pub const PULL_TRIGGER_PX: f64 = 64.0;

/// Discover's loaded data and its controls, shared with the sections.
#[derive(Clone, Copy)]
pub struct DiscoverData {
    pub stage: Signal<Option<StageSection>>,
    pub mixes: Signal<Option<MixesSection>>,
    pub off_server: Signal<Option<OffServerSection>>,
    /// The Rewind segment; loads the first time Rewind is opened.
    pub rewind: Resource<Option<RewindSection>>,
    /// The chosen Rewind window in months (3, 6 or 12).
    pub rewind_months: Memo<u32>,
    /// A refresh is in flight (old content stays visible meanwhile).
    pub refreshing: Signal<bool>,
    /// Index of the centred Cover Flow item.
    pub centre: Signal<usize>,
    /// Bumped when new stage data lands, so Cover Flow remounts centred.
    pub flow_key: Signal<u64>,
    pub active: Memo<Vec<ServerConfig>>,
    pub offline: Memo<bool>,
    /// Set once the user opens Rewind, so its shelves load lazily.
    pub rewind_wanted: Signal<bool>,
    generation: Signal<u64>,
}

impl DiscoverData {
    /// New picks for today: reseed and reload every section.
    pub fn refresh(&self) {
        if *self.refreshing.peek() || self.active.peek().is_empty() {
            return;
        }
        refresh_discover(&self.active.peek());
        let mut generation = self.generation;
        *generation.write() += 1;
    }
}

/// Creates the Discover data, provides it as context and drives the loads.
fn use_discover_data(servers: Signal<Vec<ServerConfig>>, settings: Signal<AppSettings>) -> DiscoverData {
    let active = use_memo(move || {
        servers
            .read()
            .iter()
            .filter(|s| s.active)
            .cloned()
            .collect::<Vec<_>>()
    });
    // Memos only notify on real changes, so unrelated settings writes (volume,
    // the saved-mix map, the Rewind window) don't reload the feed.
    let config = use_memo(move || DiscoveryConfig::from_settings(&settings.read()));
    let offline = use_memo(move || settings.read().offline_mode);
    let rewind_months = use_memo(move || {
        let months = settings.read().discover_rewind_months;
        if REWIND_MONTH_CHOICES.contains(&months) {
            months
        } else {
            DEFAULT_REWIND_MONTHS
        }
    });
    let mut stage = use_signal(|| None::<StageSection>);
    let mut mixes = use_signal(|| None::<MixesSection>);
    let mut off_server = use_signal(|| None::<OffServerSection>);
    let mut refreshing = use_signal(|| false);
    let mut centre = use_signal(|| 0usize);
    let mut flow_key = use_signal(|| 0u64);
    let generation = use_signal(|| 0u64);
    let rewind_wanted = use_signal(|| false);

    let rewind = use_resource(move || {
        let wanted = rewind_wanted();
        let months = rewind_months();
        let ready = stage.read().is_some();
        let servers = active();
        let settings = settings.peek().clone();
        async move {
            if !wanted || !ready || servers.is_empty() {
                return None;
            }
            Some(load_discover_rewind(&servers, &settings, months).await)
        }
    });

    let data = use_context_provider(|| DiscoverData {
        stage,
        mixes,
        off_server,
        rewind,
        rewind_months,
        refreshing,
        centre,
        flow_key,
        active,
        offline,
        rewind_wanted,
        generation,
    });

    let mut task = use_signal(|| None::<Task>);
    let mut seen_generation = use_signal(|| 0u64);
    use_effect(move || {
        let servers = active();
        let _ = config();
        let _ = offline();
        let current = generation();

        if let Some(previous) = *task.peek() {
            previous.cancel();
        }
        let is_refresh = current != *seen_generation.peek();
        seen_generation.set(current);
        if !is_refresh {
            stage.set(None);
            mixes.set(None);
            off_server.set(None);
        }
        if servers.is_empty() {
            refreshing.set(false);
            return;
        }
        refreshing.set(is_refresh);
        let settings = settings.peek().clone();
        let load = spawn(async move {
            let section = load_discover_stage(&servers, &settings).await;
            centre.set(stage::initial_centre(section.stage.len()));
            *flow_key.write() += 1;
            stage.set(Some(section));
            mixes.set(Some(load_discover_mixes(&servers, &settings).await));
            off_server.set(Some(load_discover_off_server(&servers, &settings).await));
            refreshing.set(false);
        });
        task.set(Some(load));
    });

    data
}

const SEGMENTS: [&str; 3] = ["For You", "New", "Rewind"];

#[component]
pub fn HomeView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let settings = use_context::<Signal<AppSettings>>();
    let main_scroll = try_use_context::<MainScroll>();
    let wide = !use_layout().is_compact();
    use_context_provider(Toast::new);
    use_preview_provider();
    let data = use_discover_data(servers, settings);
    let mut rewind_wanted = data.rewind_wanted;
    let mut segment = use_signal(|| 0usize);
    let mut pull = use_signal(|| 0.0f64);
    let mut pull_start = use_signal(|| None::<f64>);

    // No server, or every server switched off: the first-run welcome handles both.
    if servers.read().is_empty() || data.active.read().is_empty() {
        return rsx! { crate::components::views::FirstRun {} };
    }

    let at_top = move || main_scroll.map(|m| *m.top.peek() <= 0.0).unwrap_or(true);
    let end_pull = move |_| {
        if pull_start.peek().is_none() {
            return;
        }
        pull_start.set(None);
        if *pull.peek() >= PULL_TRIGGER_PX {
            data.refresh();
        }
        if *pull.peek() != 0.0 {
            pull.set(0.0);
        }
    };

    let selected = segment();
    let offline = (data.offline)();

    rsx! {
        div {
            class: "rs-view rs-view--flush rs-discover",
            class: if wide { "rs-discover--wide" },
            "data-testid": "discover",
            ontouchstart: move |evt| {
                if wide || *data.refreshing.peek() || !at_top() {
                    return;
                }
                pull_start.set(evt.touches().first().map(|t| t.client_coordinates().y));
            },
            ontouchmove: move |evt| {
                let Some(start) = *pull_start.peek() else {
                    return;
                };
                let Some(y) = evt.touches().first().map(|t| t.client_coordinates().y) else {
                    return;
                };
                // Damped, and quantised so the indicator re-renders sparingly.
                let distance = (((y - start) * 0.5).max(0.0) / 4.0).floor() * 4.0;
                if distance != *pull.peek() {
                    pull.set(distance.min(PULL_TRIGGER_PX * 1.5));
                }
            },
            ontouchend: end_pull,
            ontouchcancel: move |_| {
                pull_start.set(None);
                pull.set(0.0);
            },

            DiscoverStage { wide, pull }

            div { class: "rs-discover__page",
                div { class: "rs-discover__controls",
                    Segmented {
                        options: SEGMENTS.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                        selected,
                        inline: wide,
                        label: "Discover view".to_string(),
                        testid: "discover-segments".to_string(),
                        on_select: move |index: usize| {
                            if index == 2 && !*rewind_wanted.peek() {
                                rewind_wanted.set(true);
                            }
                            segment.set(index);
                        },
                    }
                    if wide {
                        p { class: "rs-discover__keys", aria_hidden: "true",
                            kbd { "←" }
                            kbd { "→" }
                            " to flip covers, "
                            kbd { "Return" }
                            " to open"
                        }
                    }
                }
                if offline {
                    p { class: "rs-discover__note", role: "status",
                        "You're offline. Discover shows what's cached; only downloaded songs play."
                    }
                }
                match (selected, wide) {
                    (0, true) => rsx! {
                        div { class: "rs-discover__cols",
                            div { class: "rs-discover__col",
                                SectionHeader { title: "Mixes for you".to_string() }
                                MixList { wide }
                            }
                            div { class: "rs-discover__col", OffServerList { wide } }
                        }
                    },
                    (0, false) => rsx! {
                        MixList { wide }
                        OffServerList { wide }
                    },
                    (1, true) => rsx! {
                        div { class: "rs-discover__cols",
                            div { class: "rs-discover__col", NewShelf {} }
                            div { class: "rs-discover__col", OffServerList { wide } }
                        }
                    },
                    (1, false) => rsx! {
                        NewShelf {}
                        OffServerList { wide }
                    },
                    _ => rsx! {
                        RewindPanel { wide }
                    },
                }
            }
            NoticeCapsule {}
        }
    }
}
