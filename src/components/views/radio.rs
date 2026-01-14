use crate::api::*;
use crate::components::Icon;
use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
enum RadioFormMode {
    Closed,
    Add,
    Edit(RadioStation),
}

#[component]
pub fn RadioView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();

    let form_mode = use_signal(|| RadioFormMode::Closed);
    let mut form_name = use_signal(String::new);
    let mut form_stream_url = use_signal(String::new);
    let mut form_home_page_url = use_signal(String::new);
    let mut form_server_id = use_signal(String::new);
    let error_message = use_signal(|| None::<String>);
    let is_saving = use_signal(|| false);
    let refresh_key = use_signal(|| 0u32);

    let stations = use_resource(move || {
        let _ = refresh_key();
        let servers = servers();
        async move {
            let mut stations = Vec::new();
            for server in servers.into_iter().filter(|s| s.active) {
                let client = NavidromeClient::new(server);
                if let Ok(server_stations) = client.get_internet_radio_stations().await {
                    stations.extend(server_stations);
                }
            }
            stations
        }
    });

    let server_list = servers();
    let active_servers: Vec<ServerConfig> =
        server_list.iter().cloned().filter(|s| s.active).collect();
    let has_active_servers = !active_servers.is_empty();

    let form_mode_value = form_mode();
    let is_form_open = !matches!(form_mode_value, RadioFormMode::Closed);
    let is_editing = matches!(form_mode_value, RadioFormMode::Edit(_));

    let selected_server_name = active_servers
        .iter()
        .find(|s| s.id == form_server_id())
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "Select a server".to_string());

    let on_open_add = {
        let servers = servers.clone();
        let mut form_mode = form_mode.clone();
        let mut form_name = form_name.clone();
        let mut form_stream_url = form_stream_url.clone();
        let mut form_home_page_url = form_home_page_url.clone();
        let mut form_server_id = form_server_id.clone();
        let mut error_message = error_message.clone();
        move |_| {
            let server_id = servers()
                .into_iter()
                .find(|s| s.active)
                .map(|s| s.id)
                .unwrap_or_default();
            if server_id.is_empty() {
                error_message.set(Some(
                    "Add an active server before creating radio stations.".to_string(),
                ));
                form_mode.set(RadioFormMode::Closed);
                return;
            }
            form_name.set(String::new());
            form_stream_url.set(String::new());
            form_home_page_url.set(String::new());
            form_server_id.set(server_id);
            error_message.set(None);
            form_mode.set(RadioFormMode::Add);
        }
    };

    let on_add_demo_station = {
        let servers = servers.clone();
        let mut error_message = error_message.clone();
        let mut refresh_key = refresh_key.clone();
        move |_| {
            let active_servers: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
            if active_servers.is_empty() {
                error_message.set(Some("No active servers found. Please add and activate a server first.".to_string()));
                return;
            }

            // For demo, use the first active server, but in a real implementation you might want to let the user choose
            let server = active_servers.into_iter().next().unwrap();
            let client = NavidromeClient::new(server);

            spawn(async move {
                match client.create_internet_radio_station(
                    "Downtown Hot Radio",
                    "https://usa11.fastcast4u.com/proxy/downtownhott?mp=/1",
                    Some("https://downtownhottradio.com"),
                ).await {
                    Ok(_) => {
                        refresh_key.with_mut(|value| *value += 1);
                    }
                    Err(err) => {
                        error_message.set(Some(format!("Failed to add demo station: {}", err)));
                    }
                }
            });
        }
    };

    let on_cancel_form = {
        let mut form_mode = form_mode.clone();
        let mut error_message = error_message.clone();
        move |_| {
            form_mode.set(RadioFormMode::Closed);
            error_message.set(None);
        }
    };

    let on_save_form = {
        let servers = servers.clone();
        let mut form_mode = form_mode.clone();
        let form_name = form_name.clone();
        let form_stream_url = form_stream_url.clone();
        let form_home_page_url = form_home_page_url.clone();
        let form_server_id = form_server_id.clone();
        let mut error_message = error_message.clone();
        let mut is_saving = is_saving.clone();
        let mut refresh_key = refresh_key.clone();
        move |_| {
            let name = form_name().trim().to_string();
            let stream_url = form_stream_url().trim().to_string();
            let home_page = form_home_page_url().trim().to_string();
            let mode_snapshot = form_mode();
            let server_id = match &mode_snapshot {
                RadioFormMode::Edit(station) => station.server_id.clone(),
                _ => form_server_id(),
            };

            if name.is_empty() {
                error_message.set(Some("Station name is required.".to_string()));
                return;
            }
            if stream_url.is_empty() {
                error_message.set(Some("Stream URL is required.".to_string()));
                return;
            }
            if server_id.is_empty() {
                error_message.set(Some("Select a server for this station.".to_string()));
                return;
            }

            is_saving.set(true);
            error_message.set(None);

            let servers_snapshot = servers();
            spawn(async move {
                let server = servers_snapshot.into_iter().find(|s| s.id == server_id);
                let Some(server) = server else {
                    error_message.set(Some("Server not found.".to_string()));
                    is_saving.set(false);
                    return;
                };

                let client = NavidromeClient::new(server);
                let home_page_opt = if home_page.is_empty() {
                    None
                } else {
                    Some(home_page)
                };

                let result = match mode_snapshot {
                    RadioFormMode::Add => {
                        client
                            .create_internet_radio_station(
                                &name,
                                &stream_url,
                                home_page_opt.as_deref(),
                            )
                            .await
                    }
                    RadioFormMode::Edit(station) => {
                        client
                            .update_internet_radio_station(
                                &station.id,
                                &name,
                                &stream_url,
                                home_page_opt.as_deref(),
                            )
                            .await
                    }
                    RadioFormMode::Closed => Ok(()),
                };

                match result {
                    Ok(_) => {
                        form_mode.set(RadioFormMode::Closed);
                        refresh_key.with_mut(|value| *value += 1);
                    }
                    Err(err) => {
                        error_message.set(Some(err));
                    }
                }

                is_saving.set(false);
            });
        }
    };

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header page-header--split",
                div { class: "space-y-1",
                    h1 { class: "page-title", "Radio Stations" }
                    p { class: "page-subtitle", "Internet radio from your servers" }
                }
                button {
                    class: "inline-flex items-center gap-2 rounded-full bg-emerald-500/15 px-4 py-2 text-sm font-semibold text-emerald-200 hover:bg-emerald-500/25 transition-colors disabled:opacity-40 disabled:cursor-not-allowed",
                    onclick: on_open_add,
                    disabled: !has_active_servers,
                    Icon {
                        name: "plus".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                    "Add station"
                }
            }

            if let Some(message) = error_message() {
                div { class: "rounded-xl border border-rose-500/30 bg-rose-500/10 px-4 py-3 text-sm text-rose-200",
                    "{message}"
                }
            }

            if !has_active_servers {
                div { class: "rounded-xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-200",
                    "Connect an active server to manage radio stations."
                }
            }

            if is_form_open {
                div { class: "rounded-2xl border border-zinc-800/70 bg-zinc-900/60 p-5 space-y-4",
                    div { class: "flex flex-wrap items-center justify-between gap-3",
                        div { class: "space-y-1",
                            h2 { class: "text-lg font-semibold text-white",
                                if is_editing {
                                    "Edit station"
                                } else {
                                    "Add station"
                                }
                            }
                            p { class: "text-xs text-zinc-400",
                                "Stream URL should point to a direct audio stream."
                            }
                        }
                        button {
                            class: "text-xs uppercase tracking-widest text-zinc-400 hover:text-white",
                            onclick: on_cancel_form,
                            "Cancel"
                        }
                    }

                    div { class: "grid gap-4 md:grid-cols-2",
                        div { class: "space-y-2",
                            label { class: "text-xs uppercase tracking-widest text-zinc-500",
                                "Name"
                            }
                            input {
                                class: "w-full rounded-xl border border-zinc-800/80 bg-zinc-950/70 px-4 py-3 text-sm text-white placeholder:text-zinc-600 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                                placeholder: "Station name",
                                value: form_name,
                                oninput: move |e| form_name.set(e.value()),
                            }
                        }
                        div { class: "space-y-2",
                            label { class: "text-xs uppercase tracking-widest text-zinc-500",
                                "Stream URL"
                            }
                            input {
                                class: "w-full rounded-xl border border-zinc-800/80 bg-zinc-950/70 px-4 py-3 text-sm text-white placeholder:text-zinc-600 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                                placeholder: "https://stream.example.com/radio.mp3",
                                value: form_stream_url,
                                oninput: move |e| form_stream_url.set(e.value()),
                            }
                        }
                        div { class: "space-y-2",
                            label { class: "text-xs uppercase tracking-widest text-zinc-500",
                                "Homepage (optional)"
                            }
                            input {
                                class: "w-full rounded-xl border border-zinc-800/80 bg-zinc-950/70 px-4 py-3 text-sm text-white placeholder:text-zinc-600 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                                placeholder: "https://station.example.com",
                                value: form_home_page_url,
                                oninput: move |e| form_home_page_url.set(e.value()),
                            }
                        }
                        div { class: "space-y-2",
                            label { class: "text-xs uppercase tracking-widest text-zinc-500",
                                "Server"
                            }
                            if has_active_servers {
                                select {
                                    class: "w-full rounded-xl border border-zinc-800/80 bg-zinc-950/70 px-4 py-3 text-sm text-white focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20 disabled:text-zinc-500",
                                    value: form_server_id,
                                    disabled: is_editing,
                                    onchange: move |e| form_server_id.set(e.value()),
                                    for server in active_servers.iter().cloned() {
                                        option { value: "{server.id}", "{server.name}" }
                                    }
                                }
                            } else {
                                div { class: "rounded-xl border border-zinc-800/80 bg-zinc-950/70 px-4 py-3 text-sm text-zinc-500",
                                    "{selected_server_name}"
                                }
                            }
                        }
                    }

                    div { class: "flex flex-wrap items-center gap-3",
                        button {
                            class: "inline-flex items-center justify-center rounded-full bg-emerald-500 px-5 py-2 text-sm font-semibold text-black shadow-lg shadow-emerald-500/20 hover:bg-emerald-400 transition-colors disabled:opacity-60 disabled:cursor-not-allowed",
                            onclick: on_save_form,
                            disabled: is_saving(),
                            if is_saving() {
                                "Saving..."
                            } else {
                                "Save station"
                            }
                        }
                        button {
                            class: "inline-flex items-center justify-center rounded-full border border-zinc-700/70 px-5 py-2 text-sm font-semibold text-zinc-200 hover:bg-zinc-800/60 transition-colors",
                            onclick: on_cancel_form,
                            "Cancel"
                        }
                    }
                }
            }

            {
                match stations() {
                    Some(stations) if !stations.is_empty() => rsx! {
                        div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4",
                            for station in stations {
                                RadioStationCard {
                                    station: station.clone(),
                                    on_play: {
                                        let station = station.clone();
                                        move |_| {
                                            let radio_song = Song {
                                                id: station.id.clone(),
                                                title: station.name.clone(),
                                                artist: Some("Internet Radio".to_string()),
                                                album: None,
                                                album_id: None,
                                                artist_id: None,
                                                duration: 0,
                                                track: None,
                                                cover_art: None,
                                                content_type: Some("audio/mpeg".to_string()),
                                                stream_url: Some(station.stream_url.clone()),
                                                suffix: None,
                                                bitrate: None,
                                                starred: None,
                                                user_rating: None,
                                                year: None,
                                                genre: None,
                                                server_id: station.server_id.clone(),
                                                server_name: "Radio".to_string(),
                                            };
                                            now_playing.set(Some(radio_song));
                                            is_playing.set(true);
                                        }
                                    },
                                    on_edit: {
                                        let station = station.clone();
                                        let mut form_mode = form_mode.clone();
                                        let mut form_name = form_name.clone();
                                        let mut form_stream_url = form_stream_url.clone();
                                        let mut form_home_page_url = form_home_page_url.clone();
                                        let mut form_server_id = form_server_id.clone();
                                        let mut error_message = error_message.clone();
                                        move |_| {
                                            form_name.set(station.name.clone());
                                            form_stream_url.set(station.stream_url.clone());
                                            form_home_page_url.set(station.home_page_url.clone().unwrap_or_default());
                                            form_server_id.set(station.server_id.clone());
                                            error_message.set(None);
                                            form_mode.set(RadioFormMode::Edit(station.clone()));
                                        }
                                    },
                                    on_delete: {
                                        let station_id = station.id.clone();
                                        let station_server_id = station.server_id.clone();
                                        let servers = servers.clone();
                                        let mut refresh_key = refresh_key.clone();
                                        let mut error_message = error_message.clone();
                                        move |_| {
                                            let station_id = station_id.clone();
                                            let station_server_id = station_server_id.clone();
                                            let servers_snapshot = servers();
                                            spawn(async move {
                                                let server = servers_snapshot
                                                    .into_iter()
                                                    .find(|s| s.id == station_server_id);
                                                let Some(server) = server else {
                                                    error_message.set(Some("Server not found.".to_string()));
                                                    return;
                                                };
                                                let client = NavidromeClient::new(server);
                                                match client.delete_internet_radio_station(&station_id).await {
                                                    Ok(_) => {
                                                        refresh_key.with_mut(|value| *value += 1);
                                                    }
                                                    Err(err) => {
                                                        error_message.set(Some(err));
                                                    }
                                                }
                                            });
                                        }
                                    },
                                }
                            }
                        }
                    },
                    Some(_) => rsx! {
                        div { class: "flex flex-col items-center justify-center py-20",
                            Icon {
                                name: "radio".to_string(),
                                class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                            }
                            h2 { class: "text-xl font-semibold text-white mb-2", "No radio stations" }
                            p { class: "text-zinc-400 mb-6", "Add radio stations in your Navidrome server" }
                            button {
                                class: "inline-flex items-center gap-2 rounded-full bg-emerald-500/15 px-6 py-3 text-sm font-semibold text-emerald-200 hover:bg-emerald-500/25 transition-colors",
                                onclick: on_add_demo_station,
                                Icon { name: "plus".to_string(), class: "w-4 h-4".to_string() }
                                "Add Downtown Hot Radio Demo Station"
                            }
                        }
                    },
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
fn RadioStationCard(
    station: RadioStation,
    on_play: EventHandler<MouseEvent>,
    on_edit: EventHandler<MouseEvent>,
    on_delete: EventHandler<MouseEvent>,
) -> Element {
    let initials: String = station
        .name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_uppercase();

    rsx! {
        div {
            class: "group flex items-center gap-4 p-4 rounded-xl bg-zinc-800/30 border border-zinc-700/30 hover:bg-zinc-800/50 hover:border-emerald-500/30 transition-all",
            onclick: move |e| on_play.call(e),
            // Station icon
            div { class: "w-14 h-14 rounded-xl bg-gradient-to-br from-amber-500 to-orange-600 flex items-center justify-center flex-shrink-0 shadow-lg",
                span { class: "text-white font-bold text-lg", "{initials}" }
            }
            // Station info
            div { class: "flex-1 min-w-0 text-left",
                p { class: "font-medium text-white truncate group-hover:text-emerald-400 transition-colors",
                    "{station.name}"
                }
                p { class: "text-xs text-zinc-400 truncate", "{station.stream_url}" }
            }
            // Actions
            div { class: "flex items-center gap-2",
                button {
                    class: "p-2 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800/70 transition-colors",
                    aria_label: "Edit station",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_edit.call(e);
                    },
                    Icon {
                        name: "settings".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
                button {
                    class: "p-2 rounded-lg text-zinc-400 hover:text-rose-300 hover:bg-rose-500/10 transition-colors",
                    aria_label: "Delete station",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_delete.call(e);
                    },
                    Icon {
                        name: "trash".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
                div { class: "w-10 h-10 rounded-full bg-zinc-700/50 group-hover:bg-emerald-500 flex items-center justify-center transition-colors",
                    Icon {
                        name: "play".to_string(),
                        class: "w-4 h-4 text-zinc-400 group-hover:text-white ml-0.5".to_string(),
                    }
                }
            }
        }
    }
}
