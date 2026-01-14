use crate::api::*;
use crate::components::{Icon, Navigation};
use crate::db::AppSettings;
use dioxus::prelude::*;

#[component]
pub fn StatsView() -> Element {
    let _navigation = use_context::<Navigation>();
    let _app_settings = use_context::<Signal<AppSettings>>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();

    rsx! {
        div { class: "space-y-8",
            // Header
            header { class: "page-header",
                h1 { class: "page-title", "Statistics" }
                p { class: "page-subtitle", "App performance and usage statistics" }
            }

            // Server Statistics
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-6 flex items-center gap-2",
                    Icon {
                        name: "server".to_string(),
                        class: "w-5 h-5".to_string(),
                    }
                    "Server Statistics"
                }

                div { class: "space-y-4",
                    {
                        let server_list = servers();
                        let total_servers = server_list.len();
                        let active_servers = server_list.iter().filter(|s| s.active).count();

                        rsx! {
                            div { class: "grid grid-cols-1 md:grid-cols-3 gap-6",
                                div { class: "bg-zinc-900/50 rounded-xl p-4",
                                    div { class: "text-2xl font-bold text-cyan-400", "{total_servers}" }
                                    div { class: "text-sm text-zinc-400", "Total Servers" }
                                }
                                div { class: "bg-zinc-900/50 rounded-xl p-4",
                                    div { class: "text-2xl font-bold text-green-400", "{active_servers}" }
                                    div { class: "text-sm text-zinc-400", "Active Servers" }
                                }
                                div { class: "bg-zinc-900/50 rounded-xl p-4",
                                    div { class: "text-2xl font-bold text-yellow-400", "{total_servers - active_servers}" }
                                    div { class: "text-sm text-zinc-400", "Inactive Servers" }
                                }
                            }

                            // Server list
        
                            div { class: "mt-6 space-y-3",
                                for server in server_list {
                                    div { class: "flex items-center justify-between p-3 bg-zinc-900/30 rounded-lg",
                                        div { class: "flex items-center gap-3",
                                            div { class: if server.active { "w-2 h-2 rounded-full bg-emerald-500" } else { "w-2 h-2 rounded-full bg-zinc-500" } }
                                            div {
                                                p { class: "font-medium text-white", "{server.name}" }
                                                p { class: "text-sm text-zinc-400", "{server.url}" }
                                            }
                                        }
                                        div { class: "text-sm text-zinc-500",
                                            {
                                                let status = if server.active { "Active" } else { "Inactive" };
                                                format!("{}", status)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Performance Statistics
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-6 flex items-center gap-2",
                    Icon { name: "zap".to_string(), class: "w-5 h-5".to_string() }
                    "Performance"
                }

                div { class: "grid grid-cols-1 md:grid-cols-2 gap-6",
                    // Average load time (placeholder)
                    div { class: "bg-zinc-900/50 rounded-xl p-4",
                        div { class: "text-2xl font-bold text-pink-400", "--" }
                        div { class: "text-sm text-zinc-400", "Avg Load Time" }
                        div { class: "text-xs text-zinc-500 mt-1", "Coming soon" }
                    }

                    // Placeholder for future metrics
                    div { class: "bg-zinc-900/50 rounded-xl p-4",
                        div { class: "text-2xl font-bold text-indigo-400", "--" }
                        div { class: "text-sm text-zinc-400", "Request Count" }
                        div { class: "text-xs text-zinc-500 mt-1", "Coming soon" }
                    }
                }
            }
        }
    }
}