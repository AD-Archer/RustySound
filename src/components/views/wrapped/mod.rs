//! Wrapped: a look back at a year of listening, as a story of cards. Opened
//! from Settings ("Your 2026 in Music"); "This year so far" by default, with
//! a picker for earlier years that have listening.
//!
//! The data comes from `api::listening` (worked out off the render path and
//! cached per year); the cards are in `cards.rs`, their words in `copy.rs`.

mod cards;
mod copy;

use crate::api::listening::{current_year, load_wrapped, wrapped_years};
use crate::api::ServerConfig;
use crate::components::ui::Story;
use crate::components::Icon;
use cards::{cards_for, render, CardKind, LoadingCard};
use dioxus::prelude::*;
use std::rc::Rc;

/// The full-screen Wrapped story. The parent decides whether it is mounted.
#[component]
pub(super) fn WrappedStory(on_close: EventHandler<()>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let this_year = use_hook(current_year);
    let mut year = use_signal(|| this_year);
    let mut years = use_signal(|| vec![this_year]);
    let mut years_asked = use_signal(|| false);
    let mut index = use_signal(|| 0usize);

    let data = use_resource(move || {
        let wanted = year();
        let list = servers.peek().clone();
        async move { Rc::new(load_wrapped(&list, wanted).await) }
    });
    let loaded = data.read().clone().filter(|y| y.year == year());

    // Once the first year is in, find the other years with listening (this
    // reads the rest of the history, so it waits its turn).
    use_effect(move || {
        if data.read().is_none() || *years_asked.peek() {
            return;
        }
        years_asked.set(true);
        spawn(async move {
            let list = servers.peek().clone();
            years.set(wrapped_years(&list).await);
        });
    });

    let kinds: Vec<CardKind> = loaded.as_deref().map(cards_for).unwrap_or_default();
    let count = kinds.len().max(1);
    let picked = year();

    rsx! {
        Story {
            count,
            index,
            label: "Wrapped".to_string(),
            testid: "wrapped".to_string(),
            on_close: move |_| on_close.call(()),
            header: rsx! {
                label { class: "rs-wrapped-year",
                    span { class: "rs-sr-only", "Year" }
                    select {
                        "data-testid": "wrapped-year",
                        value: "{picked}",
                        onchange: move |evt| {
                            if let Ok(next) = evt.value().parse::<i32>() {
                                index.set(0);
                                year.set(next);
                            }
                        },
                        for y in years() {
                            option { key: "{y}", value: "{y}", selected: y == picked,
                                if y == this_year { "This year so far" } else { "{y}" }
                            }
                        }
                    }
                    Icon { name: "chevron-down".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                }
            },
            card: move |i: usize| -> Element {
                match (loaded.clone(), kinds.get(i).copied()) {
                    (Some(year), Some(kind)) => render(kind, year, i == *index.peek()),
                    _ => rsx! { LoadingCard {} },
                }
            },
        }
    }
}

/// The Settings entry: calm, near the top. Hidden when there is nothing to
/// look back on (no active server and no music on the device).
#[component]
pub(super) fn WrappedEntry(on_open: EventHandler<()>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let has_source = servers.read().iter().any(|s| s.active) || crate::local_music::has_music();
    if !has_source {
        return rsx! {};
    }
    let year = current_year();
    rsx! {
        ul { class: "rs-list rs-list--icons rs-settings-wrapped", aria_label: "Wrapped",
            li {
                button {
                    r#type: "button",
                    class: "rs-row rs-settings-wrapped__row",
                    "data-testid": "settings-wrapped",
                    onclick: move |_| on_open.call(()),
                    span { class: "rs-row__lead rs-settings-wrapped__lead", aria_hidden: "true",
                        Icon { name: "headphones".to_string(), class: "rs-ic rs-ic--md".to_string() }
                    }
                    span { class: "rs-row__meta",
                        span { class: "rs-row__title",
                            span { "Your {year} in Music" }
                        }
                        span { class: "rs-row__sub", "Wrapped: a look back at your year so far" }
                    }
                    span { class: "rs-row__chev",
                        Icon { name: "chevron-right".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                    }
                }
            }
        }
    }
}
