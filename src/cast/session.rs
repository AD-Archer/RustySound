//! A cast in progress. Commands go to the speaker; its polled state comes
//! back as a `SessionSnapshot`, the same shape the local players report,
//! so the audio controller can't tell the difference:
//!
//! * Commands are optimistic: right after Play the snapshot says playing,
//!   while the speaker buffers, instead of bouncing the play button.
//! * Between polls (one a second) the position runs on by the clock.
//! * The end of a song: Cast says `FINISHED`; UPnP just stops, so a stop
//!   near the end that we didn't ask for counts as the end, and anywhere
//!   else as a pause (someone used the speaker's own controls). The
//!   controller then moves the queue on and loads the next song here.

use super::lan_server::LanServer;
use super::stream::{CastMedia, MediaSource};
use super::{CastResult, CastTrack, Renderer, RendererStatus, Speaker, Transport};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long after a command the speaker's own report is not believed
/// (it may still describe the moment before).
pub const SETTLE: Duration = Duration::from_millis(3000);
/// A stop within this many seconds of the end is the end.
pub const END_SLACK: f64 = 4.0;
/// Polls in a row that may fail before the speaker counts as gone.
pub const MAX_FAILURES: u32 = 5;

/// What the session is told to do (the audio manager's commands, typed).
#[derive(Clone, Debug, PartialEq)]
pub enum SessionCommand {
    Load { track: CastTrack, position: f64, play: bool },
    Play,
    Pause,
    Seek(f64),
    /// 0–1.
    Volume(f64),
    Stop,
}

/// The speaker as the player sees it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionSnapshot {
    pub current_time: f64,
    pub duration: f64,
    pub paused: bool,
    /// The song played to its end (until the next load, play or seek).
    pub ended: bool,
    pub song_id: Option<String>,
    /// `play` / `pause` once, when the speaker changed by its own controls.
    pub action: Option<String>,
}

/// The state behind the snapshot. Pure: time comes in as `now`.
#[derive(Clone, Debug)]
pub struct RemoteState {
    song_id: Option<String>,
    duration: f64,
    live: bool,
    position: f64,
    position_at: Instant,
    transport: Transport,
    want_playing: bool,
    commanded_at: Option<Instant>,
    /// The furthest position seen while playing (for spotting the end).
    furthest: f64,
    ended: bool,
    action: Option<String>,
    /// `SETTLE`; shorter in tests.
    settle: Duration,
}

impl RemoteState {
    pub fn new(now: Instant) -> Self {
        RemoteState {
            settle: SETTLE,
            song_id: None,
            duration: 0.0,
            live: false,
            position: 0.0,
            position_at: now,
            transport: Transport::Stopped,
            want_playing: false,
            commanded_at: None,
            furthest: 0.0,
            ended: false,
            action: None,
        }
    }

    fn settling(&self, now: Instant) -> bool {
        self.commanded_at.is_some_and(|at| now.saturating_duration_since(at) < self.settle)
    }

    fn commanded(&mut self, now: Instant) {
        self.commanded_at = Some(now);
        self.ended = false;
        self.action = None;
    }

    fn set_position(&mut self, position: f64, now: Instant) {
        self.position = position.max(0.0);
        self.position_at = now;
    }

    pub fn on_load(&mut self, track: &CastTrack, position: f64, play: bool, now: Instant) {
        self.commanded(now);
        self.song_id = Some(track.song_id.clone());
        self.duration = track.duration.max(0.0);
        self.live = track.live;
        self.furthest = position.max(0.0);
        self.want_playing = play;
        self.transport = if play { Transport::Buffering } else { Transport::Paused };
        self.set_position(position, now);
    }

    pub fn on_play(&mut self, now: Instant) {
        let position = self.current_time(now);
        self.commanded(now);
        self.set_position(position, now);
        self.want_playing = true;
    }

    pub fn on_pause(&mut self, now: Instant) {
        let position = self.current_time(now);
        self.commanded(now);
        self.set_position(position, now);
        self.want_playing = false;
    }

    pub fn on_seek(&mut self, position: f64, now: Instant) {
        self.commanded(now);
        let position = if self.duration > 0.0 { position.min(self.duration) } else { position };
        self.furthest = position.max(0.0);
        self.set_position(position, now);
    }

    pub fn on_stop(&mut self, now: Instant) {
        self.commanded(now);
        self.want_playing = false;
        self.song_id = None;
        self.transport = Transport::Stopped;
        self.set_position(0.0, now);
    }

    fn playing(&self, now: Instant) -> bool {
        if self.ended {
            return false;
        }
        if self.settling(now) {
            return self.want_playing;
        }
        matches!(self.transport, Transport::Playing | Transport::Buffering)
    }

    fn current_time(&self, now: Instant) -> f64 {
        let mut time = self.position;
        if self.playing(now) && self.transport != Transport::Buffering {
            time += now.saturating_duration_since(self.position_at).as_secs_f64();
        }
        if self.duration > 0.0 {
            time = time.min(self.duration);
        }
        time
    }

    /// Takes in one poll of the speaker.
    pub fn observe(&mut self, status: &RendererStatus, now: Instant) {
        if self.song_id.is_none() || self.ended {
            return;
        }
        if status.duration > 0.0 {
            self.duration = status.duration;
        }
        let settling = self.settling(now);
        let was_playing = matches!(self.transport, Transport::Playing | Transport::Buffering) || self.want_playing;
        // Where it should be by now, if it kept playing since the last poll.
        let reached = self.furthest.max(self.current_time(now)).max(status.position);

        let finished = !self.live
            && (status.finished
                || (status.transport == Transport::Stopped
                    && !settling
                    && was_playing
                    && self.duration > 0.0
                    && reached >= self.duration - END_SLACK));
        if finished {
            self.ended = true;
            self.want_playing = false;
            self.transport = Transport::Stopped;
            self.set_position(self.duration.max(self.furthest), now);
            return;
        }
        if settling {
            // Only take the speaker's position once it's playing what we asked.
            if status.transport == Transport::Playing && self.want_playing {
                self.transport = Transport::Playing;
            }
            return;
        }
        let playing_now = matches!(status.transport, Transport::Playing | Transport::Buffering);
        if playing_now != self.want_playing {
            // Changed on the speaker itself: follow it, and tell the player.
            self.want_playing = playing_now;
            self.action = Some(if playing_now { "play" } else { "pause" }.to_string());
        }
        self.transport = status.transport;
        if status.transport != Transport::Stopped || status.position > 0.0 {
            self.set_position(status.position, now);
        }
        if playing_now {
            self.furthest = self.furthest.max(status.position);
        }
    }

    pub fn snapshot(&mut self, now: Instant) -> SessionSnapshot {
        SessionSnapshot {
            current_time: self.current_time(now),
            duration: self.duration,
            paused: !self.playing(now),
            ended: self.ended,
            song_id: self.song_id.clone(),
            action: self.action.take(),
        }
    }
}

/// A connected speaker and what it's playing.
pub struct CastSession {
    pub speaker: Speaker,
    renderer: Renderer,
    state: Arc<Mutex<RemoteState>>,
    lan: Option<LanServer>,
    /// A paused load's start position (UPnP seeks once it plays).
    pending_seek: Option<f64>,
    failures: u32,
}

impl CastSession {
    pub async fn start(speaker: Speaker) -> CastResult<Self> {
        let renderer = Renderer::connect(&speaker).await?;
        Ok(CastSession {
            speaker,
            renderer,
            state: Arc::new(Mutex::new(RemoteState::new(Instant::now()))),
            lan: None,
            pending_seek: None,
            failures: 0,
        })
    }

    /// Shared with the audio manager's snapshot reads.
    pub fn state(&self) -> Arc<Mutex<RemoteState>> {
        self.state.clone()
    }

    pub fn plays(&self, mime: &str) -> bool {
        self.renderer.plays(mime)
    }

    pub async fn volume(&self) -> Option<f64> {
        self.renderer.volume().await
    }

    /// A URL the speaker can fetch the media from (sharing a local file
    /// through the LAN server, started the first time it's needed).
    pub async fn media_url(&mut self, media: &CastMedia) -> CastResult<String> {
        match &media.source {
            MediaSource::Url(url) => Ok(url.clone()),
            MediaSource::File(path) => {
                if self.lan.is_none() {
                    let peer = self.speaker.ip().ok_or("The speaker has no address")?;
                    self.lan = Some(LanServer::start(peer).await.map_err(|err| err.to_string())?);
                }
                Ok(self.lan.as_ref().map(|lan| lan.share(path.clone(), &media.mime)).unwrap_or_default())
            }
        }
    }

    fn with_state(&self, change: impl FnOnce(&mut RemoteState)) {
        if let Ok(mut state) = self.state.lock() {
            change(&mut state);
        }
    }

    pub async fn apply(&mut self, command: SessionCommand) -> CastResult<()> {
        let now = Instant::now();
        match command {
            SessionCommand::Load { track, position, play } => {
                self.with_state(|state| state.on_load(&track, position, play, now));
                self.pending_seek = (!play && position > 1.0 && !track.live).then_some(position);
                self.renderer.load(&track, position, play).await
            }
            SessionCommand::Play => {
                self.with_state(|state| state.on_play(now));
                self.renderer.play().await?;
                if let Some(position) = self.pending_seek.take() {
                    let _ = self.renderer.seek(position).await;
                }
                Ok(())
            }
            SessionCommand::Pause => {
                self.with_state(|state| state.on_pause(now));
                self.renderer.pause().await
            }
            SessionCommand::Seek(position) => {
                self.with_state(|state| state.on_seek(position, now));
                if self.pending_seek.is_some() {
                    self.pending_seek = Some(position);
                    return Ok(());
                }
                self.renderer.seek(position).await
            }
            SessionCommand::Volume(level) => self.renderer.set_volume(level).await,
            SessionCommand::Stop => {
                self.with_state(|state| state.on_stop(now));
                self.pending_seek = None;
                self.renderer.stop().await
            }
        }
    }

    /// Reads the speaker's state into the snapshot. An error after
    /// `MAX_FAILURES` misses in a row: the speaker is gone.
    pub async fn poll(&mut self) -> CastResult<()> {
        match self.renderer.status().await {
            Ok(status) => {
                self.failures = 0;
                self.with_state(|state| state.observe(&status, Instant::now()));
                Ok(())
            }
            Err(err) => {
                self.failures += 1;
                if self.failures >= MAX_FAILURES {
                    Err(err)
                } else {
                    Ok(())
                }
            }
        }
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        self.state.lock().map(|mut state| state.snapshot(Instant::now())).unwrap_or_default()
    }

    #[cfg(test)]
    pub fn set_settle(&self, settle: Duration) {
        self.with_state(|state| state.settle = settle);
    }

    /// Stops the speaker (playback goes back to this device).
    pub async fn stop(mut self) {
        let _ = self.apply(SessionCommand::Stop).await;
    }
}
