//! Changes made while a server can't be reached, kept until it can.
//!
//! Favourites, ratings and playlist edits (create, add, remove, reorder) go
//! through [`submit`] inside the API layer (`MusicClient` for favourites and
//! ratings, `NavidromeClient` for playlists), so callers don't change:
//!
//! * Online, the call goes straight to the server. When the server can't be
//!   reached (a network error or a 5xx), Offline Mode is on, or changes for
//!   that server are already waiting, the change is queued instead and the
//!   call succeeds, so the UI keeps its optimistic state.
//! * The queue belongs to the active profile and is durable
//!   (`db::save_json_value`: SQLite on native, localStorage on web).
//!   [`STATUS`] tells the UI how many changes wait.
//! * [`replay`] sends them in order, server by server, in the background
//!   (the app shell's `offline_sync` host runs it when the server is back,
//!   on a timer while changes wait, and from "Sync now"). A server that is
//!   still unreachable keeps its changes; a change the server refuses (the
//!   item is gone, no permission) is dropped.
//!
//! Conflicts: favourites and ratings are last write wins. A newer change to
//! the same item replaces the queued one, and replay sets it whatever the
//! server has now. Playlist edits replay against the playlist as the server
//! has it then: removes find their songs by id and skip the ones already
//! gone, a reorder keeps songs added elsewhere (after the queued order), and
//! a playlist created offline gets its server id before the edits that
//! follow it are sent.
//!
//! Music on the device is never queued (it's always there), and neither is
//! anything a source can't do (`Capabilities`; Jellyfin has no star ratings,
//! so `MusicClient::set_rating` refuses before it gets here).

use super::{Capabilities, ServerConfig};
use crate::local_music;
use dioxus::prelude::*;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Where the queue is saved, per profile.
const STORE_KEY: &str = "offline_changes";
/// Ids handed out for playlists created offline, until the server names them.
const TEMP_PLAYLIST_PREFIX: &str = "offline-";

/// One change, as the server will be asked to make it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    Star { id: String, item_type: String, starred: bool },
    Rate { id: String, rating: u32 },
    CreatePlaylist { temp_id: String, name: String, comment: Option<String>, song_ids: Vec<String> },
    AddToPlaylist { playlist_id: String, song_ids: Vec<String> },
    /// The songs to take out, by id (one entry per copy).
    RemoveFromPlaylist { playlist_id: String, song_ids: Vec<String> },
    /// The whole playlist in its new order.
    ReorderPlaylist { playlist_id: String, song_ids: Vec<String> },
}

impl Change {
    /// The item a favourite or rating is about: a newer change to it
    /// replaces a queued one.
    fn last_write_key(&self) -> Option<(&'static str, &str, &str)> {
        match self {
            Change::Star { id, item_type, .. } => Some(("star", id, item_type)),
            Change::Rate { id, .. } => Some(("rate", id, "")),
            _ => None,
        }
    }

    fn playlist_id_mut(&mut self) -> Option<&mut String> {
        match self {
            Change::AddToPlaylist { playlist_id, .. }
            | Change::RemoveFromPlaylist { playlist_id, .. }
            | Change::ReorderPlaylist { playlist_id, .. } => Some(playlist_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub seq: u64,
    pub server_id: String,
    pub queued_at_ms: i64,
    pub change: Change,
}

/// The waiting changes, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Queue {
    #[serde(default)]
    next_seq: u64,
    #[serde(default)]
    items: Vec<Pending>,
    /// `server::temp id` of playlists created offline, to the server's id.
    #[serde(default)]
    created: HashMap<String, String>,
    /// The profile this was loaded for.
    #[serde(skip)]
    profile: Option<String>,
}

impl Queue {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[cfg(test)]
    fn items(&self) -> &[Pending] {
        &self.items
    }

    fn has_server(&self, server_id: &str) -> bool {
        self.items.iter().any(|p| p.server_id == server_id)
    }

    /// Queues `change`. A favourite or rating replaces the one queued for
    /// the same item (last write wins), and goes to the back.
    pub fn push(&mut self, server_id: &str, change: Change, now_ms: i64) {
        if let Some(key) = change.last_write_key() {
            self.items
                .retain(|p| !(p.server_id == server_id && p.change.last_write_key() == Some(key)));
        }
        self.next_seq += 1;
        self.items.push(Pending {
            seq: self.next_seq,
            server_id: server_id.to_string(),
            queued_at_ms: now_ms,
            change,
        });
    }

    /// The oldest change for `server_id`.
    fn next_for(&self, server_id: &str) -> Option<Pending> {
        self.items.iter().find(|p| p.server_id == server_id).cloned()
    }

    fn remove(&mut self, seq: u64) {
        self.items.retain(|p| p.seq != seq);
    }

    /// A playlist created offline got its server id: later edits use it.
    fn created(&mut self, server_id: &str, temp_id: &str, real_id: &str) {
        for item in self.items.iter_mut().filter(|p| p.server_id == server_id) {
            if let Some(id) = item.change.playlist_id_mut() {
                if id == temp_id {
                    *id = real_id.to_string();
                }
            }
        }
        self.created.insert(format!("{server_id}::{temp_id}"), real_id.to_string());
    }

    /// The server's id for a playlist that was created offline.
    fn resolve(&self, server_id: &str, playlist_id: &str) -> Option<String> {
        self.created.get(&format!("{server_id}::{playlist_id}")).cloned()
    }

    /// Drops what waits for a server that's no longer configured.
    fn keep_servers(&mut self, server_ids: &[&str]) -> usize {
        let before = self.items.len();
        self.items.retain(|p| server_ids.contains(&p.server_id.as_str()));
        before - self.items.len()
    }
}

/// What the UI shows: changes waiting, and whether they're being sent.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct QueueStatus {
    pub pending: usize,
    pub syncing: bool,
}

pub static STATUS: GlobalSignal<QueueStatus> = Signal::global(QueueStatus::default);

static QUEUE: Lazy<Mutex<Queue>> = Lazy::new(|| Mutex::new(Queue::default()));
static REPLAYING: AtomicBool = AtomicBool::new(false);

/// Runs `f` on the active profile's queue (loading it the first time, and
/// again after a profile switch). Saves and publishes when `f` says it changed.
fn with_queue<T>(f: impl FnOnce(&mut Queue) -> (T, bool)) -> T {
    let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let profile = crate::db::profiles::active_profile_id();
    if queue.profile.as_deref() != Some(profile.as_str()) {
        *queue = crate::db::load_json_value::<Queue>(STORE_KEY).unwrap_or_default();
        queue.profile = Some(profile);
    }
    let (value, changed) = f(&mut queue);
    if changed {
        let _ = crate::db::save_json_value(STORE_KEY, &*queue);
    }
    let pending = queue.len();
    drop(queue);
    publish(|status| status.pending = pending);
    value
}

/// Updates [`STATUS`] when there is a Dioxus runtime to tell (not in tests).
fn publish(change: impl FnOnce(&mut QueueStatus)) {
    if dioxus::core::Runtime::try_current().is_none() {
        return;
    }
    let mut next = *STATUS.peek();
    change(&mut next);
    if next != *STATUS.peek() {
        *STATUS.write() = next;
    }
}

/// Changes waiting in the active profile. Also loads the queue, so
/// [`STATUS`] is right after launch and after a profile switch.
pub fn pending_count() -> usize {
    with_queue(|q| (q.len(), false))
}

/// The server's id for `playlist_id` when it was created offline and has
/// synced since; otherwise `playlist_id` itself.
pub fn resolve_playlist_id(server_id: &str, playlist_id: &str) -> String {
    with_queue(|q| (q.resolve(server_id, playlist_id), false)).unwrap_or_else(|| playlist_id.to_string())
}

/// Whether `playlist_id` was made offline and isn't on the server yet.
pub fn is_temp_playlist(playlist_id: &str) -> bool {
    playlist_id.starts_with(TEMP_PLAYLIST_PREFIX)
}

/// A fresh id for a playlist created offline.
pub fn temp_playlist_id() -> String {
    format!("{TEMP_PLAYLIST_PREFIX}{}", uuid::Uuid::new_v4().simple())
}

/// Whether an error means "couldn't reach the server" (worth retrying),
/// rather than the server saying no.
pub fn is_unreachable(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    let server_error = lower.starts_with('5')
        && lower.len() >= 4
        && lower[..3].bytes().all(|b| b.is_ascii_digit())
        && lower.as_bytes()[3] == b' ';
    server_error
        || [
            "error sending request",
            "failed to fetch",
            "load failed",
            "networkerror",
            "network error",
            "connection",
            "timed out",
            "timeout",
            "dns error",
            "status 5",
            "offline://",
        ]
        .iter()
        .any(|needle| lower.contains(needle))
}

/// Sends a change to `server`, or queues it (see the module docs). `send`
/// is the server call; `queue` builds the change and the value to answer
/// with when it's queued (`None` when it can't be queued, then the call's
/// own error stands).
pub async fn submit<T>(
    server: &ServerConfig,
    send: impl Future<Output = Result<T, String>>,
    queue: impl FnOnce() -> Option<(Change, T)>,
) -> Result<T, String> {
    if local_music::is_local(&server.id) {
        return send.await;
    }
    let waiting = with_queue(|q| (q.has_server(&server.id), false));
    if crate::cache_service::is_offline_mode() || waiting {
        return enqueue(server, queue)
            .ok_or_else(|| "You're offline. Connect to make this change.".to_string());
    }
    match send.await {
        Err(error) if is_unreachable(&error) => enqueue(server, queue).ok_or(error),
        result => result,
    }
}

/// Whether `server` can take `change` at all (`Capabilities`).
fn supported(capabilities: Capabilities, change: &Change) -> bool {
    match change {
        Change::Star { .. } => true,
        Change::Rate { .. } => capabilities.star_ratings,
        _ => capabilities.playlist_edits,
    }
}

fn enqueue<T>(server: &ServerConfig, queue: impl FnOnce() -> Option<(Change, T)>) -> Option<T> {
    let (change, value) = queue()?;
    if !supported(Capabilities::of(server), &change) {
        return None;
    }
    let now = chrono::Utc::now().timestamp_millis();
    with_queue(|q| {
        q.push(&server.id, change, now);
        ((), true)
    });
    Some(value)
}

// ---- Replay ----------------------------------------------------------------

/// The server calls replay needs, so tests can stand in for a server.
pub(crate) trait Sender {
    async fn favorite(&self, id: &str, item_type: &str, starred: bool) -> Result<(), String>;
    async fn rate(&self, id: &str, rating: u32) -> Result<(), String>;
    async fn create_playlist(&self, name: &str, comment: Option<&str>, song_ids: &[String]) -> Result<Option<String>, String>;
    /// The playlist's song ids as the server has them now (not cached).
    async fn playlist_song_ids(&self, playlist_id: &str) -> Result<Vec<String>, String>;
    async fn add_songs(&self, playlist_id: &str, song_ids: &[String]) -> Result<(), String>;
    async fn remove_indices(&self, playlist_id: &str, indices: &[usize]) -> Result<(), String>;
    async fn reorder(&self, playlist_id: &str, song_ids: &[String], existing: usize) -> Result<(), String>;
}

/// How one change went.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Sent,
    /// A playlist created offline now has this server id.
    Created(String),
    /// Couldn't reach the server: keep it and stop for now.
    Unreachable,
    /// The server said no: drop it.
    Refused,
}

/// The positions of `wanted` in `current`, each copy matched once. Songs
/// already gone are skipped.
fn indices_to_remove(current: &[String], wanted: &[String]) -> Vec<usize> {
    let mut taken = vec![false; current.len()];
    let mut out = Vec::new();
    for id in wanted {
        if let Some(i) = (0..current.len()).find(|&i| !taken[i] && &current[i] == id) {
            taken[i] = true;
            out.push(i);
        }
    }
    out
}

/// The order to save: the queued order for songs the server still has,
/// then the songs it gained meanwhile, in its order.
fn merged_order(current: &[String], wanted: &[String]) -> Vec<String> {
    let mut left: Vec<Option<&String>> = current.iter().map(Some).collect();
    let mut out = Vec::with_capacity(current.len());
    for id in wanted {
        if let Some(slot) = left.iter_mut().find(|slot| slot.is_some_and(|s| s == id)) {
            *slot = None;
            out.push(id.clone());
        }
    }
    out.extend(left.into_iter().flatten().cloned());
    out
}

fn outcome(result: Result<(), String>) -> Outcome {
    match result {
        Ok(()) => Outcome::Sent,
        Err(error) if is_unreachable(&error) => Outcome::Unreachable,
        Err(_) => Outcome::Refused,
    }
}

async fn apply<S: Sender>(sender: &S, change: &Change) -> Outcome {
    match change {
        Change::Star { id, item_type, starred } => outcome(sender.favorite(id, item_type, *starred).await),
        Change::Rate { id, rating } => outcome(sender.rate(id, *rating).await),
        Change::CreatePlaylist { name, comment, song_ids, .. } => {
            match sender.create_playlist(name, comment.as_deref(), song_ids).await {
                Ok(Some(real)) => Outcome::Created(real),
                Ok(None) => Outcome::Sent,
                Err(error) => outcome(Err(error)),
            }
        }
        Change::AddToPlaylist { playlist_id, song_ids } => {
            if is_temp_playlist(playlist_id) {
                // Its playlist never made it to the server.
                return Outcome::Refused;
            }
            outcome(sender.add_songs(playlist_id, song_ids).await)
        }
        Change::RemoveFromPlaylist { playlist_id, song_ids } => {
            let current = match sender.playlist_song_ids(playlist_id).await {
                Ok(ids) => ids,
                Err(error) => return outcome(Err(error)),
            };
            let indices = indices_to_remove(&current, song_ids);
            if indices.is_empty() {
                return Outcome::Sent;
            }
            outcome(sender.remove_indices(playlist_id, &indices).await)
        }
        Change::ReorderPlaylist { playlist_id, song_ids } => {
            let current = match sender.playlist_song_ids(playlist_id).await {
                Ok(ids) => ids,
                Err(error) => return outcome(Err(error)),
            };
            let order = merged_order(&current, song_ids);
            if order == current {
                return Outcome::Sent;
            }
            outcome(sender.reorder(playlist_id, &order, current.len()).await)
        }
    }
}

/// What a replay did.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReplayReport {
    pub sent: usize,
    pub dropped: usize,
    /// Servers still unreachable.
    pub waiting_servers: usize,
}

/// Sends one server's changes (`next` hands them out, oldest first) until
/// none are left or the server stops answering. `finish` records each step.
async fn replay_server<S: Sender>(
    sender: &S,
    next: impl Fn() -> Option<Pending>,
    mut finish: impl FnMut(&Pending, &Outcome),
) -> ReplayReport {
    let mut report = ReplayReport::default();
    while let Some(item) = next() {
        let result = apply(sender, &item.change).await;
        finish(&item, &result);
        match result {
            Outcome::Sent | Outcome::Created(_) => report.sent += 1,
            Outcome::Refused => report.dropped += 1,
            Outcome::Unreachable => {
                report.waiting_servers += 1;
                break;
            }
        }
    }
    report
}

/// Applies a step's outcome to the queue.
fn finish_step(queue: &mut Queue, item: &Pending, result: &Outcome) {
    match result {
        Outcome::Unreachable => {}
        Outcome::Created(real) => {
            queue.remove(item.seq);
            if let Change::CreatePlaylist { temp_id, .. } = &item.change {
                queue.created(&item.server_id, temp_id, real);
            }
        }
        Outcome::Sent | Outcome::Refused => queue.remove(item.seq),
    }
}

/// The real server behind replay.
struct ServerSender(ServerConfig);

impl Sender for ServerSender {
    async fn favorite(&self, id: &str, item_type: &str, starred: bool) -> Result<(), String> {
        super::MusicClient::new(self.0.clone()).send_favorite(id, item_type, starred).await
    }
    async fn rate(&self, id: &str, rating: u32) -> Result<(), String> {
        super::MusicClient::new(self.0.clone()).send_rating(id, rating).await
    }
    async fn create_playlist(&self, name: &str, comment: Option<&str>, song_ids: &[String]) -> Result<Option<String>, String> {
        super::NavidromeClient::new(self.0.clone()).create_playlist_now(name, comment, song_ids).await
    }
    async fn playlist_song_ids(&self, playlist_id: &str) -> Result<Vec<String>, String> {
        let client = super::NavidromeClient::new(self.0.clone());
        client.refresh_playlist_cache();
        Ok(client.get_playlist(playlist_id).await?.1.into_iter().map(|s| s.id).collect())
    }
    async fn add_songs(&self, playlist_id: &str, song_ids: &[String]) -> Result<(), String> {
        super::NavidromeClient::new(self.0.clone()).add_songs_to_playlist_now(playlist_id, song_ids).await
    }
    async fn remove_indices(&self, playlist_id: &str, indices: &[usize]) -> Result<(), String> {
        super::NavidromeClient::new(self.0.clone()).remove_songs_from_playlist_now(playlist_id, indices).await
    }
    async fn reorder(&self, playlist_id: &str, song_ids: &[String], existing: usize) -> Result<(), String> {
        super::NavidromeClient::new(self.0.clone()).reorder_playlist_now(playlist_id, song_ids, existing).await
    }
}

/// Sends every waiting change to the servers in `servers` (one replay at a
/// time; a second call while one runs does nothing). Changes for servers
/// that are no longer configured are dropped. Offline Mode sends nothing.
pub async fn replay(servers: &[ServerConfig]) -> ReplayReport {
    if crate::cache_service::is_offline_mode() || REPLAYING.swap(true, Ordering::AcqRel) {
        return ReplayReport::default();
    }
    publish(|status| status.syncing = true);
    let ids: Vec<&str> = servers.iter().map(|s| s.id.as_str()).collect();
    let dropped = with_queue(|q| {
        let dropped = q.keep_servers(&ids);
        (dropped, dropped > 0)
    });
    let mut total = ReplayReport { dropped, ..Default::default() };
    for server in servers {
        if !with_queue(|q| (q.has_server(&server.id), false)) {
            continue;
        }
        let sender = ServerSender(server.clone());
        let report = replay_server(
            &sender,
            || with_queue(|q| (q.next_for(&server.id), false)),
            |item, result| {
                with_queue(|q| {
                    finish_step(q, item, result);
                    ((), !matches!(result, Outcome::Unreachable))
                })
            },
        )
        .await;
        total.sent += report.sent;
        total.dropped += report.dropped;
        total.waiting_servers += report.waiting_servers;
    }
    REPLAYING.store(false, Ordering::Release);
    publish(|status| status.syncing = false);
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn star(id: &str, starred: bool) -> Change {
        Change::Star { id: id.into(), item_type: "song".into(), starred }
    }

    #[test]
    fn favourites_and_ratings_are_last_write_wins_and_keep_order() {
        let mut q = Queue::default();
        q.push("a", star("s1", true), 1);
        q.push("a", Change::Rate { id: "s1".into(), rating: 3 }, 2);
        q.push("a", Change::AddToPlaylist { playlist_id: "p".into(), song_ids: vec!["s9".into()] }, 3);
        q.push("a", star("s1", false), 4);
        q.push("b", star("s1", true), 5);
        let kinds: Vec<(&str, &Change)> = q.items().iter().map(|p| (p.server_id.as_str(), &p.change)).collect();
        assert_eq!(kinds.len(), 4, "the second star of s1 on a replaced the first");
        assert_eq!(kinds[0].1, &Change::Rate { id: "s1".into(), rating: 3 });
        assert!(matches!(kinds[1].1, Change::AddToPlaylist { .. }));
        assert_eq!(kinds[2], ("a", &star("s1", false)));
        assert_eq!(kinds[3], ("b", &star("s1", true)), "another server's item is separate");
        let seqs: Vec<u64> = q.items().iter().map(|p| p.seq).collect();
        assert!(seqs.windows(2).all(|w| w[0] < w[1]), "oldest first");
    }

    #[test]
    fn the_queue_survives_a_save_and_load() {
        let mut q = Queue::default();
        q.push("a", Change::CreatePlaylist { temp_id: "offline-1".into(), name: "Trip".into(), comment: None, song_ids: vec!["x".into()] }, 1);
        q.push("a", Change::RemoveFromPlaylist { playlist_id: "p".into(), song_ids: vec!["y".into()] }, 2);
        q.created("a", "offline-0", "pl-7");
        let json = serde_json::to_string(&q).unwrap();
        let back: Queue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, q);
        assert_eq!(back.resolve("a", "offline-0").as_deref(), Some("pl-7"));
        // An empty or old store loads as an empty queue.
        assert_eq!(serde_json::from_str::<Queue>("{}").unwrap().len(), 0);
    }

    #[test]
    fn unreachable_means_network_or_server_errors() {
        for e in [
            "error sending request for url (http://x/rest/star)",
            "TypeError: Failed to fetch",
            "status 503",
            "503 Service Unavailable",
            "operation timed out",
        ] {
            assert!(is_unreachable(e), "{e}");
        }
        for e in ["Wrong username or password", "Playlist not found", "404 Not Found", "Jellyfin has no star ratings."] {
            assert!(!is_unreachable(e), "{e}");
        }
    }

    #[test]
    fn removes_skip_songs_already_gone_and_match_each_copy_once() {
        let current: Vec<String> = ["a", "b", "a", "c"].map(String::from).to_vec();
        assert_eq!(indices_to_remove(&current, &["a".into(), "a".into(), "z".into()]), vec![0, 2]);
        assert_eq!(indices_to_remove(&current, &["z".into()]), Vec::<usize>::new());
    }

    #[test]
    fn a_reorder_keeps_songs_added_elsewhere() {
        let current: Vec<String> = ["a", "b", "c", "new"].map(String::from).to_vec();
        let wanted: Vec<String> = ["c", "gone", "a", "b"].map(String::from).to_vec();
        assert_eq!(merged_order(&current, &wanted), ["c", "a", "b", "new"]);
    }

    /// A server that records calls; `up` decides whether it answers.
    #[derive(Default)]
    struct FakeServer {
        up: RefCell<bool>,
        calls: RefCell<Vec<String>>,
        playlist: RefCell<Vec<String>>,
    }

    impl FakeServer {
        fn hit(&self, call: String) -> Result<(), String> {
            if !*self.up.borrow() {
                return Err("error sending request".into());
            }
            self.calls.borrow_mut().push(call);
            Ok(())
        }
    }

    impl Sender for FakeServer {
        async fn favorite(&self, id: &str, _t: &str, starred: bool) -> Result<(), String> {
            self.hit(format!("{}:{id}", if starred { "star" } else { "unstar" }))
        }
        async fn rate(&self, id: &str, rating: u32) -> Result<(), String> {
            if id == "missing" {
                return Err("Item not found".into());
            }
            self.hit(format!("rate:{id}:{rating}"))
        }
        async fn create_playlist(&self, name: &str, _c: Option<&str>, ids: &[String]) -> Result<Option<String>, String> {
            self.hit(format!("create:{name}:{}", ids.len()))?;
            Ok(Some("pl-real".into()))
        }
        async fn playlist_song_ids(&self, _id: &str) -> Result<Vec<String>, String> {
            self.hit("get".into())?;
            Ok(self.playlist.borrow().clone())
        }
        async fn add_songs(&self, id: &str, ids: &[String]) -> Result<(), String> {
            self.hit(format!("add:{id}:{}", ids.join(",")))
        }
        async fn remove_indices(&self, id: &str, indices: &[usize]) -> Result<(), String> {
            self.hit(format!("remove:{id}:{indices:?}"))
        }
        async fn reorder(&self, id: &str, ids: &[String], existing: usize) -> Result<(), String> {
            self.hit(format!("reorder:{id}:{}:{existing}", ids.join(",")))
        }
    }

    fn run_replay(server: &FakeServer, queue: &RefCell<Queue>) -> ReplayReport {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(replay_server(
            server,
            || queue.borrow().next_for("a"),
            |item, result| finish_step(&mut queue.borrow_mut(), item, result),
        ))
    }

    #[test]
    fn replay_waits_while_down_then_sends_in_order() {
        let queue = RefCell::new(Queue::default());
        {
            let mut q = queue.borrow_mut();
            q.push("a", star("s1", true), 1);
            q.push("a", Change::CreatePlaylist { temp_id: "offline-1".into(), name: "Trip".into(), comment: None, song_ids: vec![] }, 2);
            q.push("a", Change::AddToPlaylist { playlist_id: "offline-1".into(), song_ids: vec!["s2".into()] }, 3);
            q.push("a", Change::Rate { id: "missing".into(), rating: 4 }, 4);
            q.push("a", Change::RemoveFromPlaylist { playlist_id: "p".into(), song_ids: vec!["gone".into(), "x".into()] }, 5);
        }
        let server = FakeServer::default();
        *server.playlist.borrow_mut() = vec!["w".into(), "x".into()];

        let down = run_replay(&server, &queue);
        assert_eq!(down, ReplayReport { sent: 0, dropped: 0, waiting_servers: 1 });
        assert_eq!(queue.borrow().len(), 5, "nothing is lost while the server is down");

        *server.up.borrow_mut() = true;
        let up = run_replay(&server, &queue);
        assert_eq!(up, ReplayReport { sent: 4, dropped: 1, waiting_servers: 0 });
        assert_eq!(
            *server.calls.borrow(),
            ["star:s1", "create:Trip:0", "add:pl-real:s2", "get", "remove:p:[1]"],
            "in order, the add on the new playlist's real id, the remove matched by id"
        );
        assert_eq!(queue.borrow().len(), 0);
        assert_eq!(queue.borrow().resolve("a", "offline-1").as_deref(), Some("pl-real"));
    }

    #[test]
    fn a_reorder_replays_against_the_current_playlist() {
        let queue = RefCell::new(Queue::default());
        queue.borrow_mut().push("a", Change::ReorderPlaylist { playlist_id: "p".into(), song_ids: vec!["b".into(), "a".into()] }, 1);
        let server = FakeServer::default();
        *server.up.borrow_mut() = true;
        *server.playlist.borrow_mut() = vec!["a".into(), "b".into(), "c".into()];
        run_replay(&server, &queue);
        assert_eq!(*server.calls.borrow(), ["get", "reorder:p:b,a,c:3"]);
    }

    #[test]
    fn only_what_the_source_can_do_is_queued() {
        let rate = Change::Rate { id: "s".into(), rating: 5 };
        let add = Change::AddToPlaylist { playlist_id: "p".into(), song_ids: vec![] };
        assert!(supported(Capabilities::JELLYFIN, &star("s", true)), "Jellyfin has favourites");
        assert!(!supported(Capabilities::JELLYFIN, &rate), "but no star ratings");
        assert!(!supported(Capabilities::JELLYFIN, &add), "and read-only playlists");
        assert!(supported(Capabilities::SUBSONIC, &rate) && supported(Capabilities::SUBSONIC, &add));
    }

    #[test]
    fn dropping_servers_that_are_gone() {
        let mut q = Queue::default();
        q.push("a", star("s", true), 1);
        q.push("gone", star("s", true), 2);
        assert_eq!(q.keep_servers(&["a"]), 1);
        assert!(q.has_server("a") && !q.has_server("gone"));
    }
}
