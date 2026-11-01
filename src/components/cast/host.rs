//! The cast host: one coroutine in the shell's scope that owns the speaker
//! list and the session. Searches run beside it (they take a couple of
//! seconds); a connected speaker gets the player's commands in order, with
//! runs of volume changes and seeks folded into the last one.

use super::{ActiveSpeaker, CastCtx, CastPrefs, CastRequest, SpeakerInfo, PREFS_KEY};
use crate::api::{MusicClient, ServerConfig, Song};
use crate::cast::{cast_media, CastSession, CastTrack, DiscoveryOptions, RemoteState, SessionCommand, Speaker};
use crate::components::ui::StatusToast;
use crate::components::{
    output_labels, resume_local_output, set_remote_output, AudioOutput, AudioState, IsPlayingSignal,
    OutputCommand, OutputState, PlaybackPositionSignal, RemoteOutput, VolumeSignal,
};
use crate::db::{load_json_value, save_json_value, AppSettings};
use dioxus::prelude::*;
use futures_util::StreamExt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// A search answered within this long is reused when a menu opens.
const FRESH_FOR: Duration = Duration::from_secs(60);
/// How often a playing speaker is asked where it is.
const POLL_EVERY: Duration = Duration::from_secs(1);
/// The first background search waits for the app to finish starting.
const STARTUP_DELAY_MS: u64 = 4000;

/// The speakers from the last search (the views get `SpeakerInfo`s).
static FOUND: Mutex<Vec<Speaker>> = Mutex::new(Vec::new());

fn info(speaker: &Speaker) -> SpeakerInfo {
    let kind = speaker.kind_label();
    let model = speaker.model.trim();
    let detail = if model.is_empty() {
        kind.to_string()
    } else if model.contains(kind) {
        model.to_string()
    } else {
        format!("{model} · {kind}")
    };
    SpeakerInfo { id: speaker.id.clone(), name: speaker.name.clone(), detail }
}

/// The session's side of the seam: commands into a channel the host
/// drains, state from the shared snapshot.
struct CastOutput {
    commands: mpsc::UnboundedSender<OutputCommand>,
    state: Arc<Mutex<RemoteState>>,
}

impl RemoteOutput for CastOutput {
    fn command(&self, command: OutputCommand) {
        let _ = self.commands.send(command);
    }

    fn state(&self) -> OutputState {
        let Ok(mut state) = self.state.lock() else {
            return OutputState::default();
        };
        let snap = state.snapshot(Instant::now());
        OutputState {
            current_time: snap.current_time,
            duration: snap.duration,
            paused: snap.paused,
            ended: snap.ended,
            song_id: snap.song_id,
            action: snap.action,
        }
    }
}

/// A cast in progress.
struct Live {
    session: CastSession,
    commands: mpsc::UnboundedReceiver<OutputCommand>,
    /// The volume to put back on this device afterwards.
    local_volume: f64,
}

/// Folds a batch of commands: only the last of a run of volume changes,
/// and of seeks, is worth sending.
fn fold_commands(batch: Vec<OutputCommand>) -> Vec<OutputCommand> {
    let mut folded: Vec<OutputCommand> = Vec::new();
    for command in batch {
        if let (OutputCommand::Volume(_), Some(OutputCommand::Volume(_)))
        | (OutputCommand::Seek(_), Some(OutputCommand::Seek(_))) = (&command, folded.last())
        {
            folded.pop();
        }
        folded.push(command);
    }
    folded
}

/// The playback signals the host reads and writes.
#[derive(Clone, Copy)]
struct Player {
    servers: Signal<Vec<ServerConfig>>,
    settings: Signal<AppSettings>,
    now_playing: Signal<Option<Song>>,
    queue: Signal<Vec<Song>>,
    is_playing: Signal<bool>,
    position: Signal<f64>,
    volume: Signal<f64>,
    audio_state: Signal<AudioState>,
    output: AudioOutput,
    toast: StatusToast,
}

impl Player {
    fn song(&self, song_id: &str) -> Option<Song> {
        if let Some(song) = self.now_playing.peek().as_ref().filter(|song| song.id == song_id) {
            return Some(song.clone());
        }
        self.queue.peek().iter().find(|song| song.id == song_id).cloned()
    }

    /// A song the speaker can't play: say so where playback errors show,
    /// and stop (as the local player does).
    fn fail(&self, message: String) {
        let mut audio_state = self.audio_state;
        audio_state.write().playback_error.set(Some(message));
        let mut is_playing = self.is_playing;
        is_playing.set(false);
    }

    fn set_volume(&self, level: f64) {
        let mut volume = self.volume;
        volume.set(level);
    }

    fn name_route(&self, name: Option<String>) {
        let mut remote = self.output.remote;
        remote.set(name);
    }
}

/// The speaker's view of a song: a URL it can fetch, labels and cover.
async fn cast_track(session: &mut CastSession, song: &Song, servers: &[ServerConfig]) -> Result<CastTrack, String> {
    let name = session.speaker.name.clone();
    let media = cast_media(song, servers, |mime| session.plays(mime))
        .ok_or_else(|| format!("{name} can't reach this song."))?;
    let url = session.media_url(&media).await?;
    let (title, artist, album) = output_labels(song, servers);
    let cover_url = song.cover_art.as_ref().and_then(|cover| {
        let server = servers.iter().find(|server| server.id == song.server_id)?;
        MusicClient::new(server.clone()).get_cover_art_network_url(cover, 600)
    });
    let live = song.server_name == "Radio";
    Ok(CastTrack {
        song_id: song.id.clone(),
        url,
        mime: media.mime,
        title,
        artist,
        album,
        cover_url,
        duration: if live { 0.0 } else { song.duration as f64 },
        live,
    })
}

async fn run_command(live: &mut Live, player: Player, command: OutputCommand) {
    let session = &mut live.session;
    let result = match command {
        OutputCommand::Load { song_id, position, play } => {
            let Some(song) = player.song(&song_id) else {
                return;
            };
            let servers = player.servers.peek().clone();
            let loaded = match cast_track(session, &song, &servers).await {
                Ok(track) => session.apply(SessionCommand::Load { track, position, play }).await,
                Err(message) => Err(message),
            };
            if let Err(message) = &loaded {
                player.fail(message.clone());
            }
            loaded
        }
        OutputCommand::Play => session.apply(SessionCommand::Play).await,
        OutputCommand::Pause => session.apply(SessionCommand::Pause).await,
        OutputCommand::Seek(position) => session.apply(SessionCommand::Seek(position)).await,
        OutputCommand::Volume(level) => session.apply(SessionCommand::Volume(level)).await,
        OutputCommand::Clear => session.apply(SessionCommand::Stop).await,
    };
    if let Err(err) = result {
        // Pausing a stopped speaker and the like: worth a log, not a toast.
        eprintln!("[cast] {err}");
    }
}

fn save_prefs(prefs: CastPrefs) {
    let _ = save_json_value(PREFS_KEY, &prefs);
}

async fn next_command(live: &mut Option<Live>) -> Option<OutputCommand> {
    match live.as_mut() {
        Some(live) => live.commands.recv().await,
        None => std::future::pending().await,
    }
}

/// Connects, then moves playback over: the speaker's volume becomes the
/// player's, the local player lets go, and the current song loads there
/// where it was. `local_volume`: the volume to restore afterwards.
async fn take_over(speaker: Speaker, player: Player, local_volume: f64) -> Result<Live, String> {
    let session = CastSession::start(speaker).await?;
    let (commands_tx, commands) = mpsc::unbounded_channel();
    let output = CastOutput { commands: commands_tx.clone(), state: session.state() };
    let speaker_volume = session.volume().await;
    set_remote_output(Some(Arc::new(output)));
    if let Some(level) = speaker_volume {
        player.set_volume(level);
    }
    if let Some(song) = player.now_playing.peek().clone() {
        let _ = commands_tx.send(OutputCommand::Load {
            song_id: song.id,
            position: *player.position.peek(),
            play: *player.is_playing.peek(),
        });
    }
    player.name_route(Some(session.speaker.name.clone()));
    Ok(Live { session, commands, local_volume })
}

/// Puts playback back on this device: the volume as it was, and the song
/// loaded here at the current position, playing if `play`.
fn resume_here(player: Player, local_volume: f64, play: bool) {
    set_remote_output(None);
    player.name_route(None);
    player.set_volume(local_volume);
    if !play {
        let mut is_playing = player.is_playing;
        is_playing.set(false);
    }
    let song = player.now_playing.peek().clone();
    if let Some(song) = song {
        resume_local_output(
            &song,
            &player.servers.peek(),
            player.settings.peek().offline_mode,
            *player.position.peek(),
            play && *player.is_playing.peek(),
            local_volume,
        );
    }
}

/// Ends a cast on purpose: the speaker stops and the player's position is
/// the speaker's. Returns the volume to restore here.
async fn end_cast(live: Live, player: Player) -> f64 {
    let Live { session, local_volume, .. } = live;
    let position = session.snapshot().current_time;
    session.stop().await;
    let mut player_position = player.position;
    player_position.set(position);
    local_volume
}

enum Event {
    Request(Option<CastRequest>),
    Found(Vec<Speaker>),
    Command(OutputCommand),
    Tick,
}

/// Starts the host; returns where the views send requests.
pub(super) fn use_cast_host(ctx: CastCtx) -> Coroutine<CastRequest> {
    let player = Player {
        servers: use_context(),
        settings: use_context(),
        now_playing: use_context(),
        queue: use_context(),
        is_playing: use_context::<IsPlayingSignal>().0,
        position: use_context::<PlaybackPositionSignal>().0,
        volume: use_context::<VolumeSignal>().0,
        audio_state: use_context(),
        output: use_context(),
        toast: use_context(),
    };

    use_coroutine(move |mut requests: UnboundedReceiver<CastRequest>| async move {
        let CastCtx { mut speakers, mut searching, mut searched, mut active, mut discovery, .. } = ctx;
        let mut last_search: Option<Instant> = None;
        let mut live: Option<Live> = None;
        let mut poll = tokio::time::interval(POLL_EVERY);
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let (found_tx, mut found_rx) = mpsc::unbounded_channel::<Vec<Speaker>>();

        let search = move |searching: &mut Signal<bool>| {
            if *searching.peek() {
                return;
            }
            searching.set(true);
            let found_tx = found_tx.clone();
            spawn(async move {
                let found = crate::cast::discover(DiscoveryOptions::default()).await;
                let _ = found_tx.send(found);
            });
        };

        // Settings are read once the database is up. Without a search
        // before, the first one waits for the user (it asks for the local
        // network permission) until the Speakers row or settings open.
        crate::time::sleep_ms(STARTUP_DELAY_MS).await;
        let mut prefs = load_json_value::<CastPrefs>(PREFS_KEY).unwrap_or_default();
        discovery.set(prefs.discovery);
        if prefs.discovery && prefs.searched {
            search(&mut searching);
        }

        loop {
            let polling = live.is_some();
            let event = tokio::select! {
                request = requests.next() => Event::Request(request),
                Some(found) = found_rx.recv() => Event::Found(found),
                Some(command) = next_command(&mut live) => Event::Command(command),
                _ = poll.tick(), if polling => Event::Tick,
            };
            match event {
                Event::Request(None) => break,
                Event::Request(Some(CastRequest::Search { fresh_only })) => {
                    let fresh = last_search.is_some_and(|at| at.elapsed() < FRESH_FOR);
                    if prefs.discovery && !(fresh_only && fresh) {
                        if !prefs.searched {
                            prefs.searched = true;
                            save_prefs(prefs);
                        }
                        search(&mut searching);
                    }
                }
                Event::Request(Some(CastRequest::SetDiscovery(on))) => {
                    prefs.discovery = on;
                    if on {
                        prefs.searched = true;
                        search(&mut searching);
                    } else {
                        speakers.set(Vec::new());
                        searched.set(false);
                        last_search = None;
                    }
                    save_prefs(prefs);
                    discovery.set(on);
                }
                Event::Request(Some(CastRequest::Cast(target))) => {
                    let current = active.peek().as_ref().map(|active| active.id.clone());
                    if current == target {
                        continue;
                    }
                    // From one speaker straight to another: the volume to
                    // restore here is still this device's.
                    let mut local_volume = *player.volume.peek();
                    if let Some(previous) = live.take() {
                        local_volume = end_cast(previous, player).await;
                        set_remote_output(None);
                        active.set(None);
                        if target.is_none() {
                            resume_here(player, local_volume, true);
                            continue;
                        }
                    }
                    let Some(id) = target else { continue };
                    let speaker = FOUND.lock().ok().and_then(|found| found.iter().find(|s| s.id == id).cloned());
                    let Some(speaker) = speaker else { continue };
                    let name = speaker.name.clone();
                    active.set(Some(ActiveSpeaker { id: speaker.id.clone(), name: name.clone(), connecting: true }));
                    match take_over(speaker, player, local_volume).await {
                        Ok(started) => {
                            live = Some(started);
                            active.set(Some(ActiveSpeaker { id, name, connecting: false }));
                            poll.reset();
                        }
                        Err(err) => {
                            active.set(None);
                            // Coming from another speaker, play here instead.
                            if current.is_some() {
                                resume_here(player, local_volume, true);
                            }
                            player.toast.show(format!("Couldn't connect to {name}. {err}"), true);
                        }
                    }
                }
                Event::Found(mut found) => {
                    searching.set(false);
                    searched.set(true);
                    last_search = Some(Instant::now());
                    if !prefs.discovery {
                        continue;
                    }
                    // The playing speaker stays listed even if it missed this search.
                    if let Some(current) = live.as_ref().map(|live| live.session.speaker.clone()) {
                        if !found.iter().any(|s| s.id == current.id) {
                            found.push(current);
                            found.sort_by_key(|s| s.name.to_lowercase());
                        }
                    }
                    speakers.set(found.iter().map(info).collect());
                    if let Ok(mut cache) = FOUND.lock() {
                        *cache = found;
                    }
                }
                Event::Command(first) => {
                    let Some(current) = live.as_mut() else { continue };
                    let mut batch = vec![first];
                    while let Ok(more) = current.commands.try_recv() {
                        batch.push(more);
                    }
                    for command in fold_commands(batch) {
                        run_command(current, player, command).await;
                    }
                }
                Event::Tick => {
                    let Some(current) = live.as_mut() else { continue };
                    if let Err(err) = current.session.poll().await {
                        eprintln!("[cast] lost the speaker: {err}");
                        if let Some(lost) = live.take() {
                            let name = lost.session.speaker.name.clone();
                            resume_here(player, lost.local_volume, false);
                            player.toast.show(format!("Lost connection to {name}."), true);
                        }
                        active.set(None);
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::fold_commands;
    use crate::components::OutputCommand::*;

    #[test]
    fn runs_of_volume_and_seek_fold_to_the_last() {
        let folded = fold_commands(vec![Volume(0.1), Volume(0.2), Volume(0.3), Pause, Seek(1.0), Seek(9.0), Volume(0.4), Play]);
        assert_eq!(folded, vec![Volume(0.3), Pause, Seek(9.0), Volume(0.4), Play]);
        let load = Load { song_id: "a".into(), position: 0.0, play: true };
        assert_eq!(fold_commands(vec![load.clone(), Volume(0.5)]), vec![load, Volume(0.5)]);
    }
}
