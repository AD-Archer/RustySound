//! Playback sync with the server, for every platform: reportPlayback (or the
//! `scrobble` fallback, see `scrobble_song`), saving the play queue, the
//! "Continue from iPhone" offer, and "Also playing on Mac" in Now Playing.
//!
//! One loop samples the shared playback signals once a second and feeds the
//! pure trackers in `logic.rs`; every request is spawned, so nothing on the
//! playback path waits for the network.

mod continue_capsule;
mod logic;

pub use continue_capsule::ContinueCapsule;
pub use logic::*;

use crate::api::{
    client_name, device_from_client_name, NavidromeClient, ServerConfig, ServerPlayQueue, Song,
    SyncSupport,
};
use crate::components::{
    effective_rate, IsPlayingSignal, PlaybackPositionSignal, PreviewPlaybackSignal, SeekRequestSignal,
    SongDetailsController,
};
use crate::components::ui::env::sleep_ms;
use crate::db::{load_json_value, save_json_value, AppSettings};
use dioxus::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Mutex;

const MARKS_KEY: &str = "queue_sync_marks";
/// How often the loop samples the player.
const TICK_MS: u64 = 1_000;
/// "Playing elsewhere" polls getNowPlaying at most this often.
const ELSEWHERE_POLL_MS: u64 = 20_000;
/// Coming back to the foreground re-checks the server queue at most this often.
const FOREGROUND_CHECK_GAP_MS: i64 = 30_000;

/// The song that just played to its end, set by the audio controllers.
static FINISHED: Mutex<Option<(String, String)>> = Mutex::new(None);

/// Called by the audio controllers when a song plays to its end, so its
/// report says `stopped` at the full length (that's what scrobbles it).
pub fn note_track_finished(song: &Song) {
    if let Ok(mut finished) = FINISHED.lock() {
        *finished = Some((song.server_id.clone(), song.id.clone()));
    }
}

fn take_finished() -> Option<(String, String)> {
    FINISHED.lock().ok().and_then(|mut finished| finished.take())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Library songs sync; radio and songs without ids don't.
fn can_sync(song: &Song) -> bool {
    song.server_name != "Radio" && !song.id.trim().is_empty() && !song.server_id.trim().is_empty()
}

/// Another device's queue, offered by [`ContinueCapsule`].
#[derive(Debug, Clone, PartialEq)]
pub struct QueueOffer {
    pub server_id: String,
    /// "iPhone", or another client's own name.
    pub device: String,
    pub queue: ServerPlayQueue,
}

impl QueueOffer {
    pub fn title(&self) -> String {
        self.queue.current().map(|s| s.title.clone()).unwrap_or_default()
    }
}

/// App-level sync state, provided by [`use_playback_sync_provider`].
#[derive(Clone, Copy)]
pub struct PlaybackSync {
    /// The queue another device saved, while it's offered.
    pub offer: Signal<Option<QueueOffer>>,
    /// Another player of this user, while Now Playing is open.
    pub elsewhere: Signal<Option<OtherPlayer>>,
    /// Bumped when a server queue is loaded: the next save waits for play.
    restored: Signal<u64>,
    marks: Signal<HashMap<String, QueueMarks>>,
}

/// Provides [`PlaybackSync`]; call once in the app shell.
pub fn use_playback_sync_provider() -> PlaybackSync {
    let offer = use_signal(|| None::<QueueOffer>);
    let elsewhere = use_signal(|| None::<OtherPlayer>);
    let restored = use_signal(|| 0u64);
    let marks = use_signal(|| load_json_value::<HashMap<String, QueueMarks>>(MARKS_KEY).unwrap_or_default());
    use_context_provider(|| PlaybackSync { offer, elsewhere, restored, marks })
}

/// The signals a restore writes.
#[derive(Clone, Copy)]
pub struct RestoreSignals {
    pub queue: Signal<Vec<Song>>,
    pub queue_index: Signal<usize>,
    pub now_playing: Signal<Option<Song>>,
    pub is_playing: Signal<bool>,
    pub playback_position: Signal<f64>,
    pub seek_request: Signal<Option<(String, f64)>>,
}

impl RestoreSignals {
    /// From context; for components under the app shell.
    pub fn from_context() -> Self {
        Self {
            queue: use_context::<Signal<Vec<Song>>>(),
            queue_index: use_context::<Signal<usize>>(),
            now_playing: use_context::<Signal<Option<Song>>>(),
            is_playing: use_context::<IsPlayingSignal>().0,
            playback_position: use_context::<PlaybackPositionSignal>().0,
            seek_request: use_context::<SeekRequestSignal>().0,
        }
    }
}

/// Every active server's saved queue (those that have one).
pub async fn fetch_server_queues(servers: &[ServerConfig]) -> Vec<(ServerConfig, ServerPlayQueue)> {
    let mut found = Vec::new();
    for server in servers.iter().filter(|s| s.active) {
        let client = NavidromeClient::new(server.clone());
        let support = client.sync_support().await;
        if let Ok(Some(queue)) = client.get_play_queue(support).await {
            found.push((server.clone(), queue));
        }
    }
    found
}

fn offer_input(queue: &ServerPlayQueue) -> QueueOfferInput<'_> {
    QueueOfferInput {
        has_entries: !queue.entries.is_empty(),
        changed_ms: queue.changed_ms,
        changed_by: &queue.changed_by,
    }
}

/// This device's own server queue, when it should win over a bookmark
/// saved at `newest_bookmark_ms` (Resume on launch).
pub fn own_queue_for_resume(
    found: &[(ServerConfig, ServerPlayQueue)],
    newest_bookmark_ms: Option<i64>,
) -> Option<ServerPlayQueue> {
    found
        .iter()
        .filter(|(_, q)| prefer_own_server_queue(&offer_input(q), client_name(), newest_bookmark_ms))
        .max_by_key(|(_, q)| q.changed_ms)
        .map(|(_, q)| q.clone())
}

/// Loads a server queue at its song and position, paused (the resume
/// convention: never autoplay).
pub fn restore_server_queue(mut signals: RestoreSignals, queue: &ServerPlayQueue) {
    let Some(song) = queue.current().cloned() else {
        return;
    };
    let position = queue.position_ms as f64 / 1000.0;
    signals.is_playing.set(false);
    signals.queue.set(queue.entries.clone());
    signals.queue_index.set(queue.current_index);
    signals.now_playing.set(Some(song.clone()));
    signals.playback_position.set(position);
    signals.seek_request.set(Some((song.id.clone(), position)));
}

impl PlaybackSync {
    fn update_marks(mut self, server_id: &str, change: impl FnOnce(&mut QueueMarks)) {
        self.marks.with_mut(|marks| change(marks.entry(server_id.to_string()).or_default()));
        let _ = save_json_value(MARKS_KEY, &*self.marks.peek());
    }

    /// Offers the newest queue another device saved, if one should be.
    /// Nothing found (no queue, or the servers didn't answer) leaves the
    /// current offer alone.
    pub fn consider_offers(mut self, found: &[(ServerConfig, ServerPlayQueue)]) {
        if found.is_empty() {
            return;
        }
        let marks = self.marks.peek().clone();
        let best = found
            .iter()
            .filter(|(server, q)| {
                let marks = marks.get(&server.id).copied().unwrap_or_default();
                should_offer_queue(&offer_input(q), client_name(), &marks)
            })
            .max_by_key(|(_, q)| q.changed_ms);
        let next = best.map(|(server, queue)| QueueOffer {
            server_id: server.id.clone(),
            device: device_from_client_name(&queue.changed_by),
            queue: queue.clone(),
        });
        if *self.offer.peek() != next {
            self.offer.set(next);
        }
    }

    /// Takes the offered queue: loads it paused and forgets the offer.
    pub fn accept(mut self, signals: RestoreSignals) {
        let Some(offer) = self.offer.peek().clone() else {
            return;
        };
        self.offer.set(None);
        if let Some(changed) = offer.queue.changed_ms {
            self.update_marks(&offer.server_id, |m| m.handled_changed_ms = Some(changed));
        }
        self.note_restored();
        restore_server_queue(signals, &offer.queue);
    }

    /// Hides the offer until the server's queue changes again.
    pub fn dismiss(mut self) {
        let Some(offer) = self.offer.peek().clone() else {
            return;
        };
        self.offer.set(None);
        if let Some(changed) = offer.queue.changed_ms {
            self.update_marks(&offer.server_id, |m| m.handled_changed_ms = Some(changed));
        }
    }

    /// A server queue was loaded (banner or Resume on launch).
    pub fn note_restored(mut self) {
        self.restored.with_mut(|n| *n += 1);
    }
}

/// The per-tick state the loop and the lifecycle listener share.
#[derive(Default)]
struct Engine {
    tracker: ReportTracker,
    scheduler: SaveScheduler,
    restored_seen: u64,
    was_playing: bool,
    /// Web: the `pagehide` beacon holds a queue to save.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    beacon_armed: bool,
    last_foreground_check: i64,
}

/// What a queue save sends.
#[derive(Clone)]
struct QueueSave {
    server: ServerConfig,
    ids: Vec<String>,
    current_index: usize,
    position_ms: u64,
}

/// The current server's part of the queue (other servers' songs and radio
/// can't go in its queue), with the current song's index in it.
fn plan_queue_save(queue: &[Song], index: usize, current: &Song, position_ms: u64, servers: &[ServerConfig]) -> Option<QueueSave> {
    let server = servers.iter().find(|s| s.id == current.server_id)?.clone();
    let at_index = queue
        .get(index)
        .filter(|s| s.id == current.id && s.server_id == current.server_id)
        .map(|_| index);
    let mut ids = Vec::new();
    let mut current_index = None;
    for (i, song) in queue.iter().enumerate() {
        if song.server_id != current.server_id || !can_sync(song) {
            continue;
        }
        let matches = match at_index {
            Some(at) => i == at,
            None => song.id == current.id,
        };
        if matches && current_index.is_none() {
            current_index = Some(ids.len());
        }
        ids.push(song.id.clone());
    }
    let current_index = match current_index {
        Some(i) => i,
        // Playing a song that isn't in the queue: save it alone.
        None => {
            ids = vec![current.id.clone()];
            0
        }
    };
    Some(QueueSave { server, ids, current_index, position_ms })
}

fn trimmed(save: &QueueSave, support: SyncSupport) -> (Vec<String>, usize) {
    let cap = if support.form_post { QUEUE_SAVE_CAP } else { QUEUE_SAVE_CAP_NO_FORM };
    let (start, end) = trim_queue_window(save.ids.len(), save.current_index, cap);
    (save.ids[start..end].to_vec(), save.current_index - start)
}

fn queue_signature(queue: &[Song], index: usize, current: Option<&Song>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    index.hash(&mut hasher);
    if let Some(song) = current {
        song.server_id.hash(&mut hasher);
        song.id.hash(&mut hasher);
    }
    for song in queue {
        song.server_id.hash(&mut hasher);
        song.id.hash(&mut hasher);
    }
    hasher.finish()
}

thread_local! {
    /// Reports waiting to go out, and whether a task is sending them.
    static OUTBOX: RefCell<(std::collections::VecDeque<(ServerConfig, PlaybackReport)>, bool)> =
        RefCell::new((std::collections::VecDeque::new(), false));
}

/// Queues reports for one sender task, so they reach the server in order
/// (`stopped` before the next song's `starting`, `paused` before `playing`)
/// even when a request is slow.
fn send_reports(servers: &[ServerConfig], reports: Vec<PlaybackReport>) {
    let start = OUTBOX.with(|outbox| {
        let mut outbox = outbox.borrow_mut();
        outbox.0.extend(reports.into_iter().filter_map(|report| {
            let server = servers.iter().find(|s| s.id == report.server_id)?.clone();
            Some((server, report))
        }));
        !std::mem::replace(&mut outbox.1, true)
    });
    if !start {
        return;
    }
    spawn(async move {
        while let Some((server, report)) = OUTBOX.with(|outbox| {
            let mut outbox = outbox.borrow_mut();
            let next = outbox.0.pop_front();
            if next.is_none() {
                outbox.1 = false;
            }
            next
        }) {
            let client = NavidromeClient::new(server);
            if !client.sync_support().await.playback_report {
                // `scrobble_song` reports for servers without the extension.
                continue;
            }
            let _ = client
                .report_playback(&report.song_id, report.state, report.position_ms, report.rate)
                .await;
        }
    });
}

fn send_queue_save(sync: PlaybackSync, save: QueueSave) {
    spawn(async move {
        let client = NavidromeClient::new(save.server.clone());
        let support = client.sync_support().await;
        let (ids, current_index) = trimmed(&save, support);
        if client.save_play_queue(&ids, current_index, save.position_ms, support).await.is_ok() {
            let at = now_ms();
            sync.update_marks(&save.server.id, |m| m.last_save_ms = Some(at));
        }
    });
}

/// Web: arms (or disarms) the `pagehide` beacon that saves the queue when
/// the tab closes. It is armed only while playing: a paused queue was saved
/// on pause, and re-saving an idle tab's queue would overwrite a newer one
/// from another device.
#[cfg(target_arch = "wasm32")]
fn set_beacon(save: Option<(&QueueSave, SyncSupport)>) {
    let payload = save.and_then(|(save, support)| {
        let (ids, current_index) = trimmed(save, support);
        NavidromeClient::new(save.server.clone()).play_queue_beacon(&ids, current_index, support)
    });
    let js = match payload {
        Some((url, body)) => format!(
            "window.__rsQueueBeacon={{url:{},body:{}}};",
            serde_json::Value::String(url),
            serde_json::Value::String(body)
        ),
        None => "window.__rsQueueBeacon=null;".to_string(),
    };
    let _ = document::eval(&js);
}

/// Visibility changes on every platform (browsers, and the desktop and iOS
/// webviews when the window hides or the app goes to the background), and
/// the web's `pagehide` beacon. Never resolves, so `dioxus.send` stays open.
const LIFECYCLE_JS: &str = r#"
if (!window.__rsSyncLifecycle) {
  window.__rsSyncLifecycle = true;
  window.addEventListener("pagehide", () => {
    const beacon = window.__rsQueueBeacon;
    if (!beacon || !navigator.sendBeacon) return;
    const audio = document.getElementById("rustysound-audio");
    const position = audio ? Math.max(0, Math.round(audio.currentTime * 1000)) : 0;
    const body = new URLSearchParams(beacon.body + "&position=" + position);
    try { navigator.sendBeacon(beacon.url, body); } catch (_) {}
  });
}
document.addEventListener("visibilitychange", () => dioxus.send(document.hidden ? "hidden" : "visible"));
await new Promise(() => {});
"#;

/// Runs playback sync. Mounted once by the app shell, next to the audio
/// controller; renders the Continue capsule. `ready`: settings are loaded.
#[component]
pub fn PlaybackSyncHost(ready: Signal<bool>) -> Element {
    let sync = use_context::<PlaybackSync>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let settings = use_context::<Signal<AppSettings>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<IsPlayingSignal>().0;
    let position = use_context::<PlaybackPositionSignal>().0;
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let preview = use_context::<PreviewPlaybackSignal>().0;
    let song_details = use_context::<SongDetailsController>();
    let engine = use_hook(|| Rc::new(RefCell::new(Engine::default())));
    let ready_signal = ready;

    let position_ms = move || (position.peek().max(0.0) * 1000.0).round() as u64;
    // One cheap look at the queue for the scheduler (a hash, no copies).
    let queue_sample = move |now: i64| -> QueueSample {
        let song = now_playing.peek();
        let song = song.as_ref().filter(|s| can_sync(s) && !*preview.peek());
        let index = *queue_index.peek();
        QueueSample {
            signature: queue_signature(&queue.peek(), index, song),
            song: song.map(|s| (s.server_id.clone(), s.id.clone())),
            playing: *is_playing.peek(),
            position_ms: position_ms(),
            now_ms: now.max(0) as u64,
        }
    };
    // What to send, built only when a save is due.
    let plan_save = move || -> Option<QueueSave> {
        let song = now_playing.peek().clone().filter(|s| can_sync(s) && !*preview.peek())?;
        plan_queue_save(&queue.peek(), *queue_index.peek(), &song, position_ms(), &servers.peek())
    };

    // The once-a-second loop: reports and scheduled saves.
    {
        let engine = engine.clone();
        use_future(move || {
            let engine = engine.clone();
            async move {
                loop {
                    sleep_ms(TICK_MS).await;
                    if !*ready_signal.peek() {
                        continue;
                    }
                    let now = now_ms();
                    let (offline, rate_setting, queue_sync) = {
                        let s = settings.peek();
                        (s.offline_mode, s.playback_rate, s.queue_sync_enabled)
                    };
                    if offline {
                        continue;
                    }
                    let playing = *is_playing.peek();
                    let sample = {
                        let song = now_playing.peek();
                        PlaybackSample {
                            song: song.as_ref().filter(|s| can_sync(s) && !*preview.peek()).map(|s| ReportSong {
                                id: s.id.clone(),
                                server_id: s.server_id.clone(),
                                duration_ms: s.duration as u64 * 1000,
                            }),
                            playing,
                            position_ms: position_ms(),
                            rate: effective_rate(rate_setting, song.as_ref()),
                            finished: take_finished(),
                            now_ms: now.max(0) as u64,
                        }
                    };

                    let mut state = engine.borrow_mut();
                    let reports = state.tracker.observe(&sample);
                    if !reports.is_empty() {
                        send_reports(&servers.peek(), reports);
                    }

                    // Starting to play here takes over from an offered queue.
                    if playing && !state.was_playing && sync.offer.peek().is_some() {
                        let mut offer = sync.offer;
                        offer.set(None);
                    }
                    state.was_playing = playing;

                    let restored = *sync.restored.peek();
                    if state.restored_seen != restored {
                        state.restored_seen = restored;
                        state.scheduler.reset_after_restore();
                    }
                    if !queue_sync {
                        #[cfg(target_arch = "wasm32")]
                        if state.beacon_armed {
                            state.beacon_armed = false;
                            set_beacon(None);
                        }
                        continue;
                    }
                    if state.scheduler.observe(&queue_sample(now)) {
                        if let Some(save) = plan_save() {
                            #[cfg(target_arch = "wasm32")]
                            if playing {
                                state.beacon_armed = true;
                                let beacon = save.clone();
                                spawn(async move {
                                    let support = NavidromeClient::new(beacon.server.clone()).sync_support().await;
                                    set_beacon(Some((&beacon, support)));
                                });
                            }
                            send_queue_save(sync, save);
                        }
                    }
                    #[cfg(target_arch = "wasm32")]
                    if !playing && state.beacon_armed {
                        state.beacon_armed = false;
                        set_beacon(None);
                    }
                }
            }
        });
    }

    // Background / foreground: save at once when hidden, re-check the
    // server queue when visible again.
    {
        let engine = engine.clone();
        use_future(move || {
            let engine = engine.clone();
            async move {
                let mut lifecycle = document::eval(LIFECYCLE_JS);
                while let Ok(state) = lifecycle.recv::<String>().await {
                    if !*ready_signal.peek() || settings.peek().offline_mode || !settings.peek().queue_sync_enabled {
                        continue;
                    }
                    let now = now_ms();
                    if state == "hidden" {
                        let force = engine.borrow_mut().scheduler.force(&queue_sample(now));
                        if let Some(save) = force.then(plan_save).flatten() {
                            send_queue_save(sync, save);
                        }
                    } else {
                        {
                            let mut engine = engine.borrow_mut();
                            if now - engine.last_foreground_check < FOREGROUND_CHECK_GAP_MS {
                                continue;
                            }
                            engine.last_foreground_check = now;
                        }
                        let servers_now = servers.peek().clone();
                        spawn(async move {
                            let found = fetch_server_queues(&servers_now).await;
                            sync.consider_offers(&found);
                        });
                    }
                }
            }
        });
    }

    // Desktop: the window closing is the last chance to save.
    #[cfg(feature = "desktop")]
    {
        let engine = engine.clone();
        dioxus::desktop::use_wry_event_handler(move |event, _| {
            use dioxus::desktop::tao::event::{Event, WindowEvent};
            let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event else {
                return;
            };
            if !settings.peek().queue_sync_enabled || settings.peek().offline_mode {
                return;
            }
            let mut engine = engine.borrow_mut();
            let stop = engine.tracker.stop();
            let save = engine.scheduler.force(&queue_sample(now_ms())).then(plan_save).flatten();
            save_before_exit(save, stop, servers.peek().clone());
        });
    }

    // "Also playing on …": poll getNowPlaying while Now Playing is open.
    {
        let mut elsewhere = sync.elsewhere;
        let mut generation = use_signal(|| 0u64);
        use_effect(move || {
            let open = song_details.current().is_open;
            generation.with_mut(|g| *g += 1);
            let ticket = *generation.peek();
            if !open {
                if elsewhere.peek().is_some() {
                    elsewhere.set(None);
                }
                return;
            }
            spawn(async move {
                while *generation.peek() == ticket {
                    let server = {
                        let servers = servers.peek();
                        let current = now_playing.peek().as_ref().map(|s| s.server_id.clone());
                        current
                            .and_then(|id| servers.iter().find(|s| s.id == id).cloned())
                            .or_else(|| servers.iter().find(|s| s.active).cloned())
                    };
                    if let (Some(server), false) = (server, settings.peek().offline_mode) {
                        let username = server.username.clone();
                        if let Ok(entries) = NavidromeClient::new(server).get_now_playing().await {
                            if *generation.peek() != ticket {
                                break;
                            }
                            let next = pick_other_player(&entries, &username, client_name());
                            if *elsewhere.peek() != next {
                                elsewhere.set(next);
                            }
                        }
                    }
                    sleep_ms(ELSEWHERE_POLL_MS).await;
                }
            });
        });
    }

    rsx! {
        ContinueCapsule {}
    }
}

/// Desktop close: send the last save and `stopped` on a short-lived thread,
/// waiting at most 1.5 s so quitting never hangs.
#[cfg(feature = "desktop")]
fn save_before_exit(save: Option<QueueSave>, stop: Option<PlaybackReport>, servers: Vec<ServerConfig>) {
    if save.is_none() && stop.is_none() {
        return;
    }
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
            return;
        };
        runtime.block_on(async move {
            if let Some(save) = save {
                let client = NavidromeClient::new(save.server.clone());
                let support = client.sync_support().await;
                let (ids, current_index) = trimmed(&save, support);
                let _ = client.save_play_queue(&ids, current_index, save.position_ms, support).await;
            }
            if let Some(report) = stop {
                if let Some(server) = servers.into_iter().find(|s| s.id == report.server_id) {
                    let client = NavidromeClient::new(server);
                    if client.sync_support().await.playback_report {
                        let _ = client
                            .report_playback(&report.song_id, report.state, report.position_ms, report.rate)
                            .await;
                    }
                }
            }
        });
        let _ = done_tx.send(());
    });
    let _ = done_rx.recv_timeout(std::time::Duration::from_millis(1_500));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str, server: &str) -> Song {
        Song { id: id.into(), title: id.into(), server_id: server.into(), server_name: "S".into(), ..Default::default() }
    }

    fn servers() -> Vec<ServerConfig> {
        vec![
            ServerConfig { id: "a".into(), name: "A".into(), url: "http://a".into(), username: "u".into(), password: "p".into(), active: true },
            ServerConfig { id: "b".into(), name: "B".into(), url: "http://b".into(), username: "u".into(), password: "p".into(), active: true },
        ]
    }

    #[test]
    fn a_queue_save_keeps_the_current_servers_songs_and_skips_radio() {
        let mut radio = song("r", "a");
        radio.server_name = "Radio".into();
        let queue = vec![song("1", "a"), song("x", "b"), radio, song("2", "a"), song("3", "a")];
        let save = plan_queue_save(&queue, 3, &queue[3], 5_000, &servers()).unwrap();
        assert_eq!(save.server.id, "a");
        assert_eq!(save.ids, vec!["1", "2", "3"]);
        assert_eq!(save.current_index, 1);
        assert_eq!(save.position_ms, 5_000);
    }

    #[test]
    fn a_song_queued_twice_saves_the_playing_copy() {
        let queue = vec![song("1", "a"), song("2", "a"), song("1", "a")];
        let save = plan_queue_save(&queue, 2, &queue[2], 0, &servers()).unwrap();
        assert_eq!(save.current_index, 2);
    }

    #[test]
    fn a_song_outside_the_queue_is_saved_alone() {
        let queue = vec![song("1", "a")];
        let save = plan_queue_save(&queue, 0, &song("9", "a"), 0, &servers()).unwrap();
        assert_eq!(save.ids, vec!["9"]);
        assert_eq!(save.current_index, 0);
    }

    #[test]
    fn long_queues_are_trimmed_around_the_current_song() {
        let ids: Vec<String> = (0..2_000).map(|i| i.to_string()).collect();
        let save = QueueSave { server: servers()[0].clone(), ids, current_index: 1_000, position_ms: 0 };
        let (ids, current) = trimmed(&save, SyncSupport { form_post: true, ..Default::default() });
        assert_eq!(ids.len(), QUEUE_SAVE_CAP);
        assert_eq!(ids[current], "1000");
        let (ids, current) = trimmed(&save, SyncSupport::default());
        assert_eq!(ids.len(), QUEUE_SAVE_CAP_NO_FORM);
        assert_eq!(ids[current], "1000");
    }

    #[test]
    fn the_signature_follows_order_and_the_current_song() {
        let a = vec![song("1", "a"), song("2", "a")];
        let b = vec![song("2", "a"), song("1", "a")];
        assert_ne!(queue_signature(&a, 0, a.first()), queue_signature(&b, 0, b.first()));
        assert_ne!(queue_signature(&a, 0, a.first()), queue_signature(&a, 1, a.get(1)));
        assert_eq!(queue_signature(&a, 0, a.first()), queue_signature(&a.clone(), 0, a.first()));
    }
}
