//! The palette's library search: Search's own query path
//! (`views::search_servers`, which scores with `search/scoring.rs`) plus the
//! servers' playlists, matched by name. Answers are cached per query for the
//! session, so typing back over a query shows its results at once; the
//! palette debounces and drops stale answers.

use super::fuzzy::fuzzy;
use crate::api::{Album, Artist, MusicClient, Playlist, ServerConfig, Song};
use crate::components::views::search_servers;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// Rows kept per kind (the palette is a launcher, not a results page).
pub const ALBUMS: usize = 4;
pub const ARTISTS: usize = 3;
pub const SONGS: usize = 5;
pub const PLAYLISTS: usize = 3;
/// Queries remembered.
const CACHE_SIZE: usize = 24;

/// What the library had for a query, already ranked by Search's scoring.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LibraryHits {
    pub query: String,
    pub albums: Vec<Album>,
    pub artists: Vec<Artist>,
    pub songs: Vec<Song>,
    pub playlists: Vec<Playlist>,
}

thread_local! {
    static CACHE: RefCell<VecDeque<Rc<LibraryHits>>> = const { RefCell::new(VecDeque::new()) };
    /// Every server's playlists, fetched once per session (they're small).
    static PLAYLISTS_CACHE: RefCell<Option<Rc<Vec<Playlist>>>> = const { RefCell::new(None) };
}

fn cache_key(query: &str) -> String {
    query.trim().to_lowercase()
}

/// The cached answer for `query`, if this session asked it before.
pub fn cached(query: &str) -> Option<Rc<LibraryHits>> {
    let key = cache_key(query);
    CACHE.with(|cache| cache.borrow().iter().find(|hits| hits.query == key).cloned())
}

fn store(hits: Rc<LibraryHits>) {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.retain(|old| old.query != hits.query);
        cache.push_front(hits);
        cache.truncate(CACHE_SIZE);
    });
}

/// Forgets cached answers (a server was added or switched off).
pub fn clear_cache() {
    CACHE.with(|cache| cache.borrow_mut().clear());
    PLAYLISTS_CACHE.with(|cache| *cache.borrow_mut() = None);
}

async fn all_playlists(servers: &[ServerConfig]) -> Rc<Vec<Playlist>> {
    if let Some(list) = PLAYLISTS_CACHE.with(|cache| cache.borrow().clone()) {
        return list;
    }
    let mut list = Vec::new();
    for client in MusicClient::sources(servers) {
        if let Ok(found) = client.get_playlists().await {
            list.extend(found);
        }
    }
    let list = Rc::new(list);
    PLAYLISTS_CACHE.with(|cache| *cache.borrow_mut() = Some(list.clone()));
    list
}

/// Playlists whose name matches, best first.
fn match_playlists(playlists: &[Playlist], query: &str) -> Vec<Playlist> {
    let mut scored: Vec<(i32, &Playlist)> = playlists
        .iter()
        .filter_map(|p| fuzzy(query, &p.name).map(|m| (m.score, p)))
        .filter(|(score, _)| *score >= 40)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().take(PLAYLISTS).map(|(_, p)| p.clone()).collect()
}

/// Searches the active servers for `query` (or answers from the cache).
pub async fn search(servers: Vec<ServerConfig>, query: String) -> Rc<LibraryHits> {
    if let Some(hit) = cached(&query) {
        return hit;
    }
    let active: Vec<ServerConfig> = servers.into_iter().filter(|s| s.active).collect();
    let found = search_servers(active.clone(), query.clone()).await;
    let playlists = all_playlists(&active).await;
    let hits = Rc::new(LibraryHits {
        query: cache_key(&query),
        albums: found.albums.into_iter().take(ALBUMS).collect(),
        artists: found.artists.into_iter().take(ARTISTS).collect(),
        songs: found.songs.into_iter().take(SONGS).collect(),
        playlists: match_playlists(&playlists, &query),
    });
    store(hits.clone());
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_answers_the_same_query_in_any_case() {
        clear_cache();
        assert!(cached("Paper").is_none());
        store(Rc::new(LibraryHits { query: cache_key(" Paper "), ..Default::default() }));
        assert!(cached("paper").is_some());
        assert!(cached("PAPER ").is_some());
        assert!(cached("pape").is_none());
        for i in 0..CACHE_SIZE + 4 {
            store(Rc::new(LibraryHits { query: format!("q{i}"), ..Default::default() }));
        }
        assert!(cached("paper").is_none(), "oldest falls out");
    }

    #[test]
    fn playlists_match_by_name() {
        let list = vec![
            Playlist { id: "1".into(), name: "Morning Run".into(), ..Default::default() },
            Playlist { id: "2".into(), name: "Late Night Jazz".into(), ..Default::default() },
        ];
        let found = match_playlists(&list, "jazz");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "2");
        assert!(match_playlists(&list, "zzz").is_empty());
    }
}
