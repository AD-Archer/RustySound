use dioxus::prelude::*;
use crate::api::ServerConfig;
use crate::components::{AppView, Icon};

#[component]
pub fn Sidebar() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut current_view = use_context::<Signal<AppView>>();
    let view = current_view();
    
    let server_count = servers().len();
    let active_servers = servers().iter().filter(|s| s.active).count();
    
    rsx! {
        aside { class: "w-64 bg-zinc-950/50 border-r border-zinc-800/50 flex flex-col h-full backdrop-blur-xl",
            // Logo
            div { class: "p-6 border-b border-zinc-800/50",
                div { class: "flex items-center gap-3",
                    div { class: "w-10 h-10 rounded-xl bg-gradient-to-br from-emerald-500 to-teal-600 flex items-center justify-center text-white font-bold text-lg shadow-lg shadow-emerald-500/20",
                        "R"
                    }
                    div {
                        h1 { class: "text-lg font-bold text-white", "RustySound" }
                        p { class: "text-xs text-zinc-500", "{active_servers}/{server_count} servers" }
                    }
                }
            }

            // Navigation
            nav { class: "flex-1 overflow-y-auto p-4 space-y-1",
                // Main section
                div { class: "mb-6",
                    p { class: "text-xs font-semibold text-zinc-500 uppercase tracking-wider mb-3 px-3",
                        "Discover"
                    }
                    NavItem {
                        icon: "home",
                        label: "Home",
                        active: matches!(view, AppView::Home),
                        onclick: move |_| current_view.set(AppView::Home),
                    }
                    NavItem {
                        icon: "search",
                        label: "Search",
                        active: matches!(view, AppView::Search),
                        onclick: move |_| current_view.set(AppView::Search),
                    }
                    NavItem {
                        icon: "shuffle",
                        label: "Random",
                        active: matches!(view, AppView::Random),
                        onclick: move |_| current_view.set(AppView::Random),
                    }
                }

                // Library section
                div { class: "mb-6",
                    p { class: "text-xs font-semibold text-zinc-500 uppercase tracking-wider mb-3 px-3",
                        "Library"
                    }
                    NavItem {
                        icon: "album",
                        label: "Albums",
                        active: matches!(view, AppView::Albums),
                        onclick: move |_| current_view.set(AppView::Albums),
                    }
                    NavItem {
                        icon: "artist",
                        label: "Artists",
                        active: matches!(view, AppView::Artists),
                        onclick: move |_| current_view.set(AppView::Artists),
                    }
                    NavItem {
                        icon: "playlist",
                        label: "Playlists",
                        active: matches!(view, AppView::Playlists),
                        onclick: move |_| current_view.set(AppView::Playlists),
                    }
                    NavItem {
                        icon: "radio",
                        label: "Radio",
                        active: matches!(view, AppView::Radio),
                        onclick: move |_| current_view.set(AppView::Radio),
                    }
                }

                // Personal section
                div { class: "mb-6",
                    p { class: "text-xs font-semibold text-zinc-500 uppercase tracking-wider mb-3 px-3",
                        "Personal"
                    }
                    NavItem {
                        icon: "heart",
                        label: "Favorites",
                        active: matches!(view, AppView::Favorites),
                        onclick: move |_| current_view.set(AppView::Favorites),
                    }
                    NavItem {
                        icon: "queue",
                        label: "Queue",
                        active: matches!(view, AppView::Queue),
                        onclick: move |_| current_view.set(AppView::Queue),
                    }
                }
            }

            // Settings at bottom
            div { class: "p-4 border-t border-zinc-800/50",
                NavItem {
                    icon: "settings",
                    label: "Settings",
                    active: matches!(view, AppView::Settings),
                    onclick: move |_| current_view.set(AppView::Settings),
                }
            }

            // Settings
            div { class: "p-4 border-t border-zinc-800/50",
                NavItem {
                    icon: "settings",
                    label: "Settings",
                    active: matches!(view, AppView::Settings),
                    onclick: move |_| current_view.set(AppView::Settings),
                }
            }
        }
    }
}

#[component]
fn NavItem(icon: String, label: String, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let base_class = "flex items-center gap-3 px-3 py-2.5 rounded-xl text-sm font-medium transition-all duration-200 cursor-pointer";
    let active_class = if active {
        "bg-gradient-to-r from-emerald-500/20 to-teal-500/10 text-emerald-400 shadow-sm"
    } else {
        "text-zinc-400 hover:text-white hover:bg-zinc-800/50"
    };
    
    rsx! {
        button {
            class: "{base_class} {active_class} w-full",
            onclick: move |e| onclick.call(e),
            Icon { name: icon.clone(), class: "w-5 h-5".to_string() }
            span { "{label}" }
        }
    }
}
