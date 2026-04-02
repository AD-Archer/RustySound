use crate::api::*;
use crate::components::{AppView, Navigation};
use dioxus::prelude::*;

pub(crate) fn parse_artist_names(artist_text: &str) -> Vec<String> {
    let parsed: Vec<String> = artist_text
        .split([';', '•'])
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(String::from)
        .collect();

    if parsed.is_empty() {
        let fallback = artist_text.trim();
        if fallback.is_empty() {
            Vec::new()
        } else {
            vec![fallback.to_string()]
        }
    } else {
        parsed
    }
}

fn normalize_artist_name_key(artist_name: &str) -> String {
    artist_name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) async fn resolve_artist_id_for_name(
    server: ServerConfig,
    artist_name: String,
) -> Option<String> {
    let query = artist_name.trim().to_string();
    if query.is_empty() {
        eprintln!("[artist-nav.resolve.skip] empty query");
        return None;
    }

    let normalized_target = normalize_artist_name_key(&query);
    if normalized_target.is_empty() {
        eprintln!(
            "[artist-nav.resolve.skip] normalized query empty raw='{}'",
            query
        );
        return None;
    }

    let server_id = server.id.clone();
    eprintln!(
        "[artist-nav.resolve.start] server_id={} query='{}' normalized='{}'",
        server_id, query, normalized_target
    );
    let client = NavidromeClient::new(server);

    if let Ok(results) = client.search(&query, 50, 0, 0).await {
        let artist_count = results.artists.len();
        eprintln!(
            "[artist-nav.resolve.search] server_id={} query='{}' artist_results={}",
            server_id, query, artist_count
        );
        for artist in results.artists {
            let normalized_name = normalize_artist_name_key(&artist.name);
            if normalized_name == normalized_target {
                eprintln!(
                    "[artist-nav.resolve.match.search] server_id={} query='{}' artist='{}' artist_id={}",
                    server_id, query, artist.name, artist.id
                );
                return Some(artist.id);
            }
        }
    } else {
        eprintln!(
            "[artist-nav.resolve.search.err] server_id={} query='{}'",
            server_id, query
        );
    }

    if let Ok(artists) = client.get_artists().await {
        eprintln!(
            "[artist-nav.resolve.fallback.artists] server_id={} query='{}' artist_count={}",
            server_id,
            query,
            artists.len()
        );
        for artist in artists {
            if normalize_artist_name_key(&artist.name) == normalized_target {
                eprintln!(
                    "[artist-nav.resolve.match.fallback] server_id={} query='{}' artist='{}' artist_id={}",
                    server_id, query, artist.name, artist.id
                );
                return Some(artist.id);
            }
        }
    } else {
        eprintln!(
            "[artist-nav.resolve.fallback.err] server_id={} query='{}'",
            server_id, query
        );
    }

    eprintln!(
        "[artist-nav.resolve.miss] server_id={} query='{}'",
        server_id, query
    );
    None
}

#[component]
pub fn ArtistNameLinks(
    artist_text: String,
    server_id: String,
    fallback_artist_id: Option<String>,
    #[props(default = "inline-flex max-w-full min-w-0 items-center gap-1".to_string())]
    container_class: String,
    #[props(default = "inline-flex max-w-fit truncate text-left hover:text-emerald-400 transition-colors".to_string())]
    button_class: String,
    #[props(default = "text-zinc-500".to_string())] separator_class: String,
) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();

    let artist_parts = parse_artist_names(&artist_text);
    if artist_parts.is_empty() {
        return rsx! { span { class: "{container_class}" } };
    }

    let direct_artist_id = if artist_parts.len() == 1 {
        fallback_artist_id
    } else {
        None
    };

    rsx! {
        span { class: "{container_class}",
            for (index , artist_name) in artist_parts.iter().enumerate() {
                button {
                    key: "{index}-{artist_name}",
                    class: "{button_class}",
                    onclick: {
                        let navigation = navigation.clone();
                        let servers = servers.clone();
                        let server_id = server_id.clone();
                        let direct_artist_id = direct_artist_id.clone();
                        let artist_name = artist_name.clone();
                        move |evt: MouseEvent| {
                            evt.stop_propagation();
                            eprintln!(
                                "[artist-nav.link.click] server_id={} artist='{}' direct_id={}",
                                server_id,
                                artist_name,
                                direct_artist_id
                                    .as_deref()
                                    .unwrap_or("<none>")
                            );
                            if let Some(artist_id) = direct_artist_id.clone() {
                                eprintln!(
                                    "[artist-nav.link.direct] server_id={} artist='{}' artist_id={}",
                                    server_id, artist_name, artist_id
                                );
                                navigation.navigate_to(AppView::ArtistDetailView {
                                    artist_id,
                                    server_id: server_id.clone(),
                                });
                                return;
                            }

                            let server = servers().iter().find(|s| s.id == server_id).cloned();
                            let Some(server) = server else {
                                eprintln!(
                                    "[artist-nav.link.missing-server] server_id={} artist='{}'",
                                    server_id, artist_name
                                );
                                return;
                            };

                            let navigation = navigation.clone();
                            let server_id = server_id.clone();
                            let artist_name = artist_name.clone();
                            spawn(async move {
                                if let Some(artist_id) =
                                    resolve_artist_id_for_name(server, artist_name).await
                                {
                                    eprintln!(
                                        "[artist-nav.link.resolved] server_id={} artist_id={}",
                                        server_id, artist_id
                                    );
                                    navigation.navigate_to(AppView::ArtistDetailView {
                                        artist_id,
                                        server_id,
                                    });
                                } else {
                                    eprintln!(
                                        "[artist-nav.link.unresolved] server_id={}",
                                        server_id
                                    );
                                }
                            });
                        }
                    },
                    "{artist_name}"
                }
                if index + 1 < artist_parts.len() {
                    span { class: "{separator_class}", "•" }
                }
            }
        }
    }
}
