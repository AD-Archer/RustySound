//! The clock Now Playing follows: this device's playback, or, while another
//! device in the sync room plays, that device's position (ticked locally
//! from its last `state`, nothing sent over the network). Lyrics, ambient
//! mode and Lyrics Shot read it, so their lines follow whichever plays.

use super::actions::{use_player, PlayerCtx};
use crate::api::Song;
use crate::components::remote_sync::protocol::Op;
use crate::components::remote_sync::RemoteSync;
use crate::components::ui::env::sleep_ms;
use crate::components::AudioState;
use dioxus::prelude::*;

/// How often the remote position redraws while the sheet is open.
const REMOTE_TICK_MS: u64 = 250;

#[derive(Clone, Copy)]
pub(crate) struct NowPlayingClock {
    /// Seconds into the song.
    pub time: Memo<f64>,
    pub playing: Memo<bool>,
    /// Another device plays (the sheet shows its song).
    pub remote: Memo<bool>,
    /// That device, when it takes commands (`None` for "Last played on").
    pub target: Memo<Option<String>>,
    sync: RemoteSync,
    player: PlayerCtx,
}

impl NowPlayingClock {
    /// Seeks whichever device plays, to `seconds`.
    pub fn seek(&self, seconds: f64) {
        let seconds = seconds.max(0.0);
        if *self.remote.peek() {
            if let Some(target) = self.target.peek().clone() {
                self.sync.send(&target, Op::Seek { ms: (seconds * 1000.0) as u64 });
            }
        } else {
            let mut player = self.player;
            player.seek(seconds);
        }
    }
}

/// Provides the clock for the sheet. `remote_song` is the other device's
/// song (for its duration), read only while it plays.
pub(super) fn use_now_playing_clock_provider(remote_song: Memo<Option<Song>>) -> NowPlayingClock {
    let sync = use_context::<RemoteSync>();
    let audio = use_context::<Signal<AudioState>>();
    let player = use_player();
    // Another device's playback, unless this one is playing: a device the
    // room still marks active must not pull this device's lyrics off time.
    let remote = use_memo(move || sync.view().is_some() && !(player.is_playing)());
    let target = use_memo(move || sync.view().and_then(|view| view.target().map(str::to_string)));
    let mut remote_time = use_signal(|| 0.0f64);
    use_future(move || async move {
        loop {
            if *remote.peek() {
                let duration_ms = remote_song
                    .peek()
                    .as_ref()
                    .map(|song| song.duration as u64 * 1000)
                    .filter(|ms| *ms > 0);
                let seconds = sync.live_position_ms(duration_ms).unwrap_or(0) as f64 / 1000.0;
                if (*remote_time.peek() - seconds).abs() > 0.01 {
                    remote_time.set(seconds);
                }
            }
            sleep_ms(REMOTE_TICK_MS).await;
        }
    });
    let local_time = audio.peek().current_time;
    let time = use_memo(move || if remote() { remote_time() } else { local_time() });
    let playing = use_memo(move || {
        if remote() {
            sync.view().and_then(|view| view.state().map(|s| s.state.playing)).unwrap_or(false)
        } else {
            (player.is_playing)()
        }
    });
    use_context_provider(|| NowPlayingClock { time, playing, remote, target, sync, player })
}

/// The sheet's clock, else (outside the sheet) local playback.
pub(crate) fn use_clock_time() -> ReadSignal<f64> {
    let audio = use_context::<Signal<AudioState>>();
    use_hook(move || match try_consume_context::<NowPlayingClock>() {
        Some(clock) => clock.time.into(),
        None => audio.peek().current_time.into(),
    })
}

/// Whether whichever device the sheet follows is playing.
pub(crate) fn use_clock_playing() -> ReadSignal<bool> {
    let player = use_player();
    use_hook(move || match try_consume_context::<NowPlayingClock>() {
        Some(clock) => clock.playing.into(),
        None => player.is_playing.into(),
    })
}
