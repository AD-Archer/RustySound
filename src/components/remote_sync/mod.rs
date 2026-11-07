//! Remote control and live sync through a rustysound-sync server: this
//! user's devices on one music server share a room, see each other, follow
//! the active device's playback, send each other commands and move playback
//! between them ("Play on Mac"). Off unless the server has a sync URL
//! (Settings › Servers › Sync Server).
//!
//! * `protocol.rs`: the wire types (PROTOCOL.md), checked against its examples.
//! * `logic.rs`: pure rules (backoff, clock, which device the player shows,
//!   when to send), unit-tested.
//! * `transport.rs` / `connection.rs`: the socket and its reconnect loop,
//!   run as this context's coroutine.
//! * `commands.rs`: incoming commands, through `PlayerCtx`.
//! * `media.rs`: the sync server's cover and lyrics cache.
//! * `ui.rs`: the remote mini player and "Play on".
//!
//! Nothing polls while connected: state goes out on player changes (the
//! `playback_sync` sample calls [`RemoteSync::tick`] once a second, and an
//! effect sends play/pause and track changes at once); everything else
//! arrives as socket events.

mod commands;
mod connection;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod live_tests;
pub mod logic;
pub mod media;
pub mod protocol;
mod transport;
mod ui;

pub use ui::{PlayOnChoices, RemotePlayer};

use crate::api::{device_label, NavidromeClient, ServerConfig};
use crate::components::playback_sync::plan_queue_save;
use crate::components::ui::StatusToast;
use crate::components::{effective_rate, PlayerCtx, PreviewPlaybackSignal, SeekRequestSignal};
use crate::db::{load_json_value, save_json_value, AppSettings};
use connection::{Outgoing, Target};
use dioxus::prelude::*;
use futures_util::StreamExt;
use logic::{error_message, play_on_targets, queue_window, remote_view, speaks_for_room, Clock, LocalState, QueueBook, RemoteView, StateSender};
use protocol::{
    Auth, ClientFrame, ClientMessage, CommandRequest, Device, DeviceInfo, ErrorCode, Hello, Op, ServerFrame,
    ServerMessage, StateSnapshot, PROTOCOL_VERSION,
};
use std::collections::{HashMap, HashSet};

const IDENTITY_KEY: &str = "sync_device";
/// Longest device name the server takes.
const MAX_NAME_CHARS: usize = 64;

pub(crate) fn now_ms() -> u64 {
    crate::time::now_ms().max(0.0) as u64
}

/// The connection, as Settings shows it.
#[derive(Clone, Debug, PartialEq)]
pub enum SyncStatus {
    /// No sync server set (or offline mode).
    Off,
    Connecting,
    Connected,
    /// Dropped or refused for now; reconnecting with backoff.
    Retrying(String),
    /// Refused until something changes (password, allow-list, another copy).
    Failed(String),
}

impl SyncStatus {
    /// "Connected · 3 devices", "Connecting…", or the problem.
    pub fn line(&self, devices: usize) -> String {
        match self {
            Self::Off => "Off".to_string(),
            Self::Connecting => "Connecting…".to_string(),
            Self::Connected if devices == 1 => "Connected · this device only".to_string(),
            Self::Connected => format!("Connected · {devices} devices"),
            Self::Retrying(message) | Self::Failed(message) => message.clone(),
        }
    }
}

/// This install in rooms: a stable id and the name others see.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct Identity {
    id: String,
    /// Empty: the device kind ("Mac", "iPhone", "Web").
    #[serde(default)]
    name: String,
}

fn load_identity() -> Identity {
    let mut identity = load_json_value::<Identity>(IDENTITY_KEY).unwrap_or_default();
    if identity.id.trim().is_empty() {
        identity.id = uuid::Uuid::new_v4().to_string();
        let _ = save_json_value(IDENTITY_KEY, &identity);
    }
    identity
}

/// `device.platform` for this build.
pub fn device_platform() -> &'static str {
    match device_label() {
        "iPhone" => "ios",
        "iPad" => "ipad",
        "Mac" => "mac",
        "Web" => "web",
        "Windows" => "windows",
        "Android" => "android",
        _ => "linux",
    }
}

/// The name others see: the one set in Settings, else the device kind.
pub fn display_name(name: &str) -> String {
    let name = name.trim();
    let name = if name.is_empty() { device_label() } else { name };
    name.chars().take(MAX_NAME_CHARS).collect()
}

/// The music server whose room this device joins: the first one switched on
/// with a sync URL (Subsonic only; Jellyfin has its own session commands).
pub fn room_server(servers: &[ServerConfig]) -> Option<&ServerConfig> {
    servers
        .iter()
        .find(|s| s.active && !s.is_jellyfin() && !s.sync_url.trim().is_empty())
}

fn build_target(servers: &[ServerConfig], offline: bool, id: &str, name: &str, can_play: bool) -> Option<(Target, ServerConfig)> {
    if offline {
        return None;
    }
    let server = room_server(servers)?;
    let socket_url = logic::socket_url(&server.sync_url)?;
    let (t, s) = NavidromeClient::new(server.clone()).sync_auth();
    let hello = Hello {
        v: PROTOCOL_VERSION,
        server: server.url.clone(),
        user: server.username.clone(),
        auth: Auth::Token { t, s },
        device: DeviceInfo {
            id: id.to_string(),
            name: display_name(name),
            platform: device_platform().to_string(),
            can_play,
        },
    };
    Some((Target { socket_url, hello }, server.clone()))
}

/// Per-connection bookkeeping; not reactive.
#[derive(Default)]
struct Engine {
    sender: StateSender,
    queue: QueueBook,
    next_id: u64,
    /// Ids of sent commands and queues, so their errors become toasts.
    pending: HashSet<String>,
    /// A transfer paused us: name the new device once `devices` says who.
    announce_away: bool,
    /// The queue last prewarmed, so each queue is warmed once.
    prewarmed: u64,
    /// The target the running connection was started for.
    running: Option<(Target, ServerConfig)>,
}

impl Engine {
    fn next_id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{}", self.next_id)
    }
}

/// App-level remote sync, provided by [`use_remote_sync_provider`].
#[derive(Clone, Copy)]
pub struct RemoteSync {
    pub status: Signal<SyncStatus>,
    /// Everyone in the room, this device included, in join order.
    pub devices: Signal<Vec<Device>>,
    /// The room's last playback state (the active device's).
    pub room_state: Signal<Option<StateSnapshot>>,
    pub clock: Signal<Clock>,
    /// The name set in Settings ("" = the device kind).
    pub device_name: Signal<String>,
    device_id: Signal<String>,
    /// Devices seen playing since we joined (see `logic::remote_view`).
    seen_playing: Signal<HashSet<String>>,
    /// Names of devices seen in this room, for "Last played on iPad".
    names: Signal<HashMap<String, String>>,
    /// Web: false until the first tap (autoplay rules).
    can_play: Signal<bool>,
    target: Memo<Option<(Target, ServerConfig)>>,
    engine: CopyValue<Engine>,
    co: Coroutine<Outgoing>,
    player: PlayerCtx,
    preview: Signal<bool>,
    seek_request: Signal<Option<(String, f64)>>,
    toast: StatusToast,
}

/// Visibility changes (the app or tab coming back) and, on the web, the
/// first user gesture.
const WAKE_JS: &str = r#"
document.addEventListener("visibilitychange", () => { if (!document.hidden) dioxus.send("visible"); });
const active = navigator.userActivation && navigator.userActivation.hasBeenActive;
if (active) { dioxus.send("gesture"); } else {
  await new Promise((done) => {
    for (const name of ["pointerdown", "keydown", "touchstart"]) window.addEventListener(name, done, { capture: true, once: true });
  });
  dioxus.send("gesture");
}
await new Promise(() => {});
"#;

/// Provides [`RemoteSync`]; call once in the app shell, after the player's
/// signals and `StatusToast`.
pub fn use_remote_sync_provider() -> RemoteSync {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let settings = use_context::<Signal<AppSettings>>();
    let identity = use_hook(load_identity);
    let device_id = use_signal(|| identity.id.clone());
    let device_name = use_signal(|| identity.name.clone());
    let can_play = use_signal(|| !cfg!(target_arch = "wasm32"));
    let offline = use_memo(move || settings.read().offline_mode);
    let target = use_memo(move || build_target(&servers.read(), offline(), &device_id.read(), &device_name.read(), can_play()));

    let co = use_coroutine(move |mut rx: UnboundedReceiver<Outgoing>| async move {
        let sync = consume_context::<RemoteSync>();
        let running = sync.target.peek().clone();
        sync.engine.write_unchecked().running = running.clone();
        let Some((target, _)) = running else {
            sync.set_status(SyncStatus::Off);
            sync.on_disconnected();
            // Nothing to connect to; keep the channel drained.
            while rx.next().await.is_some() {}
            return;
        };
        connection::run(sync, target, &mut rx).await;
    });

    let sync = use_context_provider(|| RemoteSync {
        status: Signal::new(SyncStatus::Off),
        devices: Signal::new(Vec::new()),
        room_state: Signal::new(None),
        clock: Signal::new(Clock::default()),
        device_name,
        device_id,
        seen_playing: Signal::new(HashSet::new()),
        names: Signal::new(HashMap::new()),
        can_play,
        target,
        engine: CopyValue::new(Engine::default()),
        co,
        player: PlayerCtx::from_context(),
        preview: consume_context::<PreviewPlaybackSignal>().0,
        seek_request: consume_context::<SeekRequestSignal>().0,
        toast: consume_context::<StatusToast>(),
    });

    // A new server, login, name or `can_play` reconnects.
    use_effect(move || {
        let next = target();
        if sync.engine.peek().running != next {
            let mut co = sync.co;
            co.restart();
        }
    });

    // Play/pause and track changes go out at once, not on the next tick.
    {
        let player = sync.player;
        use_effect(move || {
            let _ = (player.is_playing)();
            let _ = player.now_playing.read();
            sync.tick();
        });
    }

    // Each server's media cache: ask `/health` when sync URLs change.
    let media_servers = use_memo(move || {
        servers
            .read()
            .iter()
            .filter(|s| !s.is_jellyfin())
            .map(|s| (s.id.clone(), s.url.clone(), s.username.clone(), s.password.clone(), s.sync_url.clone()))
            .collect::<Vec<_>>()
    });
    use_effect(move || {
        let _ = media_servers();
        if offline() {
            return;
        }
        for server in servers.peek().iter().filter(|s| !s.is_jellyfin()).cloned() {
            spawn(async move {
                let _ = media::refresh(&server).await;
            });
        }
    });

    // Back in the foreground: reconnect now instead of after the backoff.
    // Web: the first tap lets this tab play, so it rejoins with `can_play`.
    use_hook(move || {
        spawn(async move {
            let mut wake = document::eval(WAKE_JS);
            while let Ok(event) = wake.recv::<String>().await {
                if event == "gesture" {
                    let mut can_play = sync.can_play;
                    if !*can_play.peek() {
                        can_play.set(true);
                    }
                } else {
                    sync.co.send(Outgoing::Wake);
                }
            }
        });
    });

    sync
}

impl RemoteSync {
    pub fn this_device_id(&self) -> String {
        self.device_id.peek().clone()
    }

    pub fn is_connected(&self) -> bool {
        *self.status.read() == SyncStatus::Connected
    }

    /// The name others see for this device.
    pub fn this_device_name(&self) -> String {
        display_name(&self.device_name.read())
    }

    /// Renames this device (Settings); reconnects so the room sees it.
    pub fn set_device_name(&self, name: &str) {
        let name: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
        let identity = Identity { id: self.this_device_id(), name: name.clone() };
        let _ = save_json_value(IDENTITY_KEY, &identity);
        let mut device_name = self.device_name;
        if *device_name.peek() != name {
            device_name.set(name);
        }
    }

    /// What the player shows instead of local playback, if anything.
    /// Reactive: re-renders the caller on room and player changes.
    pub fn view(&self) -> Option<RemoteView> {
        if !self.is_connected() {
            return None;
        }
        let me = self.device_id.read();
        let local_has_song = self.player.now_playing.read().is_some();
        remote_view(
            &self.devices.read(),
            &me,
            self.room_state.read().as_ref(),
            local_has_song,
            &self.seen_playing.read(),
            &self.names.read(),
        )
    }

    /// The other devices "Play on" lists (empty while disconnected).
    pub fn play_on(&self) -> Vec<Device> {
        if !self.is_connected() {
            return Vec::new();
        }
        play_on_targets(&self.devices.read(), &self.device_id.read())
    }

    /// The active device's id, when it isn't this one.
    pub fn active_elsewhere(&self) -> Option<Device> {
        let me = self.device_id.read();
        self.devices.read().iter().find(|d| d.active && d.info.id != *me).cloned()
    }

    /// Sends `op` to `target`. Errors come back as a toast.
    pub fn send(&self, target: &str, op: Op) {
        if *self.status.peek() != SyncStatus::Connected {
            self.toast.show("Not connected to the sync server.", true);
            return;
        }
        let id = {
            let mut engine = self.engine.write_unchecked();
            let id = engine.next_id("c");
            engine.pending.insert(id.clone());
            id
        };
        let frame = ClientFrame::with_id(id, ClientMessage::Cmd(CommandRequest { target: target.to_string(), op }));
        self.co.send(Outgoing::Frame(frame));
    }

    /// Moves playback to `target` (this device's id: "Play here").
    pub fn transfer_to(&self, target: &str) {
        self.send(target, Op::Transfer);
    }

    pub fn play_here(&self) {
        self.transfer_to(&self.this_device_id());
    }

    /// Play/Pause on the active device, showing the change at once (the next
    /// `state` from it settles it).
    pub fn toggle_remote(&self, target: &str) {
        let Some(mut state) = self.room_state.peek().clone() else {
            return;
        };
        let playing = state.state.playing;
        self.send(target, if playing { Op::Pause } else { Op::Play });
        let server_now = self.clock.peek().server_now(now_ms());
        state.state.position_ms = state.position_at(server_now);
        state.at = server_now;
        state.state.playing = !playing;
        let mut room_state = self.room_state;
        room_state.set(Some(state));
    }

    pub(crate) fn set_status(&self, status: SyncStatus) {
        let mut signal = self.status;
        if *signal.peek() != status {
            signal.set(status);
        }
    }

    /// The socket closed: forget the room (`welcome` brings it back).
    fn on_disconnected(&self) {
        let (mut devices, mut room_state, mut seen) = (self.devices, self.room_state, self.seen_playing);
        if !devices.peek().is_empty() {
            devices.set(Vec::new());
        }
        if room_state.peek().is_some() {
            room_state.set(None);
        }
        if !seen.peek().is_empty() {
            seen.set(HashSet::new());
        }
        let mut clock = self.clock;
        clock.with_mut(Clock::reset);
        let server_id = {
            let mut engine = self.engine.write_unchecked();
            engine.pending.clear();
            engine.running.as_ref().map(|(_, server)| server.id.clone())
        };
        if let Some(id) = server_id {
            media::mark_unreachable(&id);
        }
    }

    fn set_devices(&self, list: Vec<Device>) {
        let mut names = self.names;
        let new_names: Vec<(String, String)> = list
            .iter()
            .filter(|d| names.peek().get(&d.info.id) != Some(&d.info.name))
            .map(|d| (d.info.id.clone(), d.info.name.clone()))
            .collect();
        if !new_names.is_empty() {
            names.with_mut(|map| map.extend(new_names));
        }
        let mut devices = self.devices;
        if *devices.peek() != list {
            devices.set(list);
        }
    }

    fn set_room_state(&self, state: Option<StateSnapshot>) {
        if let Some(playing_from) = state.as_ref().filter(|s| s.state.playing).map(|s| s.from.clone()) {
            let mut seen = self.seen_playing;
            if !seen.peek().contains(&playing_from) {
                seen.with_mut(|set| set.insert(playing_from));
            }
        }
        let mut room_state = self.room_state;
        room_state.set(state);
    }

    /// Handles one server frame; returns frames to send back.
    fn on_frame(&self, frame: ServerFrame) -> Vec<ClientFrame> {
        let now = now_ms();
        let mut clock = self.clock;
        let me = self.this_device_id();
        match frame.msg {
            ServerMessage::Welcome(welcome) => {
                clock.with_mut(|c| c.observe_frame(frame.ts, now));
                self.set_devices(welcome.devices);
                self.set_room_state(welcome.state);
                let server = {
                    let mut engine = self.engine.write_unchecked();
                    engine.queue.adopt(welcome.queue.as_ref());
                    engine.sender.reset();
                    engine.running.as_ref().map(|(_, server)| server.clone())
                };
                self.set_status(SyncStatus::Connected);
                if let Some(server) = server {
                    spawn(async move {
                        let _ = media::refresh(&server).await;
                    });
                }
                self.local_frames(now)
            }
            ServerMessage::Devices { devices } => {
                self.set_devices(devices);
                let announce = std::mem::take(&mut self.engine.write_unchecked().announce_away);
                if announce {
                    if let Some(active) = self.devices.peek().iter().find(|d| d.active && d.info.id != me) {
                        self.toast.show(format!("Now playing on {}", active.info.name), false);
                    }
                }
                Vec::new()
            }
            ServerMessage::State(state) => {
                clock.with_mut(|c| c.observe_frame(frame.ts, now));
                self.set_room_state(Some(state));
                Vec::new()
            }
            ServerMessage::Queue(queue) => {
                self.engine.write_unchecked().queue.adopt(Some(&queue));
                Vec::new()
            }
            ServerMessage::Cmd(cmd) => {
                if cmd.transfer {
                    match cmd.op {
                        Op::PlayQueue { .. } if cmd.from != me => {
                            let from = self
                                .devices
                                .peek()
                                .iter()
                                .find(|d| d.active && d.info.id != me)
                                .map(|d| d.info.name.clone());
                            if let Some(from) = from {
                                self.toast.show(format!("Playing from {from}"), false);
                            }
                        }
                        Op::Pause => self.engine.write_unchecked().announce_away = true,
                        _ => {}
                    }
                }
                let server = self.engine.peek().running.as_ref().map(|(_, server)| server.clone());
                if let Some(server) = server {
                    commands::apply(self.player, self.seek_request, server, cmd.op);
                }
                Vec::new()
            }
            ServerMessage::Ack { id } => {
                self.engine.write_unchecked().pending.remove(&id);
                Vec::new()
            }
            ServerMessage::Error(error) => {
                let mine = error
                    .id
                    .as_ref()
                    .is_some_and(|id| self.engine.write_unchecked().pending.remove(id));
                // A stale queue was answered with the room's (adopted
                // above); the next tick sends ours again on top of it.
                if mine && error.code != ErrorCode::StaleQueueRev {
                    let server_url = self.engine.peek().running.as_ref().map(|(_, s)| s.url.clone()).unwrap_or_default();
                    self.toast.show(error_message(error.code, &server_url), true);
                }
                Vec::new()
            }
            ServerMessage::Time { client_at, server_at } => {
                clock.with_mut(|c| c.observe_probe(client_at, server_at, now));
                Vec::new()
            }
        }
    }

    /// Sends this device's state and queue when they're due. Called once a
    /// second by the playback sync loop and on play/pause and track changes.
    pub fn tick(&self) {
        if *self.status.peek() != SyncStatus::Connected {
            return;
        }
        for frame in self.local_frames(now_ms()) {
            self.co.send(Outgoing::Frame(frame));
        }
    }

    /// `queue` and `state` for this device, when due.
    fn local_frames(&self, now: u64) -> Vec<ClientFrame> {
        let Some(server) = self.engine.peek().running.as_ref().map(|(_, server)| server.clone()) else {
            return Vec::new();
        };
        let player = self.player;
        let song = player
            .now_playing
            .peek()
            .clone()
            .filter(|s| s.server_id == server.id && !*self.preview.peek());
        let playing = song.is_some() && *player.is_playing.peek();
        let me = self.this_device_id();
        let active_here = self.devices.peek().iter().any(|d| d.active && d.info.id == me);
        if !speaks_for_room(playing, active_here) {
            return Vec::new();
        }
        let position_ms = (player.position.peek().max(0.0) * 1000.0).round() as u64;
        // The same queue rules as the server's saved play queue: this
        // server's library songs only.
        let plan = song.as_ref().and_then(|song| {
            plan_queue_save(&player.queue.peek(), *player.queue_index.peek(), song, position_ms, std::slice::from_ref(&server))
        });
        let mut frames = Vec::new();
        let mut engine = self.engine.write_unchecked();
        let mut index = None;
        if let Some(plan) = plan.as_ref() {
            let (start, end) = queue_window(plan.ids.len(), plan.current_index);
            let ids = &plan.ids[start..end];
            index = Some((plan.current_index - start) as u32);
            if let Some(update) = engine.queue.plan(ids) {
                let id = engine.next_id("q");
                engine.pending.insert(id.clone());
                frames.push(ClientFrame::with_id(id, ClientMessage::Queue(update)));
                self.prewarm(&server, &mut engine);
            }
        }
        let rate = effective_rate(player.settings.peek().playback_rate, song.as_ref());
        let local = LocalState {
            track_id: song.as_ref().map(|s| s.id.clone()),
            index,
            position_ms,
            playing,
            rate,
            volume: *player.volume.peek(),
        };
        if engine.sender.due(&local, now) {
            frames.push(ClientFrame::new(ClientMessage::State(local.to_wire(engine.queue.rev(), now))));
        }
        frames
    }

    /// Asks the sync server to warm the new queue's covers and lyrics.
    fn prewarm(&self, server: &ServerConfig, engine: &mut Engine) {
        let queue = self.player.queue.peek();
        let songs: Vec<_> = queue.iter().filter(|s| s.server_id == server.id).collect();
        let signature = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            songs.iter().for_each(|s| s.id.hash(&mut hasher));
            hasher.finish()
        };
        if engine.prewarmed == signature {
            return;
        }
        engine.prewarmed = signature;
        let index = *self.player.queue_index.peek();
        let covers = songs.iter().filter_map(|s| s.cover_art.clone()).collect();
        let upcoming = queue
            .iter()
            .skip(index)
            .filter(|s| s.server_id == server.id)
            .map(|s| media::PrewarmSong {
                id: s.id.clone(),
                artist: s.artist.clone().unwrap_or_default(),
                title: s.title.clone(),
                album: s.album.clone().unwrap_or_default(),
                duration: s.duration,
            })
            .collect();
        media::prewarm(server, covers, upcoming);
    }

    /// The active device's position now, interpolated from its last state.
    pub fn live_position_ms(&self, duration_ms: Option<u64>) -> Option<u64> {
        let state = self.room_state.peek().clone()?;
        Some(logic::live_position_ms(&state, &self.clock.peek(), now_ms(), duration_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(id: &str, active: bool, sync: &str) -> ServerConfig {
        ServerConfig {
            id: id.into(),
            name: id.into(),
            url: format!("http://{id}"),
            username: "u".into(),
            password: "p".into(),
            active,
            sync_url: sync.into(),
            ..Default::default()
        }
    }

    #[test]
    fn the_room_is_the_first_active_server_with_a_sync_url() {
        let list = vec![server("a", true, ""), server("b", false, "http://s"), server("c", true, "http://s")];
        assert_eq!(room_server(&list).map(|s| s.id.as_str()), Some("c"));
        assert_eq!(room_server(&list[..2]), None, "off unless a sync URL is set");
    }

    #[test]
    fn hello_carries_the_server_url_and_login_as_stored() {
        let list = vec![server("music", true, "http://sync.test/rs/")];
        let (target, room) = build_target(&list, false, "dev-1", "  Den  ", true).unwrap();
        assert_eq!(target.socket_url, "ws://sync.test/rs/ws");
        assert_eq!(target.hello.server, "http://music");
        assert_eq!(target.hello.user, "u");
        assert_eq!(target.hello.device.name, "Den");
        assert_eq!(target.hello.device.id, "dev-1");
        assert!(matches!(&target.hello.auth, Auth::Token { t, s } if t.len() == 32 && !s.is_empty()));
        assert_eq!(room.id, "music");
        assert!(build_target(&list, true, "dev-1", "", true).is_none(), "offline mode");
    }

    #[test]
    fn names_default_to_the_device_kind() {
        assert_eq!(display_name(""), device_label());
        assert_eq!(display_name(&"x".repeat(100)).chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn the_status_line_counts_devices() {
        assert_eq!(SyncStatus::Connected.line(3), "Connected · 3 devices");
        assert_eq!(SyncStatus::Connected.line(1), "Connected · this device only");
        assert_eq!(SyncStatus::Connecting.line(0), "Connecting…");
    }
}
