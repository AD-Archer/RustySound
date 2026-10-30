//! Radio: Navidrome internet-radio stations.
//!
//! Stations are an inset list. The playing station shows the live ICY
//! "now playing" line; the ICY stream is polled only while a station is
//! actually playing, and the poll restarts only when the station or the
//! play/pause state changes (not when the title it writes changes).

use crate::api::*;
use crate::components::ui::env::{is_radio_song, sleep_ms};
use crate::components::ui::{
    use_long_press, Menu, EmptyState, IconButton, IconButtonSize, InsetList, ListInset,
    MenuAction, Button, ButtonVariant, Sheet, Skeleton,
};
use crate::components::{AppView, Icon, IsPlayingSignal, Navigation};
use dioxus::prelude::*;

const ICY_POLL_MS: u64 = 6500;
const DEMO_NAME: &str = "Downtown Hot Radio";
const DEMO_STREAM: &str = "https://usa11.fastcast4u.com/proxy/downtownhott?mp=/1";
const DEMO_HOME: &str = "https://downtownhottradio.com";

#[derive(Clone, PartialEq)]
enum Form {
    Add,
    Edit(RadioStation),
}

/// "example.com" from a URL, for the row subtitle.
fn domain(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    host.trim_start_matches("www.").to_string()
}

fn station_song(station: &RadioStation) -> Song {
    Song {
        id: station.id.clone(),
        title: "Unknown Song".to_string(),
        artist: Some("Unknown Artist".to_string()),
        album: Some(station.name.clone()),
        content_type: Some("audio/mpeg".to_string()),
        stream_url: Some(station.stream_url.clone()),
        server_id: station.server_id.clone(),
        server_name: "Radio".to_string(),
        ..Song::default()
    }
}

#[component]
pub fn RadioView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<IsPlayingSignal>().0;

    let mut refresh_key = use_signal(|| 0u32);
    let mut stations = use_signal(|| None::<Vec<RadioStation>>);
    let mut form = use_signal(|| None::<Form>);
    let mut row_menu = use_signal(|| None::<(RadioStation, Option<(f64, f64)>)>);
    let mut confirm_delete = use_signal(|| None::<RadioStation>);
    let mut error = use_signal(|| None::<String>);
    let mut poll_generation = use_signal(|| 0u64);

    use_effect(move || {
        let _ = refresh_key();
        let active: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
        spawn(async move {
            let mut found = Vec::new();
            for server in active {
                if let Ok(list) = NavidromeClient::new(server).get_internet_radio_stations().await {
                    found.extend(list);
                }
            }
            stations.set(Some(found));
        });
    });

    // What the ICY poll depends on: the playing radio entry's identity (not
    // its title, which the poll itself rewrites) and whether it is playing.
    let live_station = use_memo(move || {
        now_playing
            .read()
            .as_ref()
            .filter(|s| is_radio_song(s) && s.duration == 0)
            .and_then(|s| {
                s.stream_url
                    .clone()
                    .map(|url| (s.id.clone(), url, s.album.clone()))
            })
    });
    let playing = use_memo(move || is_playing());

    use_effect(move || {
        let station = live_station();
        let active = playing();
        poll_generation.with_mut(|g| *g += 1);
        let generation = *poll_generation.peek();
        let (Some((song_id, stream_url, station_name)), true) = (station, active) else {
            return;
        };
        let fallback = station_name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "Internet Radio".to_string());
        spawn(async move {
            let mut last_raw = String::new();
            loop {
                if *poll_generation.peek() != generation {
                    break;
                }
                if let Ok(Some(meta)) = NavidromeClient::read_icy_now_playing(&stream_url).await {
                    if *poll_generation.peek() != generation {
                        break;
                    }
                    if !meta.raw_title.is_empty() && meta.raw_title != last_raw {
                        last_raw = meta.raw_title.clone();
                        let artist = meta
                            .artist
                            .filter(|a| !a.trim().is_empty())
                            .unwrap_or_else(|| fallback.clone());
                        let mut title = meta.title.trim().to_string();
                        if title.is_empty()
                            || title.eq_ignore_ascii_case(&fallback)
                            || title.eq_ignore_ascii_case(&artist)
                        {
                            title = "Unknown Song".to_string();
                        }
                        let apply = |song: &mut Song| {
                            if song.id == song_id {
                                song.title = title.clone();
                                song.artist = Some(artist.clone());
                            }
                        };
                        now_playing.with_mut(|current| {
                            if let Some(song) = current.as_mut() {
                                apply(song);
                            }
                        });
                        let idx = *queue_index.peek();
                        queue.with_mut(|items| {
                            if let Some(song) = items.get_mut(idx) {
                                apply(song);
                            }
                        });
                    }
                }
                sleep_ms(ICY_POLL_MS).await;
            }
        });
    });

    let mut play = move |station: RadioStation| {
        let song = station_song(&station);
        queue.set(vec![song.clone()]);
        queue_index.set(0);
        now_playing.set(Some(song));
        is_playing.set(true);
    };

    let add_demo = move |_| {
        let server = servers.peek().iter().find(|s| s.active).cloned();
        let Some(server) = server else {
            error.set(Some("Add and activate a server first.".to_string()));
            return;
        };
        spawn(async move {
            match NavidromeClient::new(server)
                .create_internet_radio_station(DEMO_NAME, DEMO_STREAM, Some(DEMO_HOME))
                .await
            {
                Ok(_) => refresh_key.with_mut(|k| *k += 1),
                Err(err) => error.set(Some(format!("Couldn't add the demo station: {err}"))),
            }
        });
    };

    let delete = move |_key: String| {
        let Some(station) = confirm_delete.peek().clone() else { return };
        confirm_delete.set(None);
        let server = servers.peek().iter().find(|s| s.id == station.server_id).cloned();
        let Some(server) = server else {
            error.set(Some("That station's server is no longer configured.".to_string()));
            return;
        };
        spawn(async move {
            match NavidromeClient::new(server)
                .delete_internet_radio_station(&station.id)
                .await
            {
                Ok(_) => stations.with_mut(|list| {
                    if let Some(list) = list.as_mut() {
                        list.retain(|s| !(s.id == station.id && s.server_id == station.server_id));
                    }
                }),
                Err(err) => error.set(Some(format!("Couldn't delete the station: {err}"))),
            }
        });
    };

    let has_servers = servers.read().iter().any(|s| s.active);
    let live = live_station();
    let now = now_playing.read().clone();
    let list = stations();

    rsx! {
        div { class: "rs-view rs-view--narrow", "data-testid": "radio",
            div { class: "rs-page-head",
                h1 { class: "rs-large-title", "Radio" }
                if has_servers {
                    div { class: "rs-personal-head-actions",
                        IconButton {
                            icon: "plus".to_string(),
                            label: "Add Station".to_string(),
                            testid: "radio-add".to_string(),
                            onclick: move |_| {
                                error.set(None);
                                form.set(Some(Form::Add));
                            },
                        }
                    }
                }
            }

            if let Some(message) = error() {
                p { class: "rs-personal-error", role: "alert", "{message}" }
            }

            if !has_servers {
                EmptyState {
                    icon: "radio".to_string(),
                    title: "No servers connected".to_string(),
                    message: "Radio stations live on your Navidrome server. Add a server to listen.".to_string(),
                    Button {
                        label: "Add Server".to_string(),
                        onclick: move |_| navigation.navigate_to(AppView::SettingsView {}),
                    }
                }
            } else {
                match list {
                    None => rsx! { Skeleton { rows: 4 } },
                    Some(list) if list.is_empty() => rsx! {
                        EmptyState {
                            icon: "radio".to_string(),
                            title: "No radio stations".to_string(),
                            message: "Add an internet radio stream by its URL, or try a demo station.".to_string(),
                            testid: "radio-empty".to_string(),
                            Button {
                                label: "Add Station".to_string(),
                                icon: "plus".to_string(),
                                onclick: move |_| form.set(Some(Form::Add)),
                            }
                            Button {
                                label: "Add Demo Station".to_string(),
                                variant: ButtonVariant::Tinted,
                                testid: "radio-demo".to_string(),
                                onclick: add_demo,
                            }
                        }
                    },
                    Some(list) => rsx! {
                        InsetList { inset: ListInset::Art, label: "Stations".to_string(), testid: "radio-list".to_string(),
                            for station in list {
                                {
                                    let is_live = live
                                        .as_ref()
                                        .map(|(id, url, _)| *id == station.id && *url == station.stream_url)
                                        .unwrap_or(false);
                                    let now_line = if is_live {
                                        now.as_ref().map(|song| {
                                            let artist = song.artist.clone().unwrap_or_default();
                                            if song.title == "Unknown Song" {
                                                if is_playing() { "Live".to_string() } else { "Paused".to_string() }
                                            } else if artist.is_empty() || artist == station.name {
                                                song.title.clone()
                                            } else {
                                                format!("{} — {}", artist, song.title)
                                            }
                                        })
                                    } else {
                                        None
                                    };
                                    rsx! {
                                        StationRow {
                                            key: "{station.server_id}:{station.id}",
                                            station: station.clone(),
                                            now_line,
                                            live: is_live && is_playing(),
                                            on_play: move |station| play(station),
                                            on_menu: move |(station, anchor)| row_menu.set(Some((station, anchor))),
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
            }

            if let Some((station, anchor)) = row_menu() {
                Menu {
                    title: station.name.clone(),
                    anchor,
                    testid: "radio-menu".to_string(),
                    actions: vec![
                        MenuAction::new("play", "Play", "play"),
                        MenuAction::new("edit", "Edit Station…", "edit"),
                        MenuAction::new("delete", "Delete Station…", "trash").danger(),
                    ],
                    on_close: move |_| row_menu.set(None),
                    on_action: {
                        let station = station.clone();
                        move |key: String| match key.as_str() {
                            "play" => play(station.clone()),
                            "edit" => {
                                error.set(None);
                                form.set(Some(Form::Edit(station.clone())));
                            }
                            "delete" => confirm_delete.set(Some(station.clone())),
                            _ => {}
                        }
                    },
                }
            }
            if let Some(station) = confirm_delete() {
                Menu {
                    title: "Delete Station?".to_string(),
                    message: format!("“{}” will be removed from your server.", station.name),
                    testid: "radio-confirm".to_string(),
                    actions: vec![MenuAction::new("delete", "Delete", "trash").danger()],
                    on_close: move |_| confirm_delete.set(None),
                    on_action: delete,
                }
            }
            if let Some(mode) = form() {
                StationForm {
                    mode,
                    on_close: move |_| form.set(None),
                    on_saved: move |_| {
                        form.set(None);
                        refresh_key.with_mut(|k| *k += 1);
                    },
                }
            }
        }
    }
}

#[component]
fn StationRow(
    station: RadioStation,
    now_line: Option<String>,
    live: bool,
    on_play: EventHandler<RadioStation>,
    on_menu: EventHandler<(RadioStation, Option<(f64, f64)>)>,
) -> Element {
    let press = use_long_press();
    let site = station
        .home_page_url
        .clone()
        .filter(|u| !u.trim().is_empty())
        .map(|u| domain(&u))
        .unwrap_or_else(|| domain(&station.stream_url));
    let playing_row = now_line.is_some();
    rsx! {
        li { "data-testid": "radio-row",
            div {
                class: "rs-personal-row",
                class: if playing_row { "rs-personal-row--playing" },
                button {
                    r#type: "button",
                    class: "rs-row",
                    aria_label: "Play {station.name}",
                    aria_current: if playing_row { Some("true") } else { None },
                    onclick: {
                        let station = station.clone();
                        move |_| {
                            if press.take_fired() {
                                return;
                            }
                            on_play.call(station.clone());
                        }
                    },
                    oncontextmenu: {
                        let station = station.clone();
                        move |evt: MouseEvent| {
                            evt.prevent_default();
                            press.cancel();
                            let p = evt.client_coordinates();
                            on_menu.call((station.clone(), Some((p.x, p.y))));
                        }
                    },
                    onpointerdown: {
                        let station = station.clone();
                        move |evt: PointerEvent| {
                            let station = station.clone();
                            press.down(&evt, move |p| on_menu.call((station, Some(p))));
                        }
                    },
                    onpointermove: move |evt| press.moved(&evt),
                    onpointerup: move |_| press.cancel(),
                    onpointercancel: move |_| press.cancel(),
                    span { class: "rs-art", style: "width:44px;height:44px;display:grid;place-items:center",
                        if playing_row {
                            span {
                                class: "rs-eq",
                                class: if live { "rs-eq--live" },
                                role: "img",
                                aria_label: "Now playing",
                                i {}
                                i {}
                                i {}
                            }
                        } else {
                            span { class: "rs-row__lead",
                                Icon { name: "radio".to_string(), class: "rs-ic rs-ic--md".to_string() }
                            }
                        }
                    }
                    span { class: "rs-row__meta",
                        span { class: "rs-row__title", span { "{station.name}" } }
                        if let Some(line) = now_line {
                            span { class: "rs-row__sub rs-personal-live", style: "display:block", "data-testid": "radio-now", "{line}" }
                        }
                        span { class: "rs-row__sub", style: "display:block", "{site}" }
                    }
                }
                IconButton {
                    icon: "more-horizontal".to_string(),
                    label: "More for {station.name}".to_string(),
                    quiet: true,
                    size: IconButtonSize::Small,
                    testid: "radio-row-more".to_string(),
                    onclick: {
                        let station = station.clone();
                        move |evt: MouseEvent| {
                            let p = evt.client_coordinates();
                            on_menu.call((station.clone(), Some((p.x - 250.0, p.y + 12.0))));
                        }
                    },
                }
            }
        }
    }
}

/// Add / Edit sheet: Name, Stream URL, Homepage, Server.
#[component]
fn StationForm(mode: Form, on_close: EventHandler<()>, on_saved: EventHandler<()>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let editing = match &mode {
        Form::Edit(station) => Some(station.clone()),
        Form::Add => None,
    };
    let mut name = use_signal(|| editing.as_ref().map(|s| s.name.clone()).unwrap_or_default());
    let mut stream = use_signal(|| editing.as_ref().map(|s| s.stream_url.clone()).unwrap_or_default());
    let mut home = use_signal(|| {
        editing
            .as_ref()
            .and_then(|s| s.home_page_url.clone())
            .unwrap_or_default()
    });
    let mut server_id = use_signal(|| {
        editing
            .as_ref()
            .map(|s| s.server_id.clone())
            .or_else(|| servers.peek().iter().find(|s| s.active).map(|s| s.id.clone()))
            .unwrap_or_default()
    });
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let active: Vec<ServerConfig> = servers.read().iter().filter(|s| s.active).cloned().collect();

    let save = {
        let editing = editing.clone();
        move |evt: FormEvent| {
            evt.prevent_default();
            if *saving.peek() {
                return;
            }
            let (n, url, page, sid) = (
                name.peek().trim().to_string(),
                stream.peek().trim().to_string(),
                home.peek().trim().to_string(),
                server_id.peek().clone(),
            );
            if n.is_empty() {
                error.set(Some("Give the station a name.".to_string()));
                return;
            }
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                error.set(Some("The stream URL must start with http:// or https://.".to_string()));
                return;
            }
            let server = servers.peek().iter().find(|s| s.id == sid).cloned();
            let Some(server) = server else {
                error.set(Some("Choose a server for this station.".to_string()));
                return;
            };
            saving.set(true);
            error.set(None);
            let editing = editing.clone();
            spawn(async move {
                let client = NavidromeClient::new(server);
                let page = (!page.is_empty()).then_some(page);
                let result = match editing {
                    Some(station) => {
                        client
                            .update_internet_radio_station(&station.id, &n, &url, page.as_deref())
                            .await
                    }
                    None => client.create_internet_radio_station(&n, &url, page.as_deref()).await,
                };
                saving.set(false);
                match result {
                    Ok(_) => on_saved.call(()),
                    Err(err) => error.set(Some(format!("The server refused the station: {err}"))),
                }
            });
        }
    };

    rsx! {
        Sheet {
            title: if editing.is_some() { "Edit Station".to_string() } else { "Add Station".to_string() },
            testid: "radio-form".to_string(),
            on_close: move |_| on_close.call(()),
            form { onsubmit: save,
                ul { class: "rs-list rs-list--text rs-personal-form",
                    li {
                        label { class: "rs-personal-field",
                            span { "Name" }
                            input {
                                r#type: "text",
                                placeholder: "Station name",
                                autocomplete: "off",
                                "data-testid": "radio-form-name",
                                value: "{name}",
                                oninput: move |e| name.set(e.value()),
                            }
                        }
                    }
                    li {
                        label { class: "rs-personal-field",
                            span { "Stream URL" }
                            input {
                                r#type: "url",
                                placeholder: "https://stream.example.com/live",
                                autocomplete: "off",
                                autocapitalize: "off",
                                spellcheck: "false",
                                "data-testid": "radio-form-stream",
                                value: "{stream}",
                                oninput: move |e| stream.set(e.value()),
                            }
                        }
                    }
                    li {
                        label { class: "rs-personal-field",
                            span { "Homepage" }
                            input {
                                r#type: "url",
                                placeholder: "Optional",
                                autocomplete: "off",
                                autocapitalize: "off",
                                spellcheck: "false",
                                value: "{home}",
                                oninput: move |e| home.set(e.value()),
                            }
                        }
                    }
                    li {
                        label { class: "rs-personal-field",
                            span { "Server" }
                            select {
                                disabled: editing.is_some() || active.len() < 2,
                                value: "{server_id}",
                                onchange: move |e| server_id.set(e.value()),
                                for server in active.iter() {
                                    option {
                                        key: "{server.id}",
                                        value: "{server.id}",
                                        selected: *server_id.read() == server.id,
                                        "{server.name}"
                                    }
                                }
                            }
                        }
                    }
                }
                p { class: "rs-personal-hint", "The stream URL must point straight at the audio stream, not a web page." }
                if let Some(message) = error() {
                    p { class: "rs-personal-error", role: "alert", "{message}" }
                }
                button {
                    r#type: "submit",
                    class: "rs-btn rs-btn--filled rs-btn--block",
                    disabled: saving(),
                    "data-testid": "radio-form-save",
                    if saving() { "Saving…" } else if editing.is_some() { "Save Changes" } else { "Add Station" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::domain;

    #[test]
    fn domain_strips_scheme_path_port_and_www() {
        assert_eq!(domain("https://www.example.com/live?x=1"), "example.com");
        assert_eq!(domain("http://radio.example.org:8000/stream"), "radio.example.org");
        assert_eq!(domain("example.net/path"), "example.net");
    }
}
