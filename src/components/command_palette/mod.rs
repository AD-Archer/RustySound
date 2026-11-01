//! The command palette (⌘K / Ctrl+K, or "/") and the app's keyboard
//! shortcuts.
//!
//! * `actions.rs`: the one action list, built from what the app can do now.
//! * `shortcuts.rs`: the one shortcut registry (page listener, hints, cheat
//!   sheet, macOS menu accelerators).
//! * `rows.rs` + `fuzzy.rs`: what the palette lists for a query.
//! * `library.rs`: the library search (Search's query path, cached).
//! * `run.rs`: carries actions out through the app's existing code paths.
//! * `palette.rs`, `cheat_sheet.rs`: the UI.
//!
//! The shell calls [`use_command_palette_provider`] and mounts
//! [`CommandPaletteHost`] once. On compact layouts there is no keyboard:
//! Search lists matching actions ([`SearchActions`]) and their "…" choices
//! open the palette.

mod actions;
mod cheat_sheet;
mod fuzzy;
mod library;
mod palette;
mod recent;
mod rows;
mod run;
mod shortcuts;

#[cfg(all(feature = "desktop", target_os = "macos"))]
pub(crate) use shortcuts::{menu_accelerator, menu_item};

use crate::api::{Capabilities, ServerConfig, Song};
use crate::components::song_details::actions::is_live_song;
use crate::components::ui::{radio_available, InsetList, ListInset, ListRow};
use crate::components::{IsPlayingSignal, ShuffleEnabledSignal, EQ_AVAILABLE};
use crate::db::{AppSettings, RepeatMode};
use actions::{Caps, SongCaps};
use dioxus::prelude::*;
use rows::{ChoicePage, Entry};
use run::Runner;

pub(crate) use actions::ActionId;
pub(crate) use shortcuts::event_runs;

const LISTENER_JS: &str = include_str!("listener.js");

/// One profile, as the palette's "Switch Profile…" lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteProfile {
    pub id: String,
    pub name: String,
    pub active: bool,
}

/// Profiles, when the app has them. The profiles feature provides this
/// context (above `CommandPaletteHost`) and the palette offers "Switch
/// Profile…"; without it the action stays out of the list.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct PaletteProfiles {
    pub list: Signal<Vec<PaletteProfile>>,
    pub switch: Callback<String>,
}

/// Open state of the palette, the shortcut cheat sheet and Wrapped (context).
#[derive(Clone, Copy, PartialEq)]
pub struct CommandPalette {
    open: Signal<bool>,
    /// The choices page the palette opens on (None: the root list).
    page: Signal<Option<ChoicePage>>,
    shortcuts: Signal<bool>,
    wrapped: Signal<bool>,
    /// ⌘ or Ctrl in hints.
    apple: Signal<bool>,
}

/// Provides [`CommandPalette`]; called once by the shell, above the views
/// (Search and Settings open it) and the host.
pub fn use_command_palette_provider() -> CommandPalette {
    let palette = CommandPalette {
        open: use_signal(|| false),
        page: use_signal(|| None),
        shortcuts: use_signal(|| false),
        wrapped: use_signal(|| false),
        apple: use_signal(|| cfg!(any(target_os = "macos", target_os = "ios"))),
    };
    use_context_provider(|| palette)
}

impl CommandPalette {
    fn set_open(&self, open: bool, page: Option<ChoicePage>) {
        let (mut open_signal, mut page_signal) = (self.open, self.page);
        if open && !*open_signal.peek() {
            // Remember where focus was, to give it back on close.
            let _ = document::eval("window.__rsPaletteReturn = document.activeElement;");
        }
        if *page_signal.peek() != page {
            page_signal.set(page);
        }
        if *open_signal.peek() != open {
            open_signal.set(open);
        }
    }

    pub fn open(&self) {
        self.set_open(true, None);
    }

    /// Opens straight on a list of choices ("Sleep Timer…").
    pub(crate) fn open_page(&self, page: ChoicePage) {
        self.set_open(true, Some(page));
    }

    pub fn close(&self) {
        self.set_open(false, None);
    }

    pub fn toggle(&self) {
        if *self.open.peek() {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn show_shortcuts(&self) {
        self.close();
        let mut shortcuts = self.shortcuts;
        shortcuts.set(true);
    }

    pub fn toggle_shortcuts(&self) {
        if *self.shortcuts.peek() {
            let mut shortcuts = self.shortcuts;
            shortcuts.set(false);
        } else {
            self.show_shortcuts();
        }
    }

    fn show_wrapped(&self) {
        let mut wrapped = self.wrapped;
        wrapped.set(true);
    }

    pub fn apple(&self) -> bool {
        (self.apple)()
    }
}

/// What the app can do right now, for the action list. Subscribes the
/// caller to the playing song, play state, shuffle, repeat and settings.
fn use_caps() -> Caps {
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<IsPlayingSignal>().0;
    let shuffle = use_context::<ShuffleEnabledSignal>().0;
    let repeat = use_context::<Signal<RepeatMode>>();
    let settings = use_context::<Signal<AppSettings>>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let profiles = try_use_context::<PaletteProfiles>();

    let offline = settings.read().offline_mode;
    let profiles = profiles.is_some_and(|p| !p.list.read().is_empty());
    let servers = servers.read();
    let song = now_playing.read().as_ref().map(|song| SongCaps {
        live: is_live_song(song),
        liked: song.starred.is_some(),
        album: song.album_id.is_some(),
        artist: song.artist_id.is_some() || song.artist.as_deref().is_some_and(|a| !a.trim().is_empty()),
        playlist_edits: Capabilities::for_source(&servers, &song.server_id).playlist_edits,
    });
    Caps {
        song,
        playing: is_playing(),
        shuffle: shuffle(),
        repeat: repeat(),
        offline,
        equalizer: EQ_AVAILABLE,
        radio: radio_available(&servers),
        profiles,
    }
}

/// The page-wide shortcut listener: runs the registry's global bindings.
fn use_shortcut_listener(runner: Runner) {
    use_hook(move || {
        let runner = runner.clone();
        spawn(async move {
            let script = LISTENER_JS.replace("__TABLE__", &shortcuts::listener_table());
            let mut listener = document::eval(&script);
            while let Ok((combo, typing)) = listener.recv::<(String, bool)>().await {
                if let Some(id) = shortcuts::resolve(&combo, typing) {
                    runner.run(id);
                }
            }
        });
    });
}

/// Mounts the palette, the shortcut cheat sheet and Wrapped when open, and
/// installs the shortcut listener and the macOS menu items. Once, in the
/// shell.
#[component]
pub fn CommandPaletteHost() -> Element {
    let palette = use_context::<CommandPalette>();
    let runner = use_hook(Runner::from_context);
    use_shortcut_listener(runner.clone());
    // Library answers belong to the servers that gave them.
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    use_effect(move || {
        servers.read();
        library::clear_cache();
    });

    // The web build learns the platform from the browser.
    #[cfg(target_arch = "wasm32")]
    use_hook(move || {
        let mut apple = palette.apple;
        spawn(async move {
            let probe = document::eval(
                "return /Mac|iPhone|iPad|iPod/i.test(navigator.platform || navigator.userAgent || '');",
            );
            if let Ok(value) = probe.join::<bool>().await {
                apple.set(value);
            }
        });
    });

    #[cfg(all(feature = "desktop", target_os = "macos"))]
    {
        let runner = runner.clone();
        dioxus::desktop::use_muda_event_handler(move |event| {
            if let Some(id) = menu_action(event.id().as_ref()) {
                runner.run(id);
            }
        });
    }

    rsx! {
        if (palette.open)() {
            palette::Palette { page: (palette.page)() }
        }
        if (palette.shortcuts)() {
            cheat_sheet::CheatSheet {
                on_close: move |_| {
                    let mut shortcuts = palette.shortcuts;
                    shortcuts.set(false);
                },
            }
        }
        if (palette.wrapped)() {
            crate::components::views::WrappedStory {
                on_close: move |_| {
                    let mut wrapped = palette.wrapped;
                    wrapped.set(false);
                },
            }
        }
    }
}

/// macOS menu item ids for palette actions: `rustysound-action-<key>`.
#[cfg(all(feature = "desktop", target_os = "macos"))]
fn menu_action(id: &str) -> Option<ActionId> {
    ActionId::from_key(id.strip_prefix("rustysound-action-")?)
}

/// Compact Search's "Actions": the palette's actions matching the query,
/// so the iPhone (no ⌘K) reaches them too. "…" actions open the palette on
/// their choices. Nothing shows for a query no action clearly matches.
#[component]
pub fn SearchActions(query: String) -> Element {
    let caps = use_caps();
    let palette = use_context::<CommandPalette>();
    let runner = use_hook(Runner::from_context);
    let list = actions::actions(&caps);
    let rows: Vec<rows::Row> = rows::root_rows(&query, &list, &[], None)
        .into_iter()
        .filter(|row| !row.hits.is_empty() && fuzzy::fuzzy(&query, &row.title).is_some_and(|m| m.score >= fuzzy::STRONG))
        .take(3)
        .collect();
    if rows.is_empty() {
        return rsx! {};
    }
    rsx! {
        InsetList { inset: ListInset::Icon, label: "Actions".to_string(), testid: "search-actions".to_string(),
            for row in rows {
                ListRow {
                    key: "{row.title}",
                    title: row.title.clone(),
                    icon: row.icon.to_string(),
                    compact: true,
                    onclick: {
                        let runner = runner.clone();
                        let entry = row.entry.clone();
                        move |_| {
                            if let Entry::Action(id) = entry {
                                match ChoicePage::for_action(id) {
                                    Some(page) => palette.open_page(page),
                                    None => runner.run(id),
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}
