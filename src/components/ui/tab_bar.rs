//! Bottom tab bar for the compact layout.

use crate::components::{AppView, Icon, Navigation, SongDetailsController};
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Discover,
    Library,
    Search,
}

fn tab_for(view: &AppView) -> Tab {
    match view {
        AppView::HomeView {} => Tab::Discover,
        AppView::SearchView {} => Tab::Search,
        _ => Tab::Library,
    }
}

/// Discover / Library / Search. Reads the route itself; no props.
/// Testids: `tab-discover`, `tab-library`, `tab-search`.
#[component]
pub fn TabBar() -> Element {
    let view = use_route::<AppView>();
    let navigation = use_context::<Navigation>();
    let song_details = use_context::<SongDetailsController>();
    let active = tab_for(&view);

    let tabs = [
        (Tab::Discover, "Discover", "discover", "tab-discover", AppView::HomeView {}),
        (Tab::Library, "Library", "library", "tab-library", AppView::LibraryView {}),
        (Tab::Search, "Search", "search", "tab-search", AppView::SearchView {}),
    ];

    rsx! {
        nav { class: "rs-tabbar", aria_label: "Tabs",
            for (tab, label, icon, testid, target) in tabs {
                button {
                    key: "{testid}",
                    r#type: "button",
                    class: "rs-tab",
                    "data-testid": testid,
                    aria_current: if tab == active { Some("page") } else { None },
                    onclick: {
                        let mut song_details = song_details.clone();
                        move |_| {
                            song_details.close();
                            navigation.navigate_to(target.clone());
                        }
                    },
                    Icon { name: icon.to_string(), class: "rs-tab__icon".to_string() }
                    "{label}"
                }
            }
        }
    }
}
