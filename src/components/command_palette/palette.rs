//! The palette itself: a centred card over a scrim, a combobox input and a
//! grouped listbox. Focus stays in the input; ↑/↓ move the active row
//! (`aria-activedescendant`), ↩ runs it, ⌘↩ runs its alternate (Play Next),
//! Tab jumps between sections, Esc backs out of a choices page, then closes.
//! Focus goes back where it was on close.
//!
//! Typing searches the library after a short pause; an answer for an older
//! query is dropped (and its task cancelled), a query asked before answers
//! from the cache at once, and until the new answer lands the old one is
//! narrowed to rows that still match.

use super::actions::{actions, ActionId};
use super::fuzzy::{fuzzy, segments};
use super::library::{self, LibraryHits};
use super::recent::{self, Recent};
use super::rows::{choice_rows, next_section, root_rows, ChoicePage, ChoiceState, Entry, Row, Section};
use super::run::Runner;
use super::shortcuts::{label, primary_combo};
use super::{use_caps, CommandPalette, PaletteProfiles};
use crate::api::ServerConfig;
use crate::components::ui::env::{appearance_attr, sleep_ms, stage_theme};
use crate::components::ui::Artwork;
use crate::components::{Icon, SleepTimer, SLEEP_CHOICES};
use crate::db::AppSettings;
use dioxus::core::Task;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Pause after the last keystroke before the library is asked.
const DEBOUNCE_MS: u64 = 160;

thread_local! {
    /// The recents, read from disk once per session.
    static RECENTS: RefCell<Option<Vec<Recent>>> = const { RefCell::new(None) };
}

fn load_recents() -> Vec<Recent> {
    RECENTS.with(|cell| cell.borrow_mut().get_or_insert_with(recent::load).clone())
}

fn remember(item: Recent) {
    let next = recent::remember(&load_recents(), item);
    recent::save(&next);
    RECENTS.with(|cell| *cell.borrow_mut() = Some(next));
}

/// `hits` cut down to the rows that still match `query`, while the answer
/// for it is on its way.
fn narrow(hits: &LibraryHits, query: &str) -> LibraryHits {
    let key = query.trim().to_lowercase();
    if hits.query == key {
        return hits.clone();
    }
    let keeps = |text: &str| fuzzy(query, text).is_some();
    LibraryHits {
        query: hits.query.clone(),
        albums: hits.albums.iter().filter(|a| keeps(&a.name) || keeps(&format!("{} {}", a.name, a.artist))).cloned().collect(),
        artists: hits.artists.iter().filter(|a| keeps(&a.name)).cloned().collect(),
        songs: hits
            .songs
            .iter()
            .filter(|s| keeps(&format!("{} {} {}", s.title, s.artist.clone().unwrap_or_default(), s.album.clone().unwrap_or_default())))
            .cloned()
            .collect(),
        playlists: hits.playlists.iter().filter(|p| keeps(&p.name)).cloned().collect(),
    }
}

fn option_id(index: usize) -> String {
    format!("rs-palette-opt-{index}")
}

/// `data-entry` for tests and styling: `action:next`, `album:<id>`…
fn entry_attr(entry: &Entry) -> String {
    match entry {
        Entry::Action(id) => format!("action:{}", id.key()),
        Entry::Choice(choice) => format!("choice:{choice:?}"),
        Entry::Album(album) => format!("album:{}", album.id),
        Entry::Artist(artist) => format!("artist:{}", artist.id),
        Entry::Song(song) => format!("song:{}", song.id),
        Entry::Playlist(playlist) => format!("playlist:{}", playlist.id),
    }
}

#[component]
pub(super) fn Palette(page: Option<ChoicePage>) -> Element {
    let palette = use_context::<CommandPalette>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let settings = use_context::<Signal<AppSettings>>();
    let timer = use_context::<SleepTimer>();
    let profiles = try_use_context::<PaletteProfiles>();
    let runner = use_hook(Runner::from_context);
    let caps = use_caps();

    let mut query = use_signal(String::new);
    let mut current_page = use_signal(|| page);
    let mut active = use_signal(|| 0usize);
    let mut library_hits = use_signal(|| None::<Rc<LibraryHits>>);
    let mut searching = use_signal(|| false);
    let mut generation = use_signal(|| 0u64);
    let mut task = use_signal(|| None::<Task>);
    let recents = use_hook(load_recents);

    // The page listener stands down while the palette is open; focus goes
    // back where it was when it closes, however it closes.
    use_hook(|| {
        let _ = document::eval("window.__rsPaletteOpen = true;");
    });
    use_drop(|| {
        let _ = document::eval(
            r#"(() => {
  window.__rsPaletteOpen = false;
  const el = window.__rsPaletteReturn;
  window.__rsPaletteReturn = null;
  if (el && el.isConnected && typeof el.focus === "function") el.focus({ preventScroll: true });
})();"#,
        );
    });

    // A page asked for while open ("Sleep Timer…" from a shortcut).
    use_effect(use_reactive((&page,), move |(page,)| {
        if *current_page.peek() != page && page.is_some() {
            current_page.set(page);
            query.set(String::new());
        }
    }));

    // Search the library as the query settles.
    use_effect(move || {
        let text = query().trim().to_string();
        let on_page = current_page().is_some();
        active.set(0);
        generation.with_mut(|g| *g = g.wrapping_add(1));
        let ticket = *generation.peek();
        if let Some(old) = task.write().take() {
            old.cancel();
        }
        if on_page || text.chars().count() < 2 {
            library_hits.set(None);
            searching.set(false);
            return;
        }
        if let Some(hit) = library::cached(&text) {
            library_hits.set(Some(hit));
            searching.set(false);
            return;
        }
        searching.set(true);
        let list = servers.peek().clone();
        let handle = spawn(async move {
            sleep_ms(DEBOUNCE_MS).await;
            if *generation.peek() != ticket {
                return;
            }
            let hits = library::search(list, text).await;
            if *generation.peek() == ticket {
                library_hits.set(Some(hits));
                searching.set(false);
            }
        });
        task.set(Some(handle));
    });

    let text = query();
    let apple = palette.apple();
    let rows: Vec<Row> = match current_page() {
        Some(choices) => {
            let current = settings.read();
            let mode = (timer.mode)();
            let minutes = (timer.minutes)();
            let state = ChoiceState {
                sleep: SLEEP_CHOICES.iter().copied().find(|c| mode.matches(*c, minutes)),
                speed: current.playback_rate,
                eq_enabled: current.equalizer.enabled,
                eq_preset: current.equalizer.preset,
                theme: stage_theme(&current.theme),
                appearance: appearance_attr(&current),
                profiles: profiles
                    .map(|p| p.list.read().iter().map(|p| (p.id.clone(), p.name.clone(), p.active)).collect())
                    .unwrap_or_default(),
            };
            choice_rows(choices, &text, &state)
        }
        None => {
            let narrowed = library_hits().map(|hits| narrow(&hits, &text));
            root_rows(&text, &actions(&caps), &recents, narrowed.as_ref())
        }
    };
    let count = rows.len();
    let selected = active().min(count.saturating_sub(1));
    let busy = searching();

    // Keep the active row in view.
    use_effect(move || {
        let index = active();
        let _ = document::eval(&format!(
            "document.getElementById('{}')?.scrollIntoView({{ block: 'nearest' }});",
            option_id(index)
        ));
    });

    let run_row = {
        let rows = rows.clone();
        let runner = runner.clone();
        move |index: usize, alternate: bool| {
            let Some(row) = rows.get(index).cloned() else {
                return;
            };
            if let (Entry::Action(id), true) = (&row.entry, row.opens) {
                remember(Recent::Action { key: id.key().to_string() });
                current_page.set(ChoicePage::for_action(*id));
                query.set(String::new());
                return;
            }
            palette.close();
            // On the shell's scope, after the palette has gone: what it
            // starts outlives the palette, and Now Playing or a new page
            // takes focus after it went back.
            let runner = runner.clone();
            runner.clone().later(move || {
                if let Some(item) = runner.run_entry(row.entry, alternate) {
                    remember(item);
                }
            });
        }
    };

    let on_key = {
        let rows = rows.clone();
        let mut run_row = run_row.clone();
        move |evt: KeyboardEvent| {
            let mods = evt.modifiers();
            let primary = mods.contains(Modifiers::META) || mods.contains(Modifiers::CONTROL);
            match evt.key() {
                Key::ArrowDown if count > 0 => {
                    evt.prevent_default();
                    active.set((selected + 1) % count);
                }
                Key::ArrowUp if count > 0 => {
                    evt.prevent_default();
                    active.set((selected + count - 1) % count);
                }
                Key::Enter => {
                    evt.prevent_default();
                    run_row(selected, primary);
                }
                Key::Tab if count > 0 => {
                    evt.prevent_default();
                    active.set(next_section(&rows, selected, mods.contains(Modifiers::SHIFT)));
                }
                Key::Escape => {
                    evt.prevent_default();
                    evt.stop_propagation();
                    if current_page.peek().is_some() {
                        current_page.set(None);
                        query.set(String::new());
                    } else {
                        palette.close();
                    }
                }
                Key::Backspace if query.peek().is_empty() && current_page.peek().is_some() => {
                    evt.prevent_default();
                    current_page.set(None);
                }
                // ⌘K again closes it.
                Key::Character(ref c) if primary && c.eq_ignore_ascii_case("k") => {
                    evt.prevent_default();
                    palette.close();
                }
                _ => {}
            }
        }
    };

    // Rows grouped by section, keeping their flat index.
    let mut groups: Vec<(Section, Vec<(usize, Row)>)> = Vec::new();
    for (index, row) in rows.iter().cloned().enumerate() {
        match groups.last_mut() {
            Some((section, items)) if *section == row.section => items.push((index, row)),
            _ => groups.push((row.section, vec![(index, row)])),
        }
    }
    let active_entry = rows.get(selected).map(|r| r.entry.clone());
    let placeholder = match current_page() {
        Some(_) => "Filter choices".to_string(),
        None => "Search actions, songs, albums…".to_string(),
    };
    let status = if busy && count == 0 {
        "Searching…".to_string()
    } else if count == 0 {
        "No matches".to_string()
    } else if count == 1 {
        "1 result".to_string()
    } else {
        format!("{count} results")
    };
    let hint = |text: &str| label(text, apple);

    rsx! {
        div {
            class: "rs-scrim rs-palette-scrim",
            aria_hidden: "true",
            onclick: move |_| palette.close(),
        }
        div {
            class: "rs-palette",
            role: "dialog",
            aria_modal: "true",
            aria_label: "Command palette",
            "data-testid": "command-palette",
            div { class: "rs-palette__field",
                if let Some(choices) = current_page() {
                    button {
                        r#type: "button",
                        class: "rs-palette__crumb",
                        tabindex: "-1",
                        aria_label: "Back to all actions",
                        onclick: move |_| {
                            current_page.set(None);
                            query.set(String::new());
                        },
                        Icon { name: "chevron-left".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                        span { "{choices.title()}" }
                    }
                } else {
                    Icon { name: "search".to_string(), class: "rs-ic rs-ic--sm rs-palette__glass".to_string() }
                }
                input {
                    class: "rs-palette__input",
                    r#type: "text",
                    role: "combobox",
                    aria_expanded: "true",
                    aria_controls: "rs-palette-list",
                    aria_autocomplete: "list",
                    aria_activedescendant: if count > 0 { Some(option_id(selected)) } else { None },
                    aria_label: "Search actions and your library",
                    "data-testid": "palette-input",
                    autocomplete: "off",
                    autocapitalize: "off",
                    spellcheck: "false",
                    enterkeyhint: "go",
                    placeholder: "{placeholder}",
                    value: "{text}",
                    onmounted: move |evt| async move {
                        let _ = evt.set_focus(true).await;
                    },
                    oninput: move |evt| query.set(evt.value()),
                    onkeydown: on_key,
                }
                if busy {
                    span { class: "rs-spinner rs-palette__spin", aria_hidden: "true" }
                }
            }
            div {
                id: "rs-palette-list",
                class: "rs-palette__list",
                role: "listbox",
                aria_label: "Results",
                aria_busy: if busy { "true" } else { "false" },
                for (section, items) in groups {
                    div {
                        key: "{section.label()}",
                        class: "rs-palette__group",
                        role: "group",
                        aria_label: "{section.label()}",
                        div { class: "rs-palette__heading", aria_hidden: "true", "{section.label()}" }
                        for (index, row) in items {
                            PaletteRow {
                                key: "{entry_attr(&row.entry)}",
                                index,
                                row: row.clone(),
                                active: index == selected,
                                apple,
                                on_hover: move |index: usize| {
                                    if *active.peek() != index {
                                        active.set(index);
                                    }
                                },
                                on_pick: {
                                    let mut run_row = run_row.clone();
                                    move |(index, alternate): (usize, bool)| run_row(index, alternate)
                                },
                            }
                        }
                    }
                }
            }
            if count == 0 {
                p { class: "rs-palette__empty", "{status}" }
            }
            div { class: "rs-palette__sr", role: "status", aria_live: "polite", "{status}" }
            div { class: "rs-palette__foot", "data-testid": "palette-hints", aria_hidden: "true",
                if let Some(entry) = active_entry.clone() {
                    span { kbd { {hint("enter")} } " {entry.primary()}" }
                    if let Some(alternate) = entry.alternate() {
                        span { kbd { {hint("mod+enter")} } " {alternate}" }
                    }
                }
                span { kbd { "Tab" } " Sections" }
                span { kbd { "Esc" } if current_page().is_some() { " Back" } else { " Close" } }
            }
        }
    }
}

#[component]
fn PaletteRow(
    index: usize,
    row: Row,
    active: bool,
    apple: bool,
    on_hover: EventHandler<usize>,
    on_pick: EventHandler<(usize, bool)>,
) -> Element {
    let shortcut = match row.entry {
        Entry::Action(id) if id != ActionId::Palette => primary_combo(id).map(|c| label(c, apple)),
        _ => None,
    };
    let lead = match &row.entry {
        Entry::Album(album) => Some((album.cover_art.clone(), album.server_id.clone(), false)),
        Entry::Song(song) => Some((song.cover_art.clone(), song.server_id.clone(), false)),
        Entry::Artist(artist) => Some((artist.cover_art.clone(), artist.server_id.clone(), true)),
        Entry::Playlist(playlist) => Some((playlist.cover_art.clone(), playlist.server_id.clone(), false)),
        _ => None,
    };
    rsx! {
        div {
            id: option_id(index),
            class: "rs-palette__row",
            class: if active { "rs-palette__row--active" },
            role: "option",
            aria_selected: if active { "true" } else { "false" },
            aria_checked: if row.checked { Some("true") } else { None },
            "data-testid": "palette-option",
            "data-entry": entry_attr(&row.entry),
            onmousemove: move |_| on_hover.call(index),
            onclick: move |evt| {
                let mods = evt.modifiers();
                on_pick.call((index, mods.contains(Modifiers::META) || mods.contains(Modifiers::CONTROL)));
            },
            span { class: "rs-palette__lead", aria_hidden: "true",
                if let Some((cover_art, server_id, round)) = lead {
                    Artwork { cover_art, server_id, size_px: 28, round, placeholder: row.icon.to_string() }
                } else {
                    Icon { name: row.icon.to_string(), class: "rs-ic rs-ic--sm".to_string() }
                }
            }
            span { class: "rs-palette__text",
                span { class: "rs-palette__title",
                    for (on, run) in segments(&row.title, &row.hits) {
                        if on {
                            mark { "{run}" }
                        } else {
                            "{run}"
                        }
                    }
                }
                if let Some(sub) = row.subtitle.clone() {
                    span { class: "rs-palette__sub", "{sub}" }
                }
            }
            if row.checked {
                Icon { name: "check".to_string(), class: "rs-ic rs-ic--sm rs-palette__check".to_string() }
            }
            if let Some(keys) = shortcut {
                kbd { class: "rs-palette__kbd", aria_hidden: "true", "{keys}" }
            }
            if row.opens {
                Icon { name: "chevron-right".to_string(), class: "rs-ic rs-ic--sm rs-palette__chev".to_string() }
            }
        }
    }
}
