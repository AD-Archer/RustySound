//! The pure half of remote sync: reconnect backoff, the clock offset, the
//! live position of another device, which device the player shows, when to
//! send `state` and `queue`, and the words for errors. No signals, no
//! network; `mod.rs` and `connection.rs` feed these.

use super::protocol::{Device, ErrorCode, PlaybackState, QueueSnapshot, QueueUpdate, StateSnapshot};
use std::collections::{HashMap, HashSet};

/// First reconnect delay; doubles per failed attempt.
pub const BACKOFF_START_MS: u64 = 1_000;
/// Longest reconnect delay.
pub const BACKOFF_MAX_MS: u64 = 60_000;
/// `state` goes out at least this often while playing.
pub const STATE_HEARTBEAT_MS: u64 = 5_000;
/// A position this far from where it should be is a seek.
const SEEK_SLACK_MS: u64 = 2_000;
/// The server takes this many queue ids at most (PROTOCOL.md limits).
pub const MAX_QUEUE_IDS: usize = 10_000;

// ---- Reconnecting ---------------------------------------------------------

/// The wait before reconnect attempt `attempt` (0 = the first retry): 1 s
/// doubling to 60 s, with "equal jitter" (between half and all of it, from
/// `jitter` in 0..1) so a room of devices doesn't reconnect in step after
/// the server restarts.
pub fn backoff_delay_ms(attempt: u32, jitter: f64) -> u64 {
    let base = BACKOFF_START_MS
        .saturating_mul(1u64 << attempt.min(16))
        .min(BACKOFF_MAX_MS);
    let jitter = jitter.clamp(0.0, 1.0);
    base / 2 + (base as f64 / 2.0 * jitter) as u64
}

/// What to do after a connection ends, from the error code the server sent
/// (or its close code). `None`: the connection dropped, so retry.
pub fn should_retry(code: Option<ErrorCode>) -> bool {
    code.is_none_or(ErrorCode::should_reconnect)
}

// ---- Clocks ---------------------------------------------------------------

/// The server clock relative to this device's, from `time` probes
/// (PROTOCOL.md "Clocks"): the probe with the lowest round trip wins. Until
/// one answers, frames' `ts` give a rougher guess.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Clock {
    /// `server_now ≈ local_now + offset_ms`.
    pub offset_ms: i64,
    best_rtt: Option<u64>,
}

impl Clock {
    /// A `time` reply: `client_at` echoed, `server_at`, received at `t_recv`.
    pub fn observe_probe(&mut self, client_at: u64, server_at: u64, t_recv: u64) {
        let rtt = t_recv.saturating_sub(client_at);
        if self.best_rtt.is_some_and(|best| rtt > best) {
            return;
        }
        self.best_rtt = Some(rtt);
        self.offset_ms = server_at as i64 + (rtt / 2) as i64 - t_recv as i64;
    }

    /// Any frame's `ts`, received at `t_recv`; used only before a probe has
    /// answered (it is off by the one-way latency).
    pub fn observe_frame(&mut self, ts: u64, t_recv: u64) {
        if self.best_rtt.is_none() {
            self.offset_ms = ts as i64 - t_recv as i64;
        }
    }

    /// Forget the probes (a new connection may take another route).
    pub fn reset(&mut self) {
        self.best_rtt = None;
    }

    pub fn server_now(&self, local_now: u64) -> u64 {
        (local_now as i64 + self.offset_ms).max(0) as u64
    }
}

/// When to send the next clock probe: three, a second apart, after joining,
/// then one every five minutes.
pub fn probe_gap_ms(sent: u32) -> u64 {
    if sent < 3 {
        1_000
    } else {
        300_000
    }
}

/// Where another device's playback is now, in milliseconds: its last state
/// moved on by the server clock while playing, never past the song's end.
pub fn live_position_ms(state: &StateSnapshot, clock: &Clock, local_now: u64, duration_ms: Option<u64>) -> u64 {
    let position = state.position_at(clock.server_now(local_now));
    match duration_ms.filter(|d| *d > 0) {
        Some(duration) => position.min(duration),
        None => position,
    }
}

// ---- Which device the player shows -----------------------------------------

/// What the player shows instead of this device's own playback.
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteView {
    /// Another connected device is the active one: "Playing on Mac".
    Active { device: Device, state: Option<StateSnapshot> },
    /// The active device left; the room keeps its last state, frozen.
    /// Shown only while nothing is loaded here: "Last played on iPhone".
    LastPlayed { name: String, state: StateSnapshot },
}

impl RemoteView {
    pub fn state(&self) -> Option<&StateSnapshot> {
        match self {
            Self::Active { state, .. } => state.as_ref(),
            Self::LastPlayed { state, .. } => Some(state),
        }
    }

    /// The device to send commands to; none for a device that left.
    pub fn target(&self) -> Option<&str> {
        match self {
            Self::Active { device, .. } => Some(device.info.id.as_str()),
            Self::LastPlayed { .. } => None,
        }
    }

    pub fn caption(&self) -> String {
        match self {
            Self::Active { device, .. } => format!("Playing on {}", device.info.name),
            Self::LastPlayed { name, .. } => format!("Last played on {name}"),
        }
    }
}

/// Decides what the player shows. `me`: this device's id. `state`: the
/// room's last state. `local_has_song`: something is loaded here.
/// `seen_playing`: devices seen playing since this device joined; a device
/// that was already paused when we joined doesn't take over music loaded
/// here (opening the iPhone shouldn't hide its own song behind a Mac that
/// stopped an hour ago), but one we watched play keeps the player after a
/// pause. `names`: device names seen in this room, so a device that left can
/// still be named.
pub fn remote_view(
    devices: &[Device],
    me: &str,
    state: Option<&StateSnapshot>,
    local_has_song: bool,
    seen_playing: &HashSet<String>,
    names: &HashMap<String, String>,
) -> Option<RemoteView> {
    if let Some(active) = devices.iter().find(|d| d.active) {
        if active.info.id == me || (local_has_song && !seen_playing.contains(&active.info.id)) {
            return None;
        }
        let state = state.filter(|s| s.from == active.info.id).cloned();
        return Some(RemoteView::Active { device: active.clone(), state });
    }
    // Nobody is active. A frozen state from a device that left, with a
    // track, is worth offering, but never over music loaded here.
    let state = state?;
    let left = !devices.iter().any(|d| d.info.id == state.from);
    if local_has_song || !left || state.from == me || state.state.track_id.is_none() {
        return None;
    }
    let name = names.get(&state.from).cloned().unwrap_or_else(|| "another device".to_string());
    Some(RemoteView::LastPlayed { name, state: state.clone() })
}

/// The devices "Play on" lists: the others that can play, in server order.
pub fn play_on_targets(devices: &[Device], me: &str) -> Vec<Device> {
    devices
        .iter()
        .filter(|d| d.info.id != me && d.info.can_play)
        .cloned()
        .collect()
}

// ---- Sending state and queue ---------------------------------------------

/// This device's playback, as `state` will carry it.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalState {
    pub track_id: Option<String>,
    pub index: Option<u32>,
    pub position_ms: u64,
    pub playing: bool,
    pub rate: f64,
    pub volume: f64,
}

impl LocalState {
    pub fn to_wire(&self, queue_rev: u64, sent_at: u64) -> PlaybackState {
        PlaybackState {
            track_id: self.track_id.clone(),
            queue_rev,
            index: self.index,
            position_ms: self.position_ms,
            playing: self.playing,
            rate: if self.rate.is_finite() { self.rate.clamp(0.0625, 4.0) } else { 1.0 },
            volume: if self.volume.is_finite() { self.volume.clamp(0.0, 1.0) } else { 1.0 },
            sent_at,
        }
    }
}

/// Whether this device's `state` and `queue` go out at all: while it plays,
/// or while it is the active device (so its pause is seen). An idle or
/// paused device that isn't active stays quiet: with nobody active, any
/// state would make it the active one.
pub fn speaks_for_room(playing: bool, active_here: bool) -> bool {
    playing || active_here
}

/// Decides when `state` goes out: on any change of track, play/pause, rate
/// or volume, on a seek, and every five seconds while playing.
#[derive(Debug, Default)]
pub struct StateSender {
    last: Option<(LocalState, u64)>,
}

impl StateSender {
    /// Whether to send `state` now; remembers it if so.
    pub fn due(&mut self, state: &LocalState, now: u64) -> bool {
        let send = match &self.last {
            None => true,
            Some((last, at)) => {
                let elapsed = now.saturating_sub(*at);
                let expected = if last.playing {
                    last.position_ms + (elapsed as f64 * last.rate) as u64
                } else {
                    last.position_ms
                };
                last.track_id != state.track_id
                    || last.index != state.index
                    || last.playing != state.playing
                    || (last.rate - state.rate).abs() > 0.001
                    || (last.volume - state.volume).abs() > 0.01
                    || expected.abs_diff(state.position_ms) > SEEK_SLACK_MS
                    || (state.playing && elapsed >= STATE_HEARTBEAT_MS)
            }
        };
        if send {
            self.last = Some((state.clone(), now));
        }
        send
    }

    /// A new connection: the next look sends.
    pub fn reset(&mut self) {
        self.last = None;
    }
}

/// Queue revisions (PROTOCOL.md "Queue revisions"): the room's queue as
/// last seen, and what to send when ours differs.
#[derive(Debug, Default)]
pub struct QueueBook {
    rev: u64,
    ids: Vec<String>,
}

impl QueueBook {
    /// The room's queue arrived (`welcome`, `queue`, or the reply to a stale
    /// revision). Adopting it means the next differing local queue goes out
    /// as `rev + 1`.
    pub fn adopt(&mut self, queue: Option<&QueueSnapshot>) {
        match queue {
            Some(q) => {
                self.rev = q.rev;
                self.ids = q.ids.clone();
            }
            None => {
                self.rev = 0;
                self.ids.clear();
            }
        }
    }

    pub fn rev(&self) -> u64 {
        self.rev
    }

    /// `queue` to send when `ids` differ from the room's, one revision on.
    pub fn plan(&mut self, ids: &[String]) -> Option<QueueUpdate> {
        if ids.is_empty() || ids == self.ids.as_slice() {
            return None;
        }
        self.rev += 1;
        self.ids = ids.to_vec();
        Some(QueueUpdate { rev: self.rev, ids: self.ids.clone() })
    }
}

/// At most [`MAX_QUEUE_IDS`] ids around `index`: (start, end) of the window.
pub fn queue_window(len: usize, index: usize) -> (usize, usize) {
    if len <= MAX_QUEUE_IDS {
        return (0, len);
    }
    let start = index.saturating_sub(MAX_QUEUE_IDS / 2).min(len - MAX_QUEUE_IDS);
    (start, start + MAX_QUEUE_IDS)
}

// ---- Addresses and words ---------------------------------------------------

/// The sync server's base URL as typed, tidied: no trailing slash, and
/// `https://` when no scheme was given. Empty stays empty (sync off).
pub fn normalize_sync_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

/// The WebSocket address for a sync server URL: `https` → `wss`, `http` →
/// `ws`, plus `/ws`.
pub fn socket_url(sync_url: &str) -> Option<String> {
    let base = normalize_sync_url(sync_url);
    if let Some(rest) = base.strip_prefix("https://") {
        Some(format!("wss://{rest}/ws"))
    } else {
        base.strip_prefix("http://").map(|rest| format!("ws://{rest}/ws"))
    }
}

/// The host of a URL, for messages ("music.example.com").
pub fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.split(['/', '?']).next().unwrap_or(rest).to_string()
}

/// Plain words for an error code. `server_url`: the music server.
pub fn error_message(code: ErrorCode, server_url: &str) -> String {
    match code {
        ErrorCode::ServerNotAllowed => {
            format!("This sync server isn't set up for {}.", host_of(server_url))
        }
        ErrorCode::Unauthorized => "The sync server couldn't sign you in. Check your password.".to_string(),
        ErrorCode::UpstreamUnavailable => "The sync server can't reach your music server. Retrying…".to_string(),
        ErrorCode::Replaced => "Connected on another copy of this device.".to_string(),
        ErrorCode::UnknownTarget => "That device just went offline.".to_string(),
        ErrorCode::NothingToTransfer => "Nothing is playing yet.".to_string(),
        ErrorCode::TargetBusy => "That device isn't responding.".to_string(),
        ErrorCode::TargetCannotPlay => "That device can't play right now.".to_string(),
        ErrorCode::RoomFull => "Too many devices are connected (16 at most).".to_string(),
        ErrorCode::UnsupportedVersion => "This sync server needs a newer RustySound.".to_string(),
        ErrorCode::RateLimited => "Too many changes at once. Reconnecting…".to_string(),
        ErrorCode::ShuttingDown => "The sync server is restarting. Reconnecting…".to_string(),
        _ => "The sync server had a problem. Retrying…".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::remote_sync::protocol::DeviceInfo;

    fn device(id: &str, name: &str, active: bool) -> Device {
        Device {
            info: DeviceInfo { id: id.into(), name: name.into(), platform: "mac".into(), can_play: true },
            active,
        }
    }

    fn snapshot(from: &str, playing: bool) -> StateSnapshot {
        StateSnapshot {
            from: from.into(),
            at: 10_000,
            state: LocalState {
                track_id: Some("t1".into()),
                index: Some(0),
                position_ms: 30_000,
                playing,
                rate: 1.0,
                volume: 1.0,
            }
            .to_wire(1, 9_990),
        }
    }

    #[test]
    fn backoff_doubles_from_one_second_to_a_minute_with_jitter() {
        assert_eq!(backoff_delay_ms(0, 1.0), 1_000);
        assert_eq!(backoff_delay_ms(0, 0.0), 500);
        assert_eq!(backoff_delay_ms(1, 1.0), 2_000);
        assert_eq!(backoff_delay_ms(5, 1.0), 32_000);
        assert_eq!(backoff_delay_ms(6, 1.0), 60_000);
        assert_eq!(backoff_delay_ms(40, 1.0), 60_000, "capped, no overflow");
        let mid = backoff_delay_ms(3, 0.5);
        assert!((4_000..=8_000).contains(&mid), "{mid}");
    }

    #[test]
    fn retries_drops_and_temporary_errors_only() {
        assert!(should_retry(None));
        assert!(should_retry(Some(ErrorCode::ShuttingDown)));
        assert!(should_retry(Some(ErrorCode::UpstreamUnavailable)));
        assert!(!should_retry(Some(ErrorCode::Unauthorized)));
        assert!(!should_retry(Some(ErrorCode::Replaced)));
    }

    #[test]
    fn the_clock_keeps_the_fastest_probe() {
        let mut clock = Clock::default();
        clock.observe_frame(5_100, 5_000);
        assert_eq!(clock.offset_ms, 100, "a frame's ts until a probe answers");
        // 40 ms round trip: offset = 10_020 + 20 - 10_040 = 0.
        clock.observe_probe(10_000, 10_020, 10_040);
        assert_eq!(clock.offset_ms, 0);
        // A slower probe doesn't replace it, nor does a frame.
        clock.observe_probe(20_000, 25_000, 20_400);
        clock.observe_frame(99_999, 1);
        assert_eq!(clock.offset_ms, 0);
        // A faster one does: 10 ms trip, server 500 ms ahead.
        clock.observe_probe(30_000, 30_505, 30_010);
        assert_eq!(clock.offset_ms, 500);
        assert_eq!(clock.server_now(40_000), 40_500);
    }

    #[test]
    fn probes_come_quickly_then_rarely() {
        assert_eq!(probe_gap_ms(0), 1_000);
        assert_eq!(probe_gap_ms(2), 1_000);
        assert_eq!(probe_gap_ms(3), 300_000);
    }

    #[test]
    fn live_position_interpolates_with_the_server_clock() {
        let mut clock = Clock::default();
        clock.observe_probe(0, 1_000, 0); // server 1 s ahead
        let playing = snapshot("mac", true);
        // Local 12 s = server 13 s, 3 s after `at`.
        assert_eq!(live_position_ms(&playing, &clock, 12_000, None), 33_000);
        assert_eq!(live_position_ms(&playing, &clock, 12_000, Some(31_000)), 31_000, "never past the end");
        assert_eq!(live_position_ms(&snapshot("mac", false), &clock, 12_000, None), 30_000);
    }

    fn seen(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn another_active_device_takes_the_player() {
        let names = HashMap::new();
        let devices = vec![device("me", "iPhone", false), device("mac", "Mac", true)];
        let view = remote_view(&devices, "me", Some(&snapshot("mac", true)), true, &seen(&["mac"]), &names).unwrap();
        assert_eq!(view.caption(), "Playing on Mac");
        assert_eq!(view.target(), Some("mac"));
        assert!(view.state().is_some());
        // Paused since, it keeps the player: we watched it play.
        assert!(remote_view(&devices, "me", Some(&snapshot("mac", false)), true, &seen(&["mac"]), &names).is_some());
        // A state from someone else isn't shown as the active device's.
        let view = remote_view(&devices, "me", Some(&snapshot("other", true)), true, &seen(&["mac"]), &names).unwrap();
        assert!(view.state().is_none());
    }

    #[test]
    fn a_device_paused_before_we_joined_leaves_our_own_song_alone() {
        let names = HashMap::new();
        let devices = vec![device("me", "iPhone", false), device("mac", "Mac", true)];
        let paused = snapshot("mac", false);
        assert_eq!(remote_view(&devices, "me", Some(&paused), true, &seen(&[]), &names), None);
        // With nothing loaded here it is shown, so it can be resumed.
        assert!(remote_view(&devices, "me", Some(&paused), false, &seen(&[]), &names).is_some());
    }

    #[test]
    fn being_active_or_alone_shows_the_local_player() {
        let names = HashMap::new();
        let devices = vec![device("me", "iPhone", true), device("mac", "Mac", false)];
        assert_eq!(remote_view(&devices, "me", Some(&snapshot("me", true)), true, &seen(&["me"]), &names), None);
        let idle = vec![device("me", "iPhone", false), device("mac", "Mac", false)];
        assert_eq!(remote_view(&idle, "me", None, false, &seen(&[]), &names), None);
        // A paused state from a device still here isn't a takeover.
        assert_eq!(remote_view(&idle, "me", Some(&snapshot("mac", false)), false, &seen(&[]), &names), None);
    }

    #[test]
    fn a_device_that_left_is_offered_only_when_nothing_plays_here() {
        let names = HashMap::from([("ipad".to_string(), "iPad".to_string())]);
        let devices = vec![device("me", "Mac", false)];
        let frozen = snapshot("ipad", false);
        let view = remote_view(&devices, "me", Some(&frozen), false, &seen(&[]), &names).unwrap();
        assert_eq!(view.caption(), "Last played on iPad");
        assert_eq!(view.target(), None);
        assert_eq!(remote_view(&devices, "me", Some(&frozen), true, &seen(&[]), &names), None);
    }

    #[test]
    fn play_on_lists_the_other_devices_that_can_play() {
        let mut web = device("web", "Web", false);
        web.info.can_play = false;
        let devices = vec![device("me", "Mac", true), device("ios", "iPhone", false), web];
        let ids: Vec<String> = play_on_targets(&devices, "me").into_iter().map(|d| d.info.id).collect();
        assert_eq!(ids, ["ios"]);
    }

    fn local(position_ms: u64, playing: bool) -> LocalState {
        LocalState { track_id: Some("t".into()), index: Some(0), position_ms, playing, rate: 1.0, volume: 0.5 }
    }

    #[test]
    fn state_goes_out_on_changes_and_every_five_seconds_while_playing() {
        let mut sender = StateSender::default();
        assert!(sender.due(&local(0, true), 0), "the first look");
        assert!(!sender.due(&local(1_000, true), 1_000), "playing on as expected");
        assert!(sender.due(&local(5_000, true), 5_000), "heartbeat");
        assert!(sender.due(&local(5_100, false), 5_100), "pause");
        assert!(!sender.due(&local(5_100, false), 60_000), "no heartbeat while paused");
        assert!(sender.due(&local(90_000, false), 61_000), "a seek");
        let mut louder = local(90_000, false);
        louder.volume = 0.9;
        assert!(sender.due(&louder, 61_500), "volume");
        let mut next = louder.clone();
        next.track_id = Some("u".into());
        assert!(sender.due(&next, 62_000), "track change");
        sender.reset();
        assert!(sender.due(&next, 62_100), "a new connection sends at once");
    }

    #[test]
    fn only_a_playing_or_active_device_speaks() {
        assert!(speaks_for_room(true, false));
        assert!(speaks_for_room(false, true), "the active device's pause is seen");
        assert!(!speaks_for_room(false, false), "an idle device never takes over");
    }

    #[test]
    fn queue_revisions_go_up_and_follow_the_room() {
        let ids = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let mut book = QueueBook::default();
        assert_eq!(book.plan(&[]), None, "nothing to send");
        let first = book.plan(&ids(&["a", "b"])).unwrap();
        assert_eq!(first.rev, 1);
        assert_eq!(book.plan(&ids(&["a", "b"])), None, "unchanged");
        // Another device's queue arrives with rev 7: ours goes out as 8.
        book.adopt(Some(&QueueSnapshot { from: "mac".into(), at: 0, rev: 7, ids: ids(&["x"]) }));
        assert_eq!(book.rev(), 7);
        assert_eq!(book.plan(&ids(&["x"])), None, "same as the room's");
        assert_eq!(book.plan(&ids(&["x", "y"])).unwrap().rev, 8);
        // Stale: the server sends its queue back, we adopt it and resend.
        book.adopt(Some(&QueueSnapshot { from: "mac".into(), at: 0, rev: 9, ids: ids(&["z"]) }));
        assert_eq!(book.plan(&ids(&["x", "y"])).unwrap().rev, 10);
    }

    #[test]
    fn long_queues_send_a_window_around_the_current_song() {
        assert_eq!(queue_window(10, 3), (0, 10));
        let (start, end) = queue_window(25_000, 20_000);
        assert_eq!(end - start, MAX_QUEUE_IDS);
        assert!((start..end).contains(&20_000));
        assert_eq!(queue_window(25_000, 24_999), (15_000, 25_000));
        assert_eq!(queue_window(25_000, 0), (0, 10_000));
    }

    #[test]
    fn sync_urls_become_socket_urls() {
        assert_eq!(socket_url("https://music.example.com/rustysound-sync/"), Some("wss://music.example.com/rustysound-sync/ws".into()));
        assert_eq!(socket_url("http://127.0.0.1:4790"), Some("ws://127.0.0.1:4790/ws".into()));
        assert_eq!(socket_url("sync.example.com"), Some("wss://sync.example.com/ws".into()));
        assert_eq!(socket_url("  "), None);
        assert_eq!(normalize_sync_url(" "), "");
    }

    #[test]
    fn errors_read_as_plain_words() {
        assert_eq!(
            error_message(ErrorCode::ServerNotAllowed, "https://music.example.com/"),
            "This sync server isn't set up for music.example.com."
        );
        assert!(error_message(ErrorCode::Unauthorized, "").contains("Check your password"));
        assert_eq!(error_message(ErrorCode::UnknownTarget, ""), "That device just went offline.");
        assert_eq!(error_message(ErrorCode::NothingToTransfer, ""), "Nothing is playing yet.");
    }
}
