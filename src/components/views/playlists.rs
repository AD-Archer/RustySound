use crate::api::*;
use crate::components::audio_manager::apply_collection_shuffle_mode;
use crate::components::{AddIntent, AddMenuController, AppView, Icon, Navigation};
use dioxus::prelude::*;

const PLAYLIST_INITIAL_LIMIT: usize = 20;

fn anchored_menu_style(
    anchor_x: f64,
    anchor_y: f64,
    menu_width: f64,
    menu_max_height: f64,
) -> String {
    let preferred_top = (anchor_y + 8.0).max(8.0);
    let preferred_left = (anchor_x - menu_width).max(4.0);
    format!(
        "top: clamp(8px, {:.1}px, calc(100vh - {:.1}px - 8px)); left: clamp(4px, {:.1}px, calc(100vw - {:.1}px - 4px)); max-height: min({:.1}px, calc(100vh - 16px)); overflow-y: auto;",
        preferred_top, menu_max_height, preferred_left, menu_width, menu_max_height
    )
}

#[component]
pub fn PlaylistsView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut search_query = use_signal(String::new);
    let limit = use_signal(|| PLAYLIST_INITIAL_LIMIT);
    let mut refresh = use_signal(|| 0usize);
    let single_active_server = servers().iter().filter(|s| s.active).count() == 1;
    let mut hide_auto_imported = use_signal(|| true);
    let mut owner_filter = use_signal(|| "all".to_string());
    let mut sort_by = use_signal(|| "newest".to_string());

    let playlists = use_resource(move || {
        let servers = servers();
        let _refresh = refresh(); // dependency to force reload
        async move {
            let mut playlists = Vec::new();
            for server in servers.into_iter().filter(|s| s.active) {
                let client = NavidromeClient::new(server);
                if let Ok(server_playlists) = client.get_playlists().await {
                    playlists.extend(server_playlists);
                }
            }
            playlists
        }
    });

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header page-header--split",
                div {
                    h1 { class: "page-title", "Playlists" }
                    p { class: "page-subtitle", "Your playlists from all servers" }
                    if !single_active_server {
                        p { class: "text-sm text-amber-200/80 bg-amber-500/10 border border-amber-500/40 rounded-lg px-3 py-2 mt-2",
                            "Playlist creation and merging require exactly one active server."
                        }
                    }
                }
                div { class: "relative w-full md:max-w-xs",
                    Icon {
                        name: "search".to_string(),
                        class: "absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500".to_string(),
                    }
                    input {
                        class: "w-full pl-10 pr-4 py-2.5 bg-zinc-800/50 border border-zinc-700/50 rounded-xl text-sm text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                        placeholder: "Search playlists",
                        value: search_query,
                        oninput: move |e| {
                            let value = e.value();
                            if value.is_empty() || value.len() >= 2 {
                                search_query.set(value);
                            }
                        },
                    }
                }
                button {
                    class: "px-4 py-2 rounded-xl bg-zinc-800/60 hover:bg-zinc-800 text-zinc-200 text-sm font-medium transition-colors",
                    onclick: move |_| refresh.set(refresh() + 1),
                    "Refresh"
                }
            }

            div { class: "overflow-x-auto",
                div { class: "flex gap-2 min-w-min sm:grid sm:grid-cols-2 lg:grid-cols-4 sm:gap-3",
                    label { class: "flex items-center gap-1.5 px-3 py-2 rounded-lg bg-zinc-800/40 border border-zinc-700/50 cursor-pointer hover:bg-zinc-800/60 transition-colors whitespace-nowrap flex-shrink-0 sm:flex-shrink",
                        input {
                            r#type: "checkbox",
                            checked: hide_auto_imported(),
                            onchange: move |e| hide_auto_imported.set(e.checked()),
                            class: "w-3.5 h-3.5 rounded cursor-pointer",
                        }
                        span { class: "text-xs sm:text-sm font-medium text-zinc-200",
                            "Hide auto-imported"
                        }
                    }
                    select {
                        class: "px-3 py-2 rounded-lg bg-zinc-800/40 border border-zinc-700/50 text-xs sm:text-sm text-zinc-200 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20 transition-colors whitespace-nowrap flex-shrink-0 sm:flex-shrink",
                        value: owner_filter(),
                        onchange: move |e| owner_filter.set(e.value()),
                        option { value: "all", "All owners" }
                    }
                    select {
                        class: "px-3 py-2 rounded-lg bg-zinc-800/40 border border-zinc-700/50 text-xs sm:text-sm text-zinc-200 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20 transition-colors whitespace-nowrap flex-shrink-0 sm:flex-shrink",
                        value: sort_by(),
                        onchange: move |e| sort_by.set(e.value()),
                        option { value: "newest", "Newest" }
                        option { value: "name", "Name (A-Z)" }
                        option { value: "created", "Date created" }
                        option { value: "changed", "Last changed" }
                    }
                }
            }

            {
                match playlists() {
                    Some(playlists) => {
                        let raw_query = search_query().trim().to_string();
                        let query = raw_query.to_lowercase();
                        let mut filtered = playlists.clone();

                        if hide_auto_imported() {
                            filtered
                                .retain(|p| {
                                    p.comment
                                        .as_ref()
                                        .map(|c| !c.to_lowercase().contains("auto-imported"))
                                        .unwrap_or(true)
                                });
                        }
                        if owner_filter() != "all" {
                            filtered
                                .retain(|p| {
                                    p.owner
                                        .as_ref()
                                        .map(|o| o == &owner_filter())
                                        .unwrap_or(false)
                                });
                        }
                        if !query.is_empty() {
                            filtered.retain(|p| p.name.to_lowercase().contains(&query));
                        }
                        match sort_by().as_str() {
                            "name" => filtered.sort_by(|a, b| a.name.cmp(&b.name)),
                            "created" => {
                                filtered
                                    .sort_by(|a, b| {
                                        let a_date = a.created.as_deref().unwrap_or("");
                                        let b_date = b.created.as_deref().unwrap_or("");
                                        b_date.cmp(a_date)
                                    })
                            }
                            "changed" => {
                                filtered
                                    .sort_by(|a, b| {
                                        let a_date = a.changed.as_deref().unwrap_or("");
                                        let b_date = b.changed.as_deref().unwrap_or("");
                                        b_date.cmp(a_date)
                                    })
                            }
                            _ => filtered.sort_by(|a, b| b.id.cmp(&a.id)),
                        }
                        let has_query = !query.is_empty();
                        let more_available = filtered.len() > limit();
                        let display: Vec<Playlist> = filtered
                            .into_iter()
                            .take(limit())
                            .collect();
                        rsx! {
                            if display.is_empty() {
                                div { class: "flex flex-col items-center justify-center py-20",
                                    Icon {
                                        name: "playlist".to_string(),
                                        class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                                    }
                                    if has_query {
                                        p { class: "text-zinc-300", "No playlists match \"{raw_query}\"" }
                                    } else {
                                        h2 { class: "text-xl font-semibold text-white mb-2", "No playlists found" }
                                        p { class: "text-zinc-400", "Try adjusting your filters" }
                                    }
                                }
                            } else {
                                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4",
                                    for playlist in display {
                                        PlaylistCard {
                                            playlist: playlist.clone(),
                                            onclick: {
                                                let navigation = navigation.clone();
                                                let playlist_id = playlist.id.clone();
                                                let playlist_server_id = playlist.server_id.clone();
                                                move |_| {
                                                    navigation
                                                        .navigate_to(AppView::PlaylistDetailView {
                                                            playlist_id: playlist_id.clone(),
                                                            server_id: playlist_server_id.clone(),
                                                        })
                                                }
                                            },
                                            on_delete: move |_| refresh.set(refresh() + 1),
                                        }
                                    }
                                }
                                if more_available {
                                    div { class: "flex justify-center mt-4",
                                        button {
                                            class: "px-4 py-2 rounded-xl bg-zinc-800/60 hover:bg-zinc-800 text-zinc-200 text-sm font-medium transition-colors",
                                            onclick: {
                                                let mut limit = limit.clone();
                                                move |_| limit.set(limit().saturating_add(PLAYLIST_INITIAL_LIMIT))
                                            },
                                            "View more"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    None => rsx! {
                        div { class: "flex items-center justify-center py-20",
                            Icon {
                                name: "loader".to_string(),
                                class: "w-8 h-8 text-zinc-500".to_string(),
                            }
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn PlaylistCard(
    playlist: Playlist,
    onclick: EventHandler<MouseEvent>,
    on_delete: EventHandler<()>,
) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let add_menu = use_context::<AddMenuController>();
    let shuffle_enabled = use_context::<crate::components::ShuffleEnabledSignal>().0;
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let now_playing = use_context::<Signal<Option<Song>>>();

    let mut show_menu = use_signal(|| false);
    let mut menu_x = use_signal(|| 0f64);
    let mut menu_y = use_signal(|| 0f64);
    let mut show_delete_confirm = use_signal(|| false);
    let mut delete_error = use_signal(|| None::<String>);
    let mut deleting = use_signal(|| false);

    let is_auto_imported = playlist
        .comment
        .as_ref()
        .map(|c| c.to_lowercase().contains("auto-imported"))
        .unwrap_or(false);
    let editing_allowed = !is_auto_imported;

    let cover_url = servers()
        .iter()
        .find(|s| s.id == playlist.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            playlist
                .cover_art
                .as_ref()
                .map(|ca| client.get_cover_art_url(ca, 300))
        });

    let on_shuffle = {
        let mut shuffle_enabled = shuffle_enabled.clone();
        let queue = queue.clone();
        let queue_index = queue_index.clone();
        let now_playing = now_playing.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_menu.set(false);
            let next = !shuffle_enabled();
            shuffle_enabled.set(next);
            let _ = apply_collection_shuffle_mode(
                queue.clone(),
                queue_index.clone(),
                now_playing.clone(),
                next,
            );
        }
    };

    let on_confirm_delete = {
        let servers = servers.clone();
        let playlist_id = playlist.id.clone();
        let playlist_server_id = playlist.server_id.clone();
        move |_: MouseEvent| {
            let servers_snapshot = servers();
            if let Some(server) = servers_snapshot
                .into_iter()
                .find(|s| s.id == playlist_server_id && s.active)
            {
                let client = NavidromeClient::new(server);
                let playlist_id = playlist_id.clone();
                deleting.set(true);
                spawn(async move {
                    match client.delete_playlist(&playlist_id).await {
                        Ok(_) => {
                            show_delete_confirm.set(false);
                            on_delete.call(());
                        }
                        Err(err) => {
                            delete_error.set(Some(err));
                            deleting.set(false);
                        }
                    }
                });
            }
        }
    };

    rsx! {
        div { class: "relative",
            button {
                class: "group w-full text-left",
                onclick: move |e| onclick.call(e),
                // Playlist cover
                div { class: "aspect-square rounded-xl bg-zinc-800 mb-3 overflow-hidden relative shadow-lg group-hover:shadow-xl transition-shadow",
                    {
                        match cover_url {
                            Some(url) => rsx! {
                                img { class: "w-full h-full object-cover", src: "{url}" }
                            },
                            None => rsx! {
                                div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-indigo-600 to-purple-700",
                                    Icon {
                                        name: "playlist".to_string(),
                                        class: "w-12 h-12 text-white/70".to_string(),
                                    }
                                }
                            },
                        }
                    }
                    button {
                        class: "absolute top-3 right-3 p-2 rounded-full bg-zinc-950/80 text-zinc-200 hover:text-white hover:bg-emerald-500 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100 z-10",
                        aria_label: "Playlist options",
                        onclick: move |evt: MouseEvent| {
                            evt.stop_propagation();
                            let coords = evt.client_coordinates();
                            menu_x.set(coords.x);
                            menu_y.set(coords.y);
                            show_menu.set(!show_menu());
                        },
                        Icon {
                            name: "more-horizontal".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                    }
                    // Play overlay
                    div { class: "absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center",
                        div { class: "w-12 h-12 rounded-full bg-emerald-500 flex items-center justify-center shadow-xl transform scale-90 group-hover:scale-100 transition-transform",
                            Icon {
                                name: "play".to_string(),
                                class: "w-5 h-5 text-white ml-0.5".to_string(),
                            }
                        }
                    }
                }
                // Playlist info
                p { class: "font-medium text-white text-sm truncate group-hover:text-emerald-400 transition-colors",
                    "{playlist.name}"
                }
                p { class: "text-xs text-zinc-400",
                    "{playlist.song_count} songs • {format_duration(playlist.duration / 1000)}"
                }
            }

            // Context menu
            if show_menu() {
                div {
                    class: "fixed inset-0 z-[9998]",
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        show_menu.set(false);
                    },
                }
                div {
                    class: "fixed z-[9999] w-52 rounded-xl border border-zinc-700 bg-zinc-900/95 shadow-2xl p-1.5 space-y-1",
                    style: anchored_menu_style(menu_x(), menu_y(), 208.0, 320.0),
                    onclick: move |evt: MouseEvent| evt.stop_propagation(),
                    button {
                        class: if shuffle_enabled() {
                            "w-full flex items-center gap-2 px-2.5 py-2.5 rounded-lg text-sm text-emerald-300 bg-emerald-500/10 hover:bg-emerald-500/20 transition-colors"
                        } else {
                            "w-full flex items-center gap-2 px-2.5 py-2.5 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors"
                        },
                        onclick: on_shuffle,
                        Icon {
                            name: "shuffle".to_string(),
                            class: if shuffle_enabled() {
                                "w-4 h-4 text-emerald-300".to_string()
                            } else {
                                "w-4 h-4".to_string()
                            },
                        }
                        if shuffle_enabled() {
                            "Shuffle: On"
                        } else {
                            "Shuffle: Off"
                        }
                    }
                    if editing_allowed {
                        button {
                            class: "w-full flex items-center gap-2 px-2.5 py-2.5 rounded-lg text-sm text-red-300 hover:bg-red-500/10 transition-colors",
                            onclick: move |_: MouseEvent| {
                                show_menu.set(false);
                                delete_error.set(None);
                                show_delete_confirm.set(true);
                            },
                            Icon {
                                name: "trash".to_string(),
                                class: "w-4 h-4".to_string(),
                            }
                            "Delete Playlist"
                        }
                    }
                    div { class: "border-t border-zinc-700/60 my-1" }
                    button {
                        class: "w-full flex items-center gap-2 px-2.5 py-2.5 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                        onclick: {
                            let mut add_menu = add_menu.clone();
                            let playlist = playlist.clone();
                            move |_: MouseEvent| {
                                show_menu.set(false);
                                add_menu.open(AddIntent::from_playlist(&playlist));
                            }
                        },
                        Icon {
                            name: "plus".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        "Add to..."
                    }
                }
            }

            // Delete confirm dialog
            if show_delete_confirm() {
                div {
                    class: "fixed inset-0 z-[10000] flex items-center justify-center bg-black/60",
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        if !deleting() {
                            show_delete_confirm.set(false);
                        }
                    },
                    div {
                        class: "bg-zinc-900 border border-zinc-700 rounded-2xl p-6 max-w-sm w-full mx-4 shadow-2xl",
                        onclick: move |evt: MouseEvent| evt.stop_propagation(),
                        h3 { class: "text-lg font-semibold text-white mb-2", "Delete playlist?" }
                        p { class: "text-sm text-zinc-400 mb-4",
                            "Are you sure you want to delete \"{playlist.name}\"? This action cannot be undone."
                        }
                        if let Some(err) = delete_error() {
                            p { class: "text-sm text-red-400 mb-3", "{err}" }
                        }
                        div { class: "flex gap-3 justify-end",
                            button {
                                class: "px-4 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-zinc-500 transition-colors text-sm",
                                disabled: deleting(),
                                onclick: move |_| show_delete_confirm.set(false),
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 rounded-lg bg-red-500/20 border border-red-500/60 text-red-300 hover:text-white hover:bg-red-500/30 transition-colors text-sm",
                                disabled: deleting(),
                                onclick: on_confirm_delete,
                                if deleting() {
                                    "Deleting..."
                                } else {
                                    "Delete"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
