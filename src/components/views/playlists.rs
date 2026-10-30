//! Playlists: one inset list (covers, name, "N songs · length", owner when it
//! isn't you), with search, one menu for owner / sort / auto-imported, a
//! Create sheet, and a "…" menu per row.

use crate::api::*;
use crate::components::ui::{
    use_layout, ActionMenu, ActionMenuGap, ActionMenuItem, Artwork, EmptyState, IconButton,
    IconButtonSize, SearchField, VirtualList,
};
use crate::components::views::library::playlist_sheets::{
    CreatePlaylistSheet, DeletePlaylistSheet, RenamePlaylistSheet,
};
use crate::components::views::library::shared::{
    use_servers,
    active_servers, format_total, is_auto_imported, use_debounced, use_player, ErrorState,
    ListSkeleton, NoServers,
};
use crate::components::{AddIntent, AddMenuController, AppView, Icon, Navigation};
use dioxus::prelude::*;

const SORTS: [(&str, &str); 4] = [
    ("newest", "Newest"),
    ("name", "Name"),
    ("created", "Date Created"),
    ("changed", "Last Changed"),
];

#[derive(Clone, PartialEq)]
enum Pending {
    Rename(Playlist),
    Delete(Playlist),
}

/// What the row "…" menu is open for, and where.
#[derive(Clone, PartialEq)]
struct RowMenu {
    playlist: Playlist,
    anchor: Option<(f64, f64)>,
}

fn owner_line(playlist: &Playlist, me: &[String]) -> Option<String> {
    let owner = playlist.owner.as_deref()?.trim();
    if owner.is_empty() || me.iter().any(|m| m.eq_ignore_ascii_case(owner)) {
        None
    } else {
        Some(owner.to_string())
    }
}

fn subtitle_for(playlist: &Playlist, me: &[String]) -> String {
    let noun = if playlist.song_count == 1 { "song" } else { "songs" };
    let mut text = format!("{} {noun}", playlist.song_count);
    if playlist.duration > 0 {
        // Subsonic playlist durations are seconds.
        text.push_str(&format!(" · {}", format_total(playlist.duration as u64)));
    }
    if let Some(owner) = owner_line(playlist, me) {
        text.push_str(&format!(" · by {owner}"));
    }
    text
}

/// Loads the songs of a playlist and plays them (row menu Play / Shuffle).
async fn fetch_playlist_songs(server: ServerConfig, playlist_id: String) -> Result<Vec<Song>, String> {
    NavidromeClient::new(server)
        .get_playlist(&playlist_id)
        .await
        .map(|(_, songs)| songs)
}

#[component]
pub fn PlaylistsView() -> Element {
    let servers = use_servers();
    let navigation = use_context::<Navigation>();
    let add_menu = use_context::<AddMenuController>();
    let player = use_player();
    let layout = use_layout();
    let mut search = use_signal(String::new);
    let query = use_debounced(search, 200);
    let mut refresh = use_signal(|| 0u32);
    let mut hide_auto = use_signal(|| true);
    let mut owner_filter = use_signal(|| None::<String>);
    let mut sort = use_signal(|| "newest".to_string());
    let mut filter_at = use_signal(|| None::<Option<(f64, f64)>>);
    let mut row_menu = use_signal(|| None::<RowMenu>);
    let mut pending = use_signal(|| None::<Pending>);
    let mut creating = use_signal(|| false);
    let mut status = use_signal(|| None::<String>);

    let playlists = use_resource(move || {
        let _ = refresh();
        let active = active_servers(&servers());
        async move {
            let mut all = Vec::new();
            let mut last_error = None;
            for server in active {
                match NavidromeClient::new(server).get_playlists().await {
                    Ok(list) => all.extend(list),
                    Err(err) => last_error = Some(err),
                }
            }
            match (all.is_empty(), last_error) {
                (true, Some(err)) => Err(err),
                _ => Ok(all),
            }
        }
    });

    let me: Vec<String> = servers.read().iter().map(|s| s.username.clone()).collect();
    let single_server = servers.read().iter().filter(|s| s.active).count() == 1;

    let filtered = use_memo(move || {
        let guard = playlists.read();
        let Some(Ok(list)) = guard.as_ref() else {
            return Vec::<Playlist>::new();
        };
        let q = query().to_lowercase();
        let owner = owner_filter();
        let mut out: Vec<Playlist> = list
            .iter()
            .filter(|p| !hide_auto() || !is_auto_imported(p))
            .filter(|p| match owner.as_deref() {
                Some(o) => p.owner.as_deref() == Some(o),
                None => true,
            })
            .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q))
            .cloned()
            .collect();
        match sort().as_str() {
            "name" => out.sort_by_cached_key(|p| p.name.to_lowercase()),
            "created" => out.sort_by(|a, b| b.created.cmp(&a.created)),
            "changed" => out.sort_by(|a, b| b.changed.cmp(&a.changed)),
            _ => out.sort_by(|a, b| {
                b.changed
                    .cmp(&a.changed)
                    .then_with(|| b.created.cmp(&a.created))
            }),
        }
        out
    });

    let owners = use_memo(move || {
        let guard = playlists.read();
        let mut names: Vec<String> = match guard.as_ref() {
            Some(Ok(list)) => list.iter().filter_map(|p| p.owner.clone()).collect(),
            _ => Vec::new(),
        };
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        names
    });

    let server_of = move |playlist: &Playlist| {
        servers
            .peek()
            .iter()
            .find(|s| s.id == playlist.server_id)
            .cloned()
    };

    let mut play_playlist = move |playlist: Playlist, shuffle: bool| {
        let Some(server) = server_of(&playlist) else {
            return;
        };
        status.set(None);
        spawn(async move {
            match fetch_playlist_songs(server, playlist.id.clone()).await {
                Ok(songs) => {
                    let source = Some((
                        QueueSourceKind::Playlist,
                        format!("{}::{}", playlist.server_id, playlist.id),
                    ));
                    let result = if shuffle {
                        player.shuffle_play(songs, source)
                    } else {
                        player.play(songs, 0, source)
                    };
                    status.set(result.err());
                }
                Err(err) => status.set(Some(format!("Couldn't load “{}”: {err}", playlist.name))),
            }
        });
    };

    let row_h = if layout.is_compact() { 58.0 } else { 62.0 };
    let has_servers = servers.read().iter().any(|s| s.active);
    let me_for_rows = me.clone();
    let rows = filtered.read().len();
    let filters_on = owner_filter().is_some() || !hide_auto();
    let sort_text = SORTS
        .iter()
        .find(|(v, _)| *v == sort().as_str())
        .map(|(_, l)| *l)
        .unwrap_or("Newest");

    let body = match &*playlists.read() {
        _ if !has_servers => rsx! { NoServers {} },
        None => rsx! { ListSkeleton { rows: 8 } },
        Some(Err(err)) => {
            let err = err.clone();
            rsx! {
                ErrorState {
                    title: "Couldn't load playlists".to_string(),
                    error: err,
                    on_retry: move |_| refresh += 1,
                }
            }
        }
        Some(Ok(all)) if rows == 0 => {
            if all.is_empty() {
                rsx! {
                    EmptyState {
                        icon: "playlist".to_string(),
                        title: "No playlists yet".to_string(),
                        message: if single_server {
                            "Make one with the + button, or add songs to a new playlist from any track menu.".to_string()
                        } else {
                            "Playlists you make on your server show up here.".to_string()
                        },
                    }
                }
            } else {
                rsx! {
                    EmptyState {
                        icon: "search".to_string(),
                        title: "No playlists match".to_string(),
                        message: "Try another search, or change the filters in the menu.".to_string(),
                    }
                }
            }
        }
        Some(Ok(_)) => rsx! {
            VirtualList {
                item_count: rows,
                row_height: row_h,
                class: "rs-list rs-library-playlists".to_string(),
                testid: "playlist-list".to_string(),
                render_row: move |i: usize| {
                    let Some(playlist) = filtered.read().get(i).cloned() else {
                        return rsx! { li {} };
                    };
                    let subtitle = subtitle_for(&playlist, &me_for_rows);
                    let target = AppView::PlaylistDetailView {
                        playlist_id: playlist.id.clone(),
                        server_id: playlist.server_id.clone(),
                    };
                    let for_menu = playlist.clone();
                    rsx! {
                        li { "data-index": "{i}",
                            div { class: "rs-row rs-library-prow",
                                button {
                                    r#type: "button",
                                    class: "rs-library-prow__open",
                                    "data-testid": "playlist-row",
                                    oncontextmenu: {
                                        let for_menu = for_menu.clone();
                                        move |evt: MouseEvent| {
                                            evt.prevent_default();
                                            let p = evt.client_coordinates();
                                            row_menu.set(Some(RowMenu { playlist: for_menu.clone(), anchor: Some((p.x, p.y)) }));
                                        }
                                    },
                                    onclick: move |_| navigation.navigate_to(target.clone()),
                                    Artwork {
                                        cover_art: playlist.cover_art.clone(),
                                        server_id: playlist.server_id.clone(),
                                        size_px: 44,
                                        placeholder: "playlist".to_string(),
                                    }
                                    span { class: "rs-row__meta",
                                        span { class: "rs-row__title", span { "{playlist.name}" } }
                                        span { class: "rs-row__sub", style: "display:block", "{subtitle}" }
                                    }
                                }
                                IconButton {
                                    icon: "more-horizontal".to_string(),
                                    label: format!("More for {}", playlist.name),
                                    quiet: true,
                                    size: IconButtonSize::Medium,
                                    class: "rs-library-prow__more".to_string(),
                                    onclick: {
                                        let for_menu = for_menu.clone();
                                        move |evt: MouseEvent| {
                                            let p = evt.client_coordinates();
                                            row_menu.set(Some(RowMenu { playlist: for_menu.clone(), anchor: Some((p.x - 250.0, p.y + 12.0)) }));
                                        }
                                    },
                                }
                            }
                        }
                    }
                },
            }
            p { class: "rs-list-foot", if rows == 1 { "1 playlist" } else { "{rows} playlists" } }
        },
    };

    rsx! {
        div { class: "rs-view rs-library-playlists-view", "data-testid": "playlists",
            div { class: "rs-page-head",
                h1 { class: "rs-large-title", "Playlists" }
                div { class: "rs-library-head-tools",
                    button {
                        r#type: "button",
                        class: "rs-library-menubtn",
                        class: if filters_on { "rs-library-menubtn--on" },
                        aria_haspopup: "menu",
                        "data-testid": "playlists-filter",
                        onclick: move |evt: MouseEvent| {
                            let p = evt.client_coordinates();
                            filter_at.set(Some(Some((p.x - 220.0, p.y + 16.0))));
                        },
                        "{sort_text}"
                        Icon { name: "chevron-down".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                    }
                    IconButton {
                        icon: "plus".to_string(),
                        label: if single_server { "New Playlist".to_string() } else { "New Playlist (needs exactly one active server)".to_string() },
                        disabled: !single_server,
                        testid: "playlist-create".to_string(),
                        onclick: move |_| creating.set(true),
                    }
                }
            }
            SearchField {
                value: search(),
                placeholder: "Search playlists".to_string(),
                testid: "playlists-search".to_string(),
                class: "rs-library-search".to_string(),
                on_input: move |value: String| search.set(value),
            }
            if has_servers && !single_server {
                p { class: "rs-library-note", role: "note",
                    "Creating playlists needs exactly one active server. Turn the others off in Settings to make one here."
                }
            }
            if let Some(text) = status() {
                p { class: "rs-library-status", role: "status", "{text}" }
            }
            {body}

            if let Some(anchor) = filter_at() {
                ActionMenu {
                    label: "Sort and filter playlists".to_string(),
                    anchor,
                    on_close: move |_| filter_at.set(None),
                    testid: "playlists-filter-menu".to_string(),
                    div { class: "rs-menu__note", "Sort by" }
                    for (value, label) in SORTS {
                        ActionMenuItem {
                            key: "{value}",
                            label: label.to_string(),
                            checked: sort() == value,
                            onclick: move |_| {
                                filter_at.set(None);
                                sort.set(value.to_string());
                            },
                        }
                    }
                    ActionMenuGap {}
                    div { class: "rs-menu__note", "Owner" }
                    ActionMenuItem {
                        label: "All Owners".to_string(),
                        checked: owner_filter().is_none(),
                        onclick: move |_| {
                            filter_at.set(None);
                            owner_filter.set(None);
                        },
                    }
                    for name in owners() {
                        ActionMenuItem {
                            key: "{name}",
                            label: name.clone(),
                            checked: owner_filter().as_deref() == Some(name.as_str()),
                            onclick: {
                                let name = name.clone();
                                move |_| {
                                    filter_at.set(None);
                                    owner_filter.set(Some(name.clone()));
                                }
                            },
                        }
                    }
                    ActionMenuGap {}
                    ActionMenuItem {
                        label: "Hide Auto-Imported".to_string(),
                        checked: hide_auto(),
                        testid: "playlists-hide-auto".to_string(),
                        onclick: move |_| {
                            filter_at.set(None);
                            hide_auto.toggle();
                        },
                    }
                    ActionMenuItem {
                        label: "Refresh".to_string(),
                        icon: "refresh-cw".to_string(),
                        onclick: move |_| {
                            filter_at.set(None);
                            for server in active_servers(&servers.peek()) {
                                NavidromeClient::new(server).refresh_playlist_cache();
                            }
                            refresh += 1;
                        },
                    }
                }
            }

            if let Some(menu) = row_menu() {
                {
                    let playlist = menu.playlist.clone();
                    let editable = !is_auto_imported(&playlist);
                    rsx! {
                        ActionMenu {
                            label: format!("{} actions", playlist.name),
                            anchor: menu.anchor,
                            on_close: move |_| row_menu.set(None),
                            testid: "playlist-row-menu".to_string(),
                            ActionMenuItem {
                                label: "Play".to_string(),
                                icon: "play".to_string(),
                                onclick: {
                                    let playlist = playlist.clone();
                                    move |_| {
                                        row_menu.set(None);
                                        play_playlist(playlist.clone(), false);
                                    }
                                },
                            }
                            ActionMenuItem {
                                label: "Shuffle".to_string(),
                                icon: "shuffle".to_string(),
                                onclick: {
                                    let playlist = playlist.clone();
                                    move |_| {
                                        row_menu.set(None);
                                        play_playlist(playlist.clone(), true);
                                    }
                                },
                            }
                            ActionMenuItem {
                                label: "Add to…".to_string(),
                                icon: "plus".to_string(),
                                onclick: {
                                    let playlist = playlist.clone();
                                    let add_menu = add_menu.clone();
                                    move |_| {
                                        row_menu.set(None);
                                        let mut add_menu = add_menu.clone();
                                        add_menu.open(AddIntent::from_playlist(&playlist));
                                    }
                                },
                            }
                            ActionMenuGap {}
                            if editable {
                                ActionMenuItem {
                                    label: "Rename…".to_string(),
                                    icon: "edit".to_string(),
                                    testid: "playlist-rename".to_string(),
                                    onclick: {
                                        let playlist = playlist.clone();
                                        move |_| {
                                            row_menu.set(None);
                                            pending.set(Some(Pending::Rename(playlist.clone())));
                                        }
                                    },
                                }
                                ActionMenuItem {
                                    label: "Delete…".to_string(),
                                    icon: "trash".to_string(),
                                    danger: true,
                                    testid: "playlist-delete".to_string(),
                                    onclick: {
                                        let playlist = playlist.clone();
                                        move |_| {
                                            row_menu.set(None);
                                            pending.set(Some(Pending::Delete(playlist.clone())));
                                        }
                                    },
                                }
                            } else {
                                div { class: "rs-menu__note",
                                    "Auto-imported from a playlist file on the server, so it can't be renamed or deleted here."
                                }
                            }
                        }
                    }
                }
            }

            if creating() {
                CreatePlaylistSheet {
                    on_close: move |_| creating.set(false),
                    on_created: move |_| {
                        creating.set(false);
                        refresh += 1;
                    },
                }
            }
            match pending() {
                Some(Pending::Rename(playlist)) => rsx! {
                    RenamePlaylistSheet {
                        playlist,
                        on_close: move |_| pending.set(None),
                        on_renamed: move |_| {
                            pending.set(None);
                            refresh += 1;
                        },
                    }
                },
                Some(Pending::Delete(playlist)) => rsx! {
                    DeletePlaylistSheet {
                        playlist,
                        on_close: move |_| pending.set(None),
                        on_deleted: move |_| {
                            pending.set(None);
                            refresh += 1;
                        },
                    }
                },
                None => rsx! {},
            }
        }
    }
}
