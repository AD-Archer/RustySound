//! Mix page (`/mix/<slug>`): a generated mix laid out like a playlist page,
//! so both read as one family. Stage band with the 2×2 mosaic, the
//! RustySound mark and "Made by RustySound" above the title, the blurb and
//! "N songs · length"; Play and Shuffle; the windowed song list; and "…" with
//! Play Next, Add to Queue and Save as Playlist.
//!
//! Data: [`load_mix`] reads today's mixes from the same cache Home uses (no
//! requests after Home has loaded), regenerates them from the day's seed
//! after a restart or on a deep link, and rebuilds an artist radio from the
//! ids in its slug. A mix it can't find shows a calm empty state.

use super::mixes::{count_label, mix_covers, MixArt, RsMark};
use super::saved::save_and_remember;
use crate::api::discovery::{load_mix, mix_id_from_slug, Mix, MixLookup};
use crate::api::{QueueSourceKind, Song};
use crate::components::ui::{
    Button, ButtonVariant, EmptyState, Menu, MenuGap, MenuItem, TrackRow, VirtualList,
};
use crate::components::views::library::hero::{DetailHero, DetailHeroSkeleton};
use crate::components::views::library::shared::{
    song_key, songs_summary, use_downloaded_keys, use_now_playing, use_player, use_servers,
    use_song_patches, ListSkeleton,
};
use crate::components::{AppView, Navigation};
use crate::db::AppSettings;
use dioxus::prelude::*;

#[component]
pub fn MixDetailView(mix_id: String) -> Element {
    let servers = use_servers();
    let settings = use_context::<Signal<AppSettings>>();
    let navigation = use_context::<Navigation>();
    let player = use_player();
    let mut songs = use_signal(Vec::<Song>::new);
    let mut status = use_signal(|| None::<String>);
    let mut menu_at = use_signal(|| None::<Option<(f64, f64)>>);
    let mut saving = use_signal(|| false);
    use_song_patches(songs);
    let downloaded = use_downloaded_keys(songs);
    let (playing_key, live) = use_now_playing();

    let data = use_resource(use_reactive!(|(mix_id)| {
        let servers = servers();
        // Peek: unrelated settings writes (volume, the saved-mix map) must
        // not regenerate the mix.
        let settings = settings.peek().clone();
        async move { load_mix(&servers, &settings, &mix_id_from_slug(&mix_id)).await }
    }));

    use_effect(move || {
        if let Some(MixLookup::Found(mix)) = &*data.read() {
            songs.set(mix.songs.clone());
        }
    });

    let mix: Mix = match &*data.read() {
        None => {
            return rsx! {
                div { class: "rs-view rs-view--flush rs-library-detail", "data-testid": "mix-loading",
                    DetailHeroSkeleton { back_label: "Home".to_string(), fallback: AppView::HomeView {} }
                    div { class: "rs-library-page", ListSkeleton { rows: 10 } }
                }
            };
        }
        Some(MixLookup::Found(mix)) => mix.clone(),
        Some(other) => {
            let offline = settings.peek().offline_mode;
            let message = match other {
                MixLookup::NoServer => "Connect a server and RustySound makes mixes from your listening.",
                _ if offline => "You're offline, so this mix can't be made right now. Home shows what's cached.",
                _ => "Mixes are made fresh each day from your listening, and this one isn't in today's set. Home has today's mixes.",
            };
            return rsx! {
                div { class: "rs-view rs-library-detail", "data-testid": "mix-missing",
                    EmptyState {
                        icon: "playlist".to_string(),
                        title: "This mix isn't available".to_string(),
                        message: message.to_string(),
                        Button {
                            label: "Go to Home".to_string(),
                            icon: "home".to_string(),
                            variant: ButtonVariant::Tinted,
                            testid: "mix-missing-home".to_string(),
                            onclick: move |_| navigation.navigate_to(AppView::HomeView {}),
                        }
                    }
                }
            };
        }
    };

    let count = songs.read().len();
    let total_secs: u64 = songs.read().iter().map(|s| u64::from(s.duration)).sum();
    let meta = if mix.freshness.is_empty() {
        count_label(&mix)
    } else {
        format!("{} · {}", count_label(&mix), mix.freshness)
    };
    let source = Some((QueueSourceKind::RandomMix, format!("discover:{}", mix.id)));
    let row_h = 58.0;

    let play_from = {
        let source = source.clone();
        move |index: usize| {
            let list = songs.peek().clone();
            status.set(player.play(list, index, source.clone()).err());
        }
    };
    let shuffle_all = {
        let source = source.clone();
        move |_| {
            let list = songs.peek().clone();
            status.set(player.shuffle_play(list, source.clone()).err());
        }
    };
    let save = {
        let mix = mix.clone();
        move |_| {
            menu_at.set(None);
            if saving() {
                return;
            }
            saving.set(true);
            status.set(Some(format!("Saving “{}”…", mix.name)));
            let servers_now = servers.peek().clone();
            let mix = mix.clone();
            spawn(async move {
                let message = match save_and_remember(mix, servers_now, settings).await {
                    Ok(message) | Err(message) => message,
                };
                status.set(Some(message));
                saving.set(false);
            });
        }
    };

    let playing_now = playing_key();
    let live_now = live();
    let covers = mix_covers(&mix);

    rsx! {
        div { class: "rs-view rs-view--flush rs-library-detail rs-mixpage", "data-testid": "mix-detail",
            DetailHero {
                title: mix.name.clone(),
                back_label: "Home".to_string(),
                fallback: AppView::HomeView {},
                art: rsx! {
                    div {
                        class: "rs-mixpage__art",
                        role: "img",
                        aria_label: "{mix.name} cover",
                        MixArt { covers, size_px: 220, eager: true }
                    }
                },
                eyebrow: rsx! {
                    span { class: "rs-mixpage__by", "data-testid": "mix-byline",
                        RsMark { class: "rs-mixmark--title".to_string() }
                        span { "Made by RustySound" }
                    }
                },
                meta,
                // What it is: the blurb (three lines at most on a phone).
                note: mix.blurb.clone(),
                more_testid: "mix-more".to_string(),
                on_more: move |evt: MouseEvent| {
                    let p = evt.client_coordinates();
                    menu_at.set(Some(Some((p.x - 250.0, p.y + 14.0))));
                },
            }

            div { class: "rs-library-page",
                div { class: "rs-library-actions",
                    Button {
                        label: "Play".to_string(),
                        icon: "play".to_string(),
                        block: true,
                        disabled: count == 0,
                        testid: "mix-play".to_string(),
                        onclick: {
                            let mut play_from = play_from.clone();
                            move |_| play_from(0)
                        },
                    }
                    Button {
                        label: "Shuffle".to_string(),
                        icon: "shuffle".to_string(),
                        variant: ButtonVariant::Tinted,
                        block: true,
                        disabled: count == 0,
                        testid: "mix-shuffle".to_string(),
                        onclick: shuffle_all,
                    }
                }
                if let Some(text) = status() {
                    p { class: "rs-library-status", role: "status", "data-testid": "mix-status", "{text}" }
                }
                VirtualList {
                    item_count: count,
                    row_height: row_h,
                    class: "rs-list rs-library-songlist".to_string(),
                    testid: "mix-tracks".to_string(),
                    render_row: move |index: usize| {
                        let Some(song) = songs.read().get(index).cloned() else {
                            return rsx! { li {} };
                        };
                        let key = song_key(&song);
                        let playing = playing_now.as_deref() == Some(key.as_str());
                        rsx! {
                            TrackRow {
                                key: "{key}:{index}",
                                playing,
                                live: playing && live_now,
                                downloaded: downloaded.read().contains(&key),
                                index,
                                song,
                                on_play: {
                                    let mut play_from = play_from.clone();
                                    move |_| play_from(index)
                                },
                            }
                        }
                    },
                }
                p { class: "rs-list-foot", "data-testid": "mix-foot",
                    "{songs_summary(count, total_secs)}. Made by RustySound from your listening; a new mix each day. Save as Playlist keeps a copy on your server."
                }
            }

            if let Some(anchor) = menu_at() {
                Menu {
                    title: mix.name.clone(),
                    message: format!("Made by RustySound · {}", count_label(&mix)),
                    anchor,
                    on_close: move |_| menu_at.set(None),
                    testid: "mix-page-menu".to_string(),
                    MenuItem {
                        label: "Play Next".to_string(),
                        icon: "play-next".to_string(),
                        disabled: count == 0,
                        testid: "mix-page-menu-next".to_string(),
                        onclick: move |_| {
                            menu_at.set(None);
                            player.enqueue(songs.peek().clone(), true);
                        },
                    }
                    MenuItem {
                        label: "Add to Queue".to_string(),
                        icon: "add-to-queue".to_string(),
                        disabled: count == 0,
                        testid: "mix-page-menu-queue".to_string(),
                        onclick: move |_| {
                            menu_at.set(None);
                            player.enqueue(songs.peek().clone(), false);
                            status.set(Some(format!("Added {count} songs to the queue.")));
                        },
                    }
                    MenuGap {}
                    MenuItem {
                        label: if saving() { "Saving…".to_string() } else { "Save as Playlist".to_string() },
                        icon: "playlist".to_string(),
                        disabled: saving() || count == 0,
                        testid: "mix-page-menu-save".to_string(),
                        onclick: save,
                    }
                    div { class: "rs-menu__note",
                        "Saving again replaces the earlier copy, so there's one playlist per mix."
                    }
                }
            }
        }
    }
}
