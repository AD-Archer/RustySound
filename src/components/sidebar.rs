use crate::api::ServerConfig;
use crate::components::{AppView, Icon, Navigation, SongDetailsController};
use dioxus::prelude::*;

#[component]
pub fn Sidebar(sidebar_open: Signal<bool>, overlay_mode: bool) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let song_details = use_context::<SongDetailsController>();
    let view = use_route::<AppView>();

    let is_open = sidebar_open();

    let server_count = servers().len();
    let active_servers = servers().iter().filter(|s| s.active).count();

    let slide_class = if is_open {
        "translate-x-0"
    } else {
        "-translate-x-full"
    };
    let sidebar_class = if overlay_mode {
        format!(
            "fixed inset-y-0 left-0 z-[120] w-72 bg-zinc-950/70 border-r border-zinc-800/60 flex flex-col min-h-0 overflow-hidden backdrop-blur-xl transform transition-transform duration-300 ease-out shadow-2xl shadow-black/30 {slide_class}"
        )
    } else {
        format!(
            "fixed inset-y-0 left-0 z-40 w-72 2xl:w-64 bg-zinc-950/70 border-r border-zinc-800/60 flex flex-col min-h-0 overflow-hidden backdrop-blur-xl transform transition-transform duration-300 ease-out 2xl:translate-x-0 2xl:static 2xl:z-auto shadow-2xl shadow-black/30 2xl:shadow-none {slide_class}"
        )
    };
    let nav_to = |target: AppView| {
        let navigation = navigation.clone();
        let mut sidebar_open = sidebar_open.clone();
        let mut song_details = song_details.clone();
        move |_| {
            navigation.navigate_to(target.clone());
            song_details.close();
            sidebar_open.set(false);
        }
    };

    rsx! {
        aside { class: "sidebar-shell {sidebar_class}",
            // Logo
            div { class: "p-5 2xl:p-6 border-b border-zinc-800/60 flex items-center justify-between",
                div { class: "flex items-center gap-3",
                    div { class: "w-10 h-10 rounded-xl bg-zinc-800/50 flex items-center justify-center shadow-lg overflow-hidden",
                        img {
                            src: asset!("/assets/favicon.svg"),
                            alt: "RustySound Logo",
                            class: "w-8 h-8 object-contain",
                        }
                    }
                    div {
                        h1 { class: "text-lg font-bold text-white", "RustySound" }
                        p { class: "text-xs text-zinc-500", "{active_servers}/{server_count} servers" }
                    }
                }
                button {
                    class: "2xl:hidden p-2 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800/60 transition-colors",
                    aria_label: "Close menu",
                    onclick: {
                        let mut sidebar_open = sidebar_open.clone();
                        move |_| sidebar_open.set(false)
                    },
                    Icon { name: "x".to_string(), class: "w-4 h-4".to_string() }
                }
            }

            // Navigation
            nav { class: "flex-1 min-h-0 overflow-y-auto p-4 space-y-1 sidebar-scrollable",
                // Main section
                div { class: "mb-6",
                    p { class: "text-xs font-semibold text-zinc-500 uppercase tracking-wider mb-3 px-3",
                        "Discover"
                    }
                    NavItem {
                        icon: "home",
                        label: "Home",
                        active: matches!(view, AppView::HomeView {}),
                        onclick: nav_to(AppView::HomeView {}),
                    }
                    NavItem {
                        icon: "search",
                        label: "Search",
                        active: matches!(view, AppView::SearchView {}),
                        onclick: nav_to(AppView::SearchView {}),
                    }
                    NavItem {
                        icon: "shuffle",
                        label: "Random",
                        active: matches!(view, AppView::RandomView {}),
                        onclick: nav_to(AppView::RandomView {}),
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
                        active: matches!(view, AppView::Albums {}),
                        onclick: nav_to(AppView::Albums {}),
                    }
                    NavItem {
                        icon: "music",
                        label: "Songs",
                        active: matches!(view, AppView::SongsView {}),
                        onclick: nav_to(AppView::SongsView {}),
                    }
                    NavItem {
                        icon: "artist",
                        label: "Artists",
                        active: matches!(view, AppView::ArtistsView {}),
                        onclick: nav_to(AppView::ArtistsView {}),
                    }
                    NavItem {
                        icon: "playlist",
                        label: "Playlists",
                        active: matches!(view, AppView::PlaylistsView {}),
                        onclick: nav_to(AppView::PlaylistsView {}),
                    }
                    NavItem {
                        icon: "radio",
                        label: "Radio",
                        active: matches!(view, AppView::RadioView {}),
                        onclick: nav_to(AppView::RadioView {}),
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
                        active: matches!(view, AppView::FavoritesView {}),
                        onclick: nav_to(AppView::FavoritesView {}),
                    }
                    NavItem {
                        icon: "bookmark",
                        label: "Bookmarks",
                        active: matches!(view, AppView::BookmarksView {}),
                        onclick: nav_to(AppView::BookmarksView {}),
                    }
                    NavItem {
                        icon: "queue",
                        label: "Queue",
                        active: matches!(view, AppView::QueueView {}),
                        onclick: nav_to(AppView::QueueView {}),
                    }
                }

            }

            div { class: "p-4 pt-3 border-t border-zinc-800/50 bg-zinc-950/60",
                NavItem {
                    icon: "settings",
                    label: "Settings",
                    active: matches!(view, AppView::SettingsView {}),
                    onclick: nav_to(AppView::SettingsView {}),
                }
            }
        }
    }
}

#[component]
fn NavItem(
    icon: String,
    label: String,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
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
