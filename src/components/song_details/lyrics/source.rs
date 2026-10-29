//! Where Now Playing's lyrics come from: the provider fetch, the LRCLIB
//! synced "upgrade", the manual search candidates, retries and the
//! last-loaded fallback. Created once per sheet (in `panels.rs`) and shared
//! through context, so switching panels never refetches.

use crate::api::{
    fetch_lyrics_with_fallback, normalize_lyrics_provider_order, search_lyrics_candidates,
    LyricsQuery, LyricsResult, LyricsSearchCandidate,
};
use crate::db::AppSettings;
use dioxus::prelude::*;

/// The song identity and the query built from it.
pub(crate) type LyricsBase = Option<(String, LyricsQuery)>;

/// Whether the current song has lyrics (drives the default panel).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LyricsAvailability {
    Loading,
    Available,
    Missing,
}

/// The lyrics-relevant settings, memoised so unrelated settings writes
/// (shuffle, repeat…) don't refetch.
#[derive(Clone, PartialEq)]
struct LyricsConfig {
    providers: Vec<String>,
    timeout_secs: u32,
    unsynced: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct LyricsSource {
    base: Memo<LyricsBase>,
    config: Memo<LyricsConfig>,
    pub override_query: Signal<Option<LyricsQuery>>,
    /// Title typed into the search sheet (None = the song's own title).
    pub search_title: Signal<Option<String>>,
    /// Term the candidate list was searched for.
    pub search_term: Signal<Option<String>>,
    candidate_nonce: Signal<u64>,
    refresh_nonce: Signal<u64>,
    primary: Resource<Result<LyricsResult, String>>,
    upgrade: Resource<Result<Option<LyricsResult>, String>>,
    pub candidates: Resource<Result<Vec<LyricsSearchCandidate>, String>>,
    cached_synced: Signal<Option<(String, LyricsResult)>>,
    last_good: Signal<Option<(String, LyricsResult)>>,
}

async fn fetch_first_available_lyrics(
    query: LyricsQuery,
    provider_order: Vec<String>,
    timeout_seconds: u32,
) -> Result<LyricsResult, String> {
    let providers = normalize_lyrics_provider_order(&provider_order);
    if providers.is_empty() {
        return Err("No lyrics providers configured.".to_string());
    }
    let mut errors = Vec::<String>::new();
    for provider in providers {
        match fetch_lyrics_with_fallback(&query, &[provider.clone()], timeout_seconds).await {
            Ok(lyrics) => return Ok(lyrics),
            Err(error) => errors.push(format!("{provider} failed: {error}")),
        }
    }
    Err(errors.join(" | "))
}

fn has_text(lyrics: &LyricsResult) -> bool {
    !lyrics.synced_lines.is_empty() || !lyrics.plain_lyrics.trim().is_empty()
}

/// Prefers the LRCLIB synced upgrade (or its cached copy) when synced
/// lyrics are on, then the primary result.
fn pick_display_lyrics(
    sync_lyrics: bool,
    primary: Option<Result<LyricsResult, String>>,
    upgrade: Option<Result<Option<LyricsResult>, String>>,
    cached_synced: Option<LyricsResult>,
) -> Option<Result<LyricsResult, String>> {
    if sync_lyrics {
        if let Some(Ok(Some(up))) = upgrade.as_ref() {
            if !up.synced_lines.is_empty() {
                return Some(Ok(up.clone()));
            }
        }
        if let Some(cached) = cached_synced.filter(|c| !c.synced_lines.is_empty()) {
            return Some(Ok(cached));
        }
    }
    match primary {
        Some(Ok(lyrics)) => Some(Ok(lyrics)),
        Some(Err(error)) => match upgrade {
            Some(Ok(Some(up))) => Some(Ok(up)),
            _ => Some(Err(error)),
        },
        None => match upgrade {
            Some(Ok(Some(up))) => Some(Ok(up)),
            _ => None,
        },
    }
}

pub(crate) fn use_lyrics_source(base: Memo<LyricsBase>) -> LyricsSource {
    let settings = use_context::<Signal<AppSettings>>();
    let config = use_memo(move || {
        let s = settings.read();
        LyricsConfig {
            providers: s.lyrics_provider_order.clone(),
            timeout_secs: s.lyrics_request_timeout_secs,
            unsynced: s.lyrics_unsynced_mode,
        }
    });
    let mut override_query = use_signal(|| None::<LyricsQuery>);
    let mut search_title = use_signal(|| None::<String>);
    let mut search_term = use_signal(|| None::<String>);
    let candidate_nonce = use_signal(|| 0u64);
    let mut refresh_nonce = use_signal(|| 0u64);
    let mut auto_retry_for = use_signal(|| None::<String>);
    let mut upgrade_retry_for = use_signal(|| None::<String>);
    let mut cached_synced = use_signal(|| None::<(String, LyricsResult)>);
    let mut last_good = use_signal(|| None::<(String, LyricsResult)>);
    let mut last_key = use_signal(|| None::<String>);

    let primary = use_resource(move || {
        let base = base();
        let config = config();
        let query_override = override_query();
        let _nonce = refresh_nonce();
        async move {
            let Some((_, query)) = base else {
                return Err("No song selected.".to_string());
            };
            let query = query_override.unwrap_or(query);
            fetch_first_available_lyrics(query, config.providers, config.timeout_secs).await
        }
    });

    let upgrade = use_resource(move || {
        let base = base();
        let config = config();
        let query_override = override_query();
        let _nonce = refresh_nonce();
        async move {
            if config.unsynced {
                return Ok(None);
            }
            let Some((_, query)) = base else {
                return Ok(None);
            };
            let query = query_override.unwrap_or(query);
            fetch_lyrics_with_fallback(&query, &["lrclib".to_string()], config.timeout_secs)
                .await
                .map(Some)
        }
    });

    let candidates = use_resource(move || {
        let base = base();
        let config = config();
        let term = search_term();
        let _nonce = candidate_nonce();
        async move {
            let Some((_, mut query)) = base else {
                return Ok(Vec::new());
            };
            let Some(term) = term.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) else {
                return Ok(Vec::new());
            };
            query.title = term;
            search_lyrics_candidates(&query, &config.providers, config.timeout_secs).await
        }
    });

    // New song: drop the manual search and the per-song retry markers.
    use_effect(move || {
        let key = base().map(|(key, _)| key);
        if *last_key.peek() != key {
            last_key.set(key);
            auto_retry_for.set(None);
            upgrade_retry_for.set(None);
            if override_query.peek().is_some() {
                override_query.set(None);
            }
            if search_title.peek().is_some() {
                search_title.set(None);
            }
            if search_term.peek().is_some() {
                search_term.set(None);
            }
        }
    });

    // Retry a failed primary fetch once per song.
    use_effect(move || {
        let failed = matches!(*primary.read(), Some(Err(_)));
        let Some(key) = base.peek().as_ref().map(|(key, _)| key.clone()) else {
            return;
        };
        if failed && auto_retry_for.peek().as_ref() != Some(&key) {
            auto_retry_for.set(Some(key));
            refresh_nonce += 1;
        }
    });

    // Remember the last synced upgrade for this song; retry it once.
    use_effect(move || {
        let Some(key) = base.peek().as_ref().map(|(key, _)| key.clone()) else {
            return;
        };
        match &*upgrade.read() {
            Some(Ok(Some(up))) if !up.synced_lines.is_empty() => {
                let fresh = cached_synced
                    .peek()
                    .as_ref()
                    .map(|(k, cached)| k != &key || cached != up)
                    .unwrap_or(true);
                if fresh {
                    cached_synced.set(Some((key, up.clone())));
                }
            }
            Some(Err(_)) => {
                if !config.peek().unsynced && upgrade_retry_for.peek().as_ref() != Some(&key) {
                    upgrade_retry_for.set(Some(key));
                    refresh_nonce += 1;
                }
            }
            _ => {}
        }
    });

    let source = LyricsSource {
        base,
        config,
        override_query,
        search_title,
        search_term,
        candidate_nonce,
        refresh_nonce,
        primary,
        upgrade,
        candidates,
        cached_synced,
        last_good,
    };

    // Keep the last good result so a failed refresh still shows lyrics.
    use_effect(move || {
        if let (Some(Ok(lyrics)), Some(key)) = (source.display(), source.key()) {
            let fresh = last_good
                .peek()
                .as_ref()
                .map(|(k, l)| k != &key || l != &lyrics)
                .unwrap_or(true);
            if fresh {
                last_good.set(Some((key, lyrics)));
            }
        }
    });

    source
}

impl LyricsSource {
    fn key(&self) -> Option<String> {
        self.base.read().as_ref().map(|(key, _)| key.clone())
    }

    /// Synced lyrics are wanted (Settings → lyrics "unsynced mode" off).
    pub fn sync_enabled(&self) -> bool {
        !self.config.read().unsynced
    }

    /// The latest result (subscribes to the fetches).
    pub fn display(&self) -> Option<Result<LyricsResult, String>> {
        let key = self.key();
        let cached = self
            .cached_synced
            .read()
            .as_ref()
            .filter(|(k, _)| Some(k) == key.as_ref())
            .map(|(_, l)| l.clone());
        pick_display_lyrics(
            self.sync_enabled(),
            self.primary.read().clone(),
            self.upgrade.read().clone(),
            cached,
        )
    }

    /// What to show: the lyrics (latest, or last good when the latest
    /// failed), the latest error if any, and whether a fetch is pending.
    pub fn shown(&self) -> (Option<LyricsResult>, Option<String>) {
        let key = self.key();
        let last_good = || {
            self.last_good
                .read()
                .as_ref()
                .filter(|(k, _)| Some(k) == key.as_ref())
                .map(|(_, l)| l.clone())
        };
        match self.display() {
            Some(Ok(lyrics)) => (Some(lyrics), None),
            Some(Err(error)) => (last_good(), Some(error)),
            None => (last_good(), None),
        }
    }

    pub fn availability(&self) -> LyricsAvailability {
        match self.display() {
            None => LyricsAvailability::Loading,
            Some(Ok(lyrics)) if has_text(&lyrics) => LyricsAvailability::Available,
            Some(_) => LyricsAvailability::Missing,
        }
    }

    pub fn refresh(&mut self) {
        self.refresh_nonce += 1;
    }

    /// Searches candidates for a typed title. Empty clears the search.
    pub fn search(&mut self, title: String) {
        let title = title.trim().to_string();
        if title.is_empty() {
            self.clear_search();
            return;
        }
        self.search_title.set(Some(title.clone()));
        self.override_query.set(None);
        self.search_term.set(Some(title));
        self.candidate_nonce += 1;
    }

    /// Uses a picked candidate for this song.
    pub fn pick(&mut self, query: LyricsQuery) {
        self.search_title.set(Some(query.title.clone()));
        self.override_query.set(Some(query));
        self.refresh_nonce += 1;
    }

    /// Back to the song's own title and the automatic match.
    pub fn clear_search(&mut self) {
        self.search_title.set(None);
        self.override_query.set(None);
        self.search_term.set(None);
        self.candidate_nonce += 1;
    }
}
