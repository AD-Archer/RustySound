//! Wrapped: a look back at a year of listening, as a story of cards, from the
//! servers' play history. Opened from the Listening page ("Play as a story",
//! for the year shown there) and the command palette; "This year so far" by
//! default, with a picker for earlier years that have listening.
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
pub(crate) fn WrappedStory(on_close: EventHandler<()>, #[props(default)] start_year: Option<i32>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let this_year = use_hook(current_year);
    let first = start_year.unwrap_or(this_year);
    let mut year = use_signal(|| first);
    let mut years = use_signal(|| if first == this_year { vec![this_year] } else { vec![this_year, first] });
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
