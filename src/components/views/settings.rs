use dioxus::prelude::*;
use crate::api::*;
use crate::components::Icon;

#[component]
pub fn SettingsView() -> Element {
    let mut servers = use_context::<Signal<Vec<ServerConfig>>>();
    
    let mut server_name = use_signal(String::new);
    let mut server_url = use_signal(String::new);
    let mut server_user = use_signal(String::new);
    let mut server_pass = use_signal(String::new);
    let mut is_testing = use_signal(|| false);
    let mut test_result = use_signal(|| None::<Result<(), String>>);
    
    let can_add = use_memo(move || {
        !server_name().trim().is_empty() 
            && !server_url().trim().is_empty()
            && !server_user().trim().is_empty()
            && !server_pass().trim().is_empty()
    });
    
    let on_test = {
        let url = server_url.clone();
        let user = server_user.clone();
        let pass = server_pass.clone();
        move |_| {
            let url = url().trim().to_string();
            let user = user().trim().to_string();
            let pass = pass().trim().to_string();
            
            is_testing.set(true);
            test_result.set(None);
            
            spawn(async move {
                let test_server = ServerConfig::new(
                    "Test".to_string(),
                    url,
                    user,
                    pass,
                );
                let client = NavidromeClient::new(test_server);
                let result = client.ping().await;
                
                test_result.set(Some(result.map(|_| ())));
                is_testing.set(false);
            });
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
        
        // Clear form
        server_name.set(String::new());
        server_url.set(String::new());
        server_user.set(String::new());
        server_pass.set(String::new());
        test_result.set(None);
    };
    
    let server_list = servers();
    
    rsx! {
        div { class: "max-w-3xl space-y-8",
            header { class: "mb-8",
                h1 { class: "text-3xl font-bold text-white mb-2", "Settings" }
                p { class: "text-zinc-400", "Manage your Navidrome server connections" }
            }

            // Add server form
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-4", "Add Server" }

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
                            oninput: move |e| server_name.set(e.value()),
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
                            oninput: move |e| server_url.set(e.value()),
                        }
                    }

                    // Username & Password
                    div { class: "grid grid-cols-2 gap-4",
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2",
                                "Username"
                            }
                            input {
                                class: "w-full px-4 py-3 bg-zinc-900/50 border border-zinc-700/50 rounded-xl text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                                placeholder: "admin",
                                value: server_user,
                                oninput: move |e| server_user.set(e.value()),
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
                                oninput: move |e| server_pass.set(e.value()),
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
                    div { class: "flex gap-3 pt-2",
                        button {
                            class: "px-4 py-2 rounded-xl bg-zinc-700/50 text-zinc-300 hover:text-white hover:bg-zinc-700 transition-colors flex items-center gap-2",
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
                        button {
                            class: if can_add() { "px-6 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-white font-medium transition-colors flex items-center gap-2" } else { "px-6 py-2 rounded-xl bg-zinc-700/50 text-zinc-500 cursor-not-allowed flex items-center gap-2" },
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
                                on_remove: {
                                    let server_id = server.id.clone();
                                    move |_| {
                                        servers
                                            .with_mut(|list| {
                                                list.retain(|s| s.id != server_id);
                                            });
                                    }
                                },
                            }
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
) -> Element {
    let initials: String = server.name.chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    
    rsx! {
        div { class: "flex items-center justify-between p-4 rounded-xl bg-zinc-900/50 border border-zinc-700/30",
            div { class: "flex items-center gap-4",
                // Icon
                div { class: "w-12 h-12 rounded-xl bg-gradient-to-br from-emerald-600 to-teal-700 flex items-center justify-center text-white font-bold shadow-lg",
                    "{initials}"
                }
                // Info
                div {
                    p { class: "font-medium text-white", "{server.name}" }
                    p { class: "text-sm text-zinc-400", "{server.url}" }
                    p { class: "text-xs text-zinc-500", "User: {server.username}" }
                }
            }
            div { class: "flex items-center gap-3",
                // Status
                div { class: if server.active { "text-xs text-emerald-400" } else { "text-xs text-zinc-500" },
                    if server.active {
                        "Active"
                    } else {
                        "Inactive"
                    }
                }
                // Toggle button
                button {
                    class: if server.active { "px-3 py-1.5 rounded-lg bg-emerald-500/20 text-emerald-400 text-sm hover:bg-emerald-500/30 transition-colors" } else { "px-3 py-1.5 rounded-lg bg-zinc-700/50 text-zinc-400 text-sm hover:bg-zinc-700 transition-colors" },
                    onclick: move |e| on_toggle.call(e),
                    if server.active {
                        "Disable"
                    } else {
                        "Enable"
                    }
                }
                // Remove button
                button {
                    class: "p-2 rounded-lg text-zinc-500 hover:text-red-400 hover:bg-red-500/10 transition-colors",
                    onclick: move |e| on_remove.call(e),
                    Icon {
                        name: "trash".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
            }
        }
    }
}
