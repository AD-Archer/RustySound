//! Apple-style inset grouped list.

use dioxus::prelude::*;

/// Where the hairline separators start.
#[derive(Clone, Copy, PartialEq, Default)]
pub enum ListInset {
    /// After a 44px thumbnail (68px). The default.
    #[default]
    Art,
    /// After a small leading icon (52px).
    Icon,
    /// After a track number (46px).
    Track,
    /// Text-only rows (16px).
    Text,
}

/// A rounded, bordered `ul` of rows. Children should be `li` elements
/// (`ListRow` and `TrackRow` render their own `li`).
/// * `inset`: separator start, see `ListInset`.
/// * `label`: aria-label for the list. `testid`: optional data-testid.
#[component]
pub fn InsetList(
    #[props(default)] inset: ListInset,
    #[props(default)] label: Option<String>,
    #[props(default)] testid: Option<String>,
    #[props(default)] class: String,
    children: Element,
) -> Element {
    let modifier = match inset {
        ListInset::Art => "",
        ListInset::Icon => "rs-list--icons",
        ListInset::Track => "rs-list--tracks",
        ListInset::Text => "rs-list--text",
    };
    rsx! {
        ul {
            class: "rs-list {modifier} {class}",
            aria_label: label.clone(),
            "data-testid": testid.clone(),
            {children}
        }
    }
}
