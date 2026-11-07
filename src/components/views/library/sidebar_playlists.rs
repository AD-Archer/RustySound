//! The sidebar's Playlists section (wide): the user's own editable
//! playlists, most recently changed first, compact and collapsible. Each is
//! a drop target: a song carried from any list with the mouse
//! (`ui/song_drop.rs`) is added to the playlist it's dropped on.

use super::shared::{active_servers, load_playlists, playlist_editable, PLAYLISTS_REVISION};
use crate::api::{Playlist, ServerConfig};
use crate::components::{use_servers, AppView, Icon};
use dioxus::prelude::*;

/// Playlists listed before "Show All".
const SHOWN: usize = 8;

/// The section. `current`: the playlist page open now (marked).
/// `on_open` opens a playlist's page.
#[component]
pub fn SidebarPlaylists(current: Option<(String, String)>, on_open: EventHandler<AppView>) -> Element {
    let servers = use_servers();
    let mut open = use_signal(|| true);
    let mut show_all = use_signal(|| false);

    // Each client caches its server's list, so this costs no request after
    // the Playlists page (or an earlier load) fetched it.
    let playlists = use_resource(move || {
        let _ = PLAYLISTS_REVISION();
        let active = active_servers(&servers());
        async move {
            let mut list: Vec<Playlist> = load_playlists(active.clone()).await.unwrap_or_default();
            list.retain(|p| playlist_editable(p, ServerConfig::find(&active, &p.server_id)));
            list.sort_by(|a, b| b.changed.cmp(&a.changed).then_with(|| b.created.cmp(&a.created)));
            list
        }
    });

    let guard = playlists.read();
    let Some(list) = guard.as_ref().filter(|list| !list.is_empty()) else {
        return rsx! {};
    };
    let total = list.len();
    let shown = if show_all() { total } else { total.min(SHOWN) };
    let expanded = open();

    rsx! {
        div { class: "rs-side__grp rs-library-sidelists", "data-testid": "sidebar-playlists",
            button {
                r#type: "button",
                class: "rs-side__head",
                aria_expanded: if expanded { "true" } else { "false" },
                "data-testid": "sidebar-playlists-toggle",
                onclick: move |_| open.toggle(),
                "Playlists"
                Icon {
                    name: if expanded { "chevron-down".to_string() } else { "chevron-right".to_string() },
                    class: "rs-library-sidelists__chev".to_string(),
                }
            }
            if expanded {
                for playlist in list.iter().take(shown) {
                    {
                        let here = current.as_ref().is_some_and(|(id, server)| {
                            *id == playlist.id && *server == playlist.server_id
                        });
                        let target = AppView::PlaylistDetailView {
                            playlist_id: playlist.id.clone(),
                            server_id: playlist.server_id.clone(),
                        };
                        rsx! {
                            button {
                                key: "{playlist.key()}",
                                r#type: "button",
                                class: "rs-nav",
                                "data-testid": "sidebar-playlist",
                                "data-drop-playlist": "{playlist.id}",
                                "data-drop-server": "{playlist.server_id}",
                                aria_current: if here { Some("page") } else { None },
                                onclick: move |_| on_open.call(target.clone()),
                                Icon { name: "playlist".to_string(), class: "rs-nav__icon".to_string() }
                                span { class: "rs-library-sidelists__name", "{playlist.name}" }
                            }
                        }
                    }
                }
                if total > SHOWN {
                    button {
                        r#type: "button",
                        class: "rs-nav rs-library-sidelists__more",
                        "data-testid": "sidebar-playlists-more",
                        onclick: move |_| show_all.toggle(),
                        if show_all() { "Show Less" } else { "Show All" }
                    }
                }
            }
        }
    }
}
