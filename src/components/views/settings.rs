use crate::api::*;
use crate::components::{Icon, VolumeSignal};
use crate::db::{save_settings, AppSettings};
use dioxus::prelude::*;

#[derive(Clone)]
struct ScanResultEntry {
    server_name: String,
    status: ScanStatus,
}

#[component]
pub fn SettingsView() -> Element {
    let mut servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut app_settings = use_context::<Signal<AppSettings>>();
    let mut volume = use_context::<VolumeSignal>().0;
    let scan_results = use_signal(|| Vec::<ScanResultEntry>::new());
    let scan_busy = use_signal(|| false);

    let mut server_name = use_signal(String::new);
    let mut server_url = use_signal(String::new);
    let mut server_user = use_signal(String::new);
    let mut server_pass = use_signal(String::new);
    let mut is_testing = use_signal(|| false);
    let mut test_result = use_signal(|| None::<Result<(), String>>);
    let mut editing_server = use_signal(|| None::<ServerConfig>);
    let mut is_testing_connection = use_signal(|| false);
    let mut connection_test_result = use_signal(|| None::<Result<(), String>>);
    let mut save_status = use_signal(|| None::<String>);

    let can_add = use_memo(move || {
        !server_name().trim().is_empty()
            && !server_url().trim().is_empty()
            && !server_user().trim().is_empty()
            && !server_pass().trim().is_empty()
            && test_result().is_some_and(|r: Result<(), String>| r.is_ok())
            && editing_server().is_none()
    });

    let on_test = {
        let url = server_url.clone();
        let user = server_user.clone();
        let pass = server_pass.clone();
        move |_| {
            if is_testing() {
                return;
            }
            let url = url().trim().to_string();
            let user = user().trim().to_string();
            let pass = pass().trim().to_string();

            is_testing.set(true);
            test_result.set(None);

            spawn(async move {
                let test_server = ServerConfig::new("Test".to_string(), url, user, pass);
                let client = NavidromeClient::new(test_server);
                let result = client.ping().await;

                test_result.set(Some(result.map(|_| ())));
                is_testing.set(false);
            });
        }
    };

    let mut on_edit_server = {
        let mut server_name = server_name.clone();
        let mut server_url = server_url.clone();
        let mut server_user = server_user.clone();
        let mut server_pass = server_pass.clone();
        move |server: ServerConfig| {
            editing_server.set(Some(server.clone()));
            server_name.set(server.name);
            server_url.set(server.url);
            server_user.set(server.username);
            server_pass.set(server.password);
            test_result.set(None);
        }
    };

    let on_cancel_edit = move |_| {
        editing_server.set(None);
        server_name.set(String::new());
        server_url.set(String::new());
        server_user.set(String::new());
        server_pass.set(String::new());
        test_result.set(None);
    };

    let on_save_edit = move |_| {
        if let Some(editing) = editing_server() {
            let name = server_name().trim().to_string();
            let url = server_url().trim().to_string();
            let user = server_user().trim().to_string();
            let pass = server_pass().trim().to_string();

            if name.is_empty() || url.is_empty() || user.is_empty() || pass.is_empty() {
                return;
            }

            servers.with_mut(|list| {
                if let Some(server) = list.iter_mut().find(|s| s.id == editing.id) {
                    server.name = name;
                    server.url = url;
                    server.username = user;
                    server.password = pass;
                }
            });

            editing_server.set(None);
            server_name.set(String::new());
            server_url.set(String::new());
            server_user.set(String::new());
            server_pass.set(String::new());
            test_result.set(None);

            save_status.set(Some("Server updated!".to_string()));
            #[cfg(target_arch = "wasm32")]
            {
                use gloo_timers::future::TimeoutFuture;
                spawn(async move {
                    TimeoutFuture::new(2000).await;
                    save_status.set(None);
                });
            }
        }
    };

    let on_add = move |_| {
        let name = server_name().trim().to_string();
        let url = server_url().trim().to_string();
        let user = server_user().trim().to_string();
        let pass = server_pass().trim().to_string();

        if name.is_empty() || url.is_empty() || user.is_empty() || pass.is_empty() {
            return;
        }

        let new_server = ServerConfig::new(name, url, user, pass);
        servers.with_mut(|list| list.push(new_server));

        server_name.set(String::new());
        server_url.set(String::new());
        server_user.set(String::new());
        server_pass.set(String::new());
        test_result.set(None);

        save_status.set(Some("Server added!".to_string()));
        #[cfg(target_arch = "wasm32")]
        {
            use gloo_timers::future::TimeoutFuture;
            spawn(async move {
                TimeoutFuture::new(2000).await;
                save_status.set(None);
            });
        }
    };

    let mut on_test_existing = {
        let servers = servers.clone();
        move |server_id: String| {
            if is_testing_connection() {
                return;
            }
            if let Some(server) = servers().iter().find(|s| s.id == server_id).cloned() {
                is_testing_connection.set(true);
                connection_test_result.set(None);

                spawn(async move {
                    let client = NavidromeClient::new(server);
                    let result = client.ping().await;

                    connection_test_result.set(Some(result.map(|_| ())));
                    is_testing_connection.set(false);
                });
            }
        }
    };

    let on_crossfade_toggle = move |_| {
        let mut settings = app_settings();
        settings.crossfade_enabled = !settings.crossfade_enabled;
        let settings_clone = settings.clone();
        app_settings.set(settings);
        spawn(async move {
            let _ = save_settings(settings_clone).await;
        });
    };

    let on_replay_gain_toggle = move |_| {
        let mut settings = app_settings();
        settings.replay_gain = !settings.replay_gain;
        let settings_clone = settings.clone();
        app_settings.set(settings);
        spawn(async move {
            let _ = save_settings(settings_clone).await;
        });
    };

    let on_crossfade_duration_change = move |e: Event<FormData>| {
        if let Ok(duration) = e.value().parse::<u32>() {
            let mut settings = app_settings();
            settings.crossfade_duration = duration;
            let settings_clone = settings.clone();
            app_settings.set(settings);
            spawn(async move {
                let _ = save_settings(settings_clone).await;
            });
        }
    };

    let on_volume_change = move |e: Event<FormData>| {
        if let Ok(vol) = e.value().parse::<f64>() {
            volume.set((vol / 100.0).clamp(0.0, 1.0));
        }
    };

    let on_start_scan = {
        let servers = servers.clone();
        let mut scan_results = scan_results.clone();
        let mut scan_busy = scan_busy.clone();
        move |_| {
            if scan_busy() {
                return;
            }
            scan_busy.set(true);
            spawn(async move {
                let mut results = Vec::new();
                for server in servers().iter().filter(|s| s.active).cloned() {
                    let client = NavidromeClient::new(server.clone());
                    if let Ok(status) = client.start_scan().await {
                        results.push(ScanResultEntry {
                            server_name: server.name.clone(),
                            status,
                        });
                    }
                }
                scan_results.set(results);
                scan_busy.set(false);
            });
        }
    };

    let on_refresh_scan = {
        let servers = servers.clone();
        let mut scan_results = scan_results.clone();
        let mut scan_busy = scan_busy.clone();
        move |_| {
            if scan_busy() {
                return;
            }
            scan_busy.set(true);
            spawn(async move {
                let mut results = Vec::new();
                for server in servers().iter().filter(|s| s.active).cloned() {
                    let client = NavidromeClient::new(server.clone());
                    if let Ok(status) = client.get_scan_status().await {
                        results.push(ScanResultEntry {
                            server_name: server.name.clone(),
                            status,
                        });
                    }
                }
                scan_results.set(results);
                scan_busy.set(false);
            });
        }
    };

    let server_list = servers();
    let settings = app_settings();
    let current_volume = volume();

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header",
                h1 { class: "page-title", "Settings" }
                p { class: "page-subtitle", "Manage your servers and playback preferences" }
            }

            // Save status notification
            if let Some(status) = save_status() {
                div { class: "fixed top-4 right-4 px-4 py-2 bg-emerald-500/20 border border-emerald-500/50 rounded-lg text-emerald-400 text-sm",
                    "{status}"
                }
            }

            // Playback Settings
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-6", "Playback Settings" }

                div { class: "space-y-6",
                    // Volume control
                    div {
                        label { class: "block text-sm font-medium text-zinc-400 mb-3",
                            "Default Volume"
                        }
                        div { class: "flex items-center gap-4",
                            Icon {
                                name: if current_volume > 0.5 { "volume-2".to_string() } else if current_volume > 0.0 { "volume-1".to_string() } else { "volume-x".to_string() },
                                class: "w-5 h-5 text-zinc-400".to_string(),
                            }
                            input {
                                r#type: "range",
                                min: "0",
                                max: "100",
                                value: (current_volume * 100.0).round() as i32,
                                class: "flex-1 h-2 bg-zinc-700 rounded-lg appearance-none cursor-pointer accent-emerald-500",
                                oninput: on_volume_change,
                                onchange: on_volume_change,
                            }
                            span { class: "text-sm text-zinc-400 w-12 text-right",
                                "{(current_volume * 100.0).round() as i32}%"
                            }
                        }
                    }

                    // Crossfade toggle
                    div { class: "flex items-center justify-between",
                        div {
                            p { class: "font-medium text-white", "Crossfade" }
                            p { class: "text-sm text-zinc-400", "Smoothly transition between songs" }
                        }
                        button {
                            class: if settings.crossfade_enabled { "w-12 h-6 bg-emerald-500 rounded-full relative transition-colors" } else { "w-12 h-6 bg-zinc-700 rounded-full relative transition-colors" },
                            onclick: on_crossfade_toggle,
                            div { class: if settings.crossfade_enabled { "w-5 h-5 bg-white rounded-full absolute top-0.5 right-0.5 transition-all" } else { "w-5 h-5 bg-zinc-400 rounded-full absolute top-0.5 left-0.5 transition-all" } }
                        }
                    }

                    // Crossfade duration (show only if crossfade is enabled)
                    if settings.crossfade_enabled {
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2",
                                "Crossfade Duration"
                            }
                            div { class: "flex items-center gap-4",
                                input {
                                    r#type: "range",
                                    min: "1",
                                    max: "12",
                                    value: settings.crossfade_duration,
                                    class: "flex-1 h-2 bg-zinc-700 rounded-lg appearance-none cursor-pointer accent-emerald-500",
                                    oninput: on_crossfade_duration_change,
                                }
                                span { class: "text-sm text-zinc-400 w-16 text-right",
                                    "{settings.crossfade_duration} seconds"
                                }
                            }
                        }
                    }

                    // Replay Gain toggle
                    div { class: "flex items-center justify-between",
                        div {
                            p { class: "font-medium text-white", "Replay Gain" }
                            p { class: "text-sm text-zinc-400", "Normalize volume across tracks" }
                        }
                        button {
                            class: if settings.replay_gain { "w-12 h-6 bg-emerald-500 rounded-full relative transition-colors" } else { "w-12 h-6 bg-zinc-700 rounded-full relative transition-colors" },
                            onclick: on_replay_gain_toggle,
                            div { class: if settings.replay_gain { "w-5 h-5 bg-white rounded-full absolute top-0.5 right-0.5 transition-all" } else { "w-5 h-5 bg-zinc-400 rounded-full absolute top-0.5 left-0.5 transition-all" } }
                        }
                    }
                }
            }

            // Quick Scan Section
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-3", "Quick Scan" }

                div { class: "space-y-4",
                    p { class: "text-sm text-zinc-400",
                        "Trigger a quick scan on your connected servers and keep an eye on the status."
                    }
                    div { class: "flex flex-wrap gap-3",
                        button {
                            class: if scan_busy() { "px-4 py-2 rounded-xl bg-emerald-500/60 text-white cursor-not-allowed flex items-center gap-2" } else { "px-4 py-2 rounded-xl bg-emerald-500 text-white hover:bg-emerald-400 transition-colors flex items-center gap-2" },
                            disabled: scan_busy(),
                            onclick: on_start_scan,
                            if scan_busy() {
                                Icon {
                                    name: "loader".to_string(),
                                    class: "w-4 h-4 text-white animate-spin".to_string(),
                                }
                                "Scanning..."
                            } else {
                                Icon {
                                    name: "search".to_string(),
                                    class: "w-4 h-4 text-white".to_string(),
                                }
                                "Start Quick Scan"
                            }
                        }
                        button {
                            class: if scan_busy() { "px-4 py-2 rounded-xl bg-zinc-700/40 text-zinc-300 cursor-not-allowed flex items-center gap-2" } else { "px-4 py-2 rounded-xl bg-zinc-700/60 text-white hover:bg-zinc-700 transition-colors flex items-center gap-2" },
                            disabled: scan_busy(),
                            onclick: on_refresh_scan,
                            if scan_busy() {
                                Icon {
                                    name: "loader".to_string(),
                                    class: "w-4 h-4 text-white animate-spin".to_string(),
                                }
                                "Refreshing..."
                            } else {
                                Icon {
                                    name: "repeat".to_string(),
                                    class: "w-4 h-4 text-white".to_string(),
                                }
                                "Refresh Status"
                            }
                        }
                    }

                    {
                        if scan_results().is_empty() {
                            rsx! {
                                p { class: "text-sm text-zinc-500", "No scan status available yet." }
                            }
                        } else {
                            rsx! {
                                div { class: "space-y-3",
                                    for entry in scan_results() {
                                        div { class: "p-4 bg-zinc-900/50 border border-zinc-800/70 rounded-2xl space-y-1",
                                            span { class: "text-sm text-zinc-500", "{entry.server_name}" }
                                            p { class: "text-sm text-white", "Status: {entry.status.status}" }
                                            if let Some(task) = entry.status.current_task.as_ref() {
                                                span { class: "text-xs text-zinc-500", "Task: {task}" }
                                            }
                                            if let Some(seconds) = entry.status.seconds_remaining {
                                                span { class: "text-xs text-zinc-500", "{seconds}s remaining" }
                                            }
                                            if let Some(elapsed) = entry.status.seconds_elapsed {
                                                span { class: "text-xs text-zinc-500", "Elapsed: {elapsed}s" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Add/Edit server form
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-4",
                    if editing_server().is_some() {
                        "Edit Server"
                    } else {
                        "Add Server"
                    }
                }

                div { class: "grid gap-4",
                    // Server name
                    div {
                        label { class: "block text-sm font-medium text-zinc-400 mb-2",
                            "Server Name"
                        }
                        input {
                            class: "w-full px-4 py-3 bg-zinc-900/50 border border-zinc-700/50 rounded-xl text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                            placeholder: "My Navidrome Server",
                            value: server_name,
                            oninput: move |e| {
                                server_name.set(e.value());
                                test_result.set(None);
                            },
                        }
                    }

                    // URL
                    div {
                        label { class: "block text-sm font-medium text-zinc-400 mb-2",
                            "Server URL"
                        }
                        input {
                            class: "w-full px-4 py-3 bg-zinc-900/50 border border-zinc-700/50 rounded-xl text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                            placeholder: "https://navidrome.example.com",
                            value: server_url,
                            oninput: move |e| {
                                server_url.set(e.value());
                                test_result.set(None);
                            },
                        }
                    }

                    // Username & Password
                    div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2",
                                "Username"
                            }
                            input {
                                class: "w-full px-4 py-3 bg-zinc-900/50 border border-zinc-700/50 rounded-xl text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                                placeholder: "admin",
                                value: server_user,
                                oninput: move |e| {
                                    server_user.set(e.value());
                                    test_result.set(None);
                                },
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2",
                                "Password"
                            }
                            input {
                                class: "w-full px-4 py-3 bg-zinc-900/50 border border-zinc-700/50 rounded-xl text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                                r#type: "password",
                                placeholder: "••••••••",
                                value: server_pass,
                                oninput: move |e| {
                                    server_pass.set(e.value());
                                    test_result.set(None);
                                },
                            }
                        }
                    }

                    // Test result
                    {
                        match test_result() {
                            Some(Ok(())) => rsx! {
                                div { class: "flex items-center gap-2 text-emerald-400 text-sm",
                                    Icon { name: "check".to_string(), class: "w-4 h-4".to_string() }
                                    "Connection successful!"
                                }
                            },
                            Some(Err(e)) => rsx! {
                                div { class: "flex items-center gap-2 text-red-400 text-sm",
                                    Icon { name: "x".to_string(), class: "w-4 h-4".to_string() }
                                    "Failed: {e}"
                                }
                            },
                            None => rsx! {},
                        }
                    }

                    // Buttons
                    div { class: "flex flex-col sm:flex-row gap-3 pt-2",
                        button {
                            class: "w-full sm:w-auto px-4 py-2 rounded-xl bg-zinc-700/50 text-zinc-300 hover:text-white hover:bg-zinc-700 transition-colors flex items-center gap-2",
                            disabled: is_testing(),
                            onclick: on_test,
                            if is_testing() {
                                Icon {
                                    name: "loader".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                            } else {
                                Icon {
                                    name: "server".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                            }
                            "Test Connection"
                        }
                        if editing_server().is_some() {
                            button {
                                class: "w-full sm:w-auto px-4 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-white font-medium transition-colors flex items-center gap-2",
                                onclick: on_save_edit,
                                Icon {
                                    name: "check".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                                "Save Changes"
                            }
                            button {
                                class: "w-full sm:w-auto px-4 py-2 rounded-xl bg-zinc-700/50 text-zinc-300 hover:text-white hover:bg-zinc-700 transition-colors flex items-center gap-2",
                                onclick: on_cancel_edit,
                                Icon {
                                    name: "x".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                                "Cancel"
                            }
                        } else {
                            button {
                                class: if can_add() { "w-full sm:w-auto px-6 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-white font-medium transition-colors flex items-center gap-2" } else { "w-full sm:w-auto px-6 py-2 rounded-xl bg-zinc-700/50 text-zinc-500 cursor-not-allowed flex items-center gap-2" },
                                disabled: !can_add(),
                                onclick: on_add,
                                Icon {
                                    name: "plus".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                                "Add Server"
                            }
                        }
                    }
                }
            }

            // Connected servers
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-4", "Connected Servers" }

                if server_list.is_empty() {
                    div { class: "flex flex-col items-center justify-center py-12 text-center",
                        Icon {
                            name: "server".to_string(),
                            class: "w-12 h-12 text-zinc-600 mb-4".to_string(),
                        }
                        p { class: "text-zinc-400", "No servers connected yet" }
                        p { class: "text-zinc-500 text-sm",
                            "Add a Navidrome server above to get started"
                        }
                    }
                } else {
                    div { class: "space-y-3",
                        for server in server_list {
                            ServerCard {
                                server: server.clone(),
                                on_toggle: {
                                    let server_id = server.id.clone();
                                    move |_| {
                                        servers
                                            .with_mut(|list| {
                                                if let Some(s) = list.iter_mut().find(|s| s.id == server_id) {
                                                    s.active = !s.active;
                                                }
                                            });
                                    }
                                },
                                on_edit: {
                                    let server = server.clone();
                                    move |_| on_edit_server(server.clone())
                                },
                                on_test: {
                                    let server_id = server.id.clone();
                                    move |_| on_test_existing(server_id.clone())
                                },
                                on_remove: {
                                    let server_id = server.id.clone();
                                    move |_| {
                                        servers
                                            .with_mut(|list| {
                                                list.retain(|s| s.id != server_id);
                                            });
                                    }
                                },
                                is_testing: is_testing_connection(),
                            }
                        }
                    }

                    // Connection test result for existing servers
                    {
                        match connection_test_result() {
                            Some(Ok(())) => rsx! {
                                div { class: "mt-4 flex items-center gap-2 text-emerald-400 text-sm",
                                    Icon { name: "check".to_string(), class: "w-4 h-4".to_string() }
                                    "Connection test successful!"
                                }
                            },
                            Some(Err(e)) => rsx! {
                                div { class: "mt-4 flex items-center gap-2 text-red-400 text-sm",
                                    Icon { name: "x".to_string(), class: "w-4 h-4".to_string() }
                                    "Connection test failed: {e}"
                                }
                            },
                            None => rsx! {},
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ServerCard(
    server: ServerConfig,
    on_toggle: EventHandler<MouseEvent>,
    on_remove: EventHandler<MouseEvent>,
    on_edit: EventHandler<MouseEvent>,
    on_test: EventHandler<MouseEvent>,
    is_testing: bool,
) -> Element {
    let initials: String = server
        .name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_uppercase();

    rsx! {
        div { class: "p-4 rounded-xl bg-zinc-900/50 border border-zinc-700/30",
            // Server info row
            div { class: "flex items-center gap-4 mb-3",
                // Icon
                div { class: "w-12 h-12 rounded-xl bg-gradient-to-br from-emerald-600 to-teal-700 flex items-center justify-center text-white font-bold shadow-lg flex-shrink-0",
                    "{initials}"
                }
                // Info
                div { class: "min-w-0 flex-1",
                    p { class: "font-medium text-white truncate", "{server.name}" }
                    p { class: "text-sm text-zinc-400 truncate", "{server.url}" }
                    p { class: "text-xs text-zinc-500", "User: {server.username}" }
                }
            }
            // Action buttons row
            div { class: "flex items-center justify-between gap-2",
                // Status and toggle
                div { class: "flex items-center gap-2",
                    div { class: if server.active { "text-xs text-emerald-400" } else { "text-xs text-zinc-500" },
                        if server.active {
                            "Active"
                        } else {
                            "Inactive"
                        }
                    }
                    button {
                        class: if server.active { "px-3 py-1.5 rounded-lg bg-emerald-500/20 text-emerald-400 text-sm hover:bg-emerald-500/30 transition-colors" } else { "px-3 py-1.5 rounded-lg bg-zinc-700/50 text-zinc-400 text-sm hover:bg-zinc-700 transition-colors" },
                        onclick: move |e| on_toggle.call(e),
                        if server.active {
                            "Disable"
                        } else {
                            "Enable"
                        }
                    }
                }
                // Action buttons
                div { class: "flex items-center gap-1",
                    button {
                        class: "p-2 rounded-lg text-zinc-500 hover:text-blue-400 hover:bg-blue-500/10 transition-colors",
                        disabled: is_testing,
                        onclick: move |e| on_test.call(e),
                        title: "Test Connection",
                        Icon {
                            name: "server".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                    }
                    button {
                        class: "p-2 rounded-lg text-zinc-500 hover:text-amber-400 hover:bg-amber-500/10 transition-colors",
                        onclick: move |e| on_edit.call(e),
                        title: "Edit Server",
                        Icon {
                            name: "settings".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                    }
                    button {
                        class: "p-2 rounded-lg text-zinc-500 hover:text-red-400 hover:bg-red-500/10 transition-colors",
                        onclick: move |e| on_remove.call(e),
                        title: "Remove Server",
                        Icon {
                            name: "trash".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                    }
                }
            }
        }
    }
}
