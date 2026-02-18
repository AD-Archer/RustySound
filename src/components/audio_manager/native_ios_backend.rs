// iOS AVPlayer backend core implementation.
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
#[repr(C)]
#[derive(Copy, Clone, Default)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    fn CMTimeMakeWithSeconds(seconds: f64, preferred_timescale: i32) -> CMTime;
    fn CMTimeGetSeconds(time: CMTime) -> f64;
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
#[link(name = "MediaPlayer", kind = "framework")]
unsafe extern "C" {}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
unsafe extern "C" {
    static MPMediaItemPropertyTitle: *mut Object;
    static MPMediaItemPropertyArtist: *mut Object;
    static MPMediaItemPropertyAlbumTitle: *mut Object;
    static MPMediaItemPropertyArtwork: *mut Object;
    static MPMediaItemPropertyPlaybackDuration: *mut Object;
    static MPNowPlayingInfoPropertyElapsedPlaybackTime: *mut Object;
    static MPNowPlayingInfoPropertyPlaybackRate: *mut Object;
    static MPNowPlayingInfoPropertyDefaultPlaybackRate: *mut Object;
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
struct IosAudioPlayer {
    player: *mut Object,
    current_song_id: Option<String>,
    metadata: Option<NativeTrackMetadata>,
    ended_sent_for_song: Option<String>,
    last_known_elapsed: f64,
    last_known_duration: f64,
    pending_seek_target: Option<f64>,
    pending_seek_ticks: u8,
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
unsafe impl Send for IosAudioPlayer {}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
impl Drop for IosAudioPlayer {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.player, pause];
            let _: () = msg_send![self.player, release];
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
impl IosAudioPlayer {
    fn new() -> Option<Self> {
        configure_ios_audio_session();
        configure_ios_remote_commands();
        let player = unsafe {
            let player_cls = class!(AVPlayer);
            let player_alloc: *mut Object = msg_send![player_cls, alloc];
            if player_alloc.is_null() {
                return None;
            }
            let player: *mut Object = msg_send![player_alloc, init];
            if player.is_null() {
                return None;
            }
            player
        };

        Some(Self {
            player,
            current_song_id: None,
            metadata: None,
            ended_sent_for_song: None,
            last_known_elapsed: 0.0,
            last_known_duration: 0.0,
            pending_seek_target: None,
            pending_seek_ticks: 0,
        })
    }

    fn apply(&mut self, cmd: serde_json::Value) {
        let Some(cmd_type) = cmd.get("type").and_then(|v| v.as_str()) else {
            return;
        };

        match cmd_type {
            "load" => {
                let src = cmd.get("src").and_then(|v| v.as_str()).unwrap_or_default();
                if src.is_empty() {
                    return;
                }

                let position = cmd
                    .get("position")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    .max(0.0);
                let volume = cmd.get("volume").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let should_play = cmd.get("play").and_then(|v| v.as_bool()).unwrap_or(false);
                let song_id = cmd
                    .get("song_id")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string());
                let metadata = cmd
                    .get("meta")
                    .cloned()
                    .and_then(|value| serde_json::from_value::<NativeTrackMetadata>(value).ok());

                unsafe {
                    if let Some(item) = make_player_item(src) {
                        let _: () = msg_send![self.player, replaceCurrentItemWithPlayerItem: item];
                        observe_ios_item_end(item);
                        let _: () =
                            msg_send![self.player, setVolume: volume.clamp(0.0, 1.0) as f32];
                        self.seek(position);
                        if should_play {
                            let _: () = msg_send![self.player, play];
                        } else {
                            let _: () = msg_send![self.player, pause];
                        }
                    }
                }

                self.current_song_id = song_id;
                self.metadata = metadata;
                self.ended_sent_for_song = None;
                self.last_known_elapsed = position;
                self.last_known_duration = self
                    .metadata
                    .as_ref()
                    .map(|m| m.duration)
                    .unwrap_or(0.0)
                    .max(0.0);
                self.pending_seek_target = Some(position);
                self.pending_seek_ticks = 20;
                self.update_now_playing_info_cached(if should_play { 1.0 } else { 0.0 });
            }
            "play" => unsafe {
                let _: () = msg_send![self.player, play];
                self.update_now_playing_info_cached(1.0);
            },
            "pause" => unsafe {
                let _: () = msg_send![self.player, pause];
                self.update_now_playing_info_cached(0.0);
            },
            "seek" => {
                let target = cmd
                    .get("position")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    .max(0.0);
                self.seek(target);
                self.ended_sent_for_song = None;
                self.last_known_elapsed = target;
                self.pending_seek_target = Some(target);
                self.pending_seek_ticks = 20;
                let rate: f32 = unsafe { msg_send![self.player, rate] };
                self.update_now_playing_info_cached(if rate > 0.0 { 1.0 } else { 0.0 });
            }
            "volume" => {
                let volume = cmd.get("value").and_then(|v| v.as_f64()).unwrap_or(1.0);
                unsafe {
                    let _: () = msg_send![self.player, setVolume: volume.clamp(0.0, 1.0) as f32];
                }
            }
            "metadata" => {
                self.metadata = cmd
                    .get("meta")
                    .cloned()
                    .and_then(|value| serde_json::from_value::<NativeTrackMetadata>(value).ok());
                self.last_known_duration = self
                    .metadata
                    .as_ref()
                    .map(|m| m.duration)
                    .unwrap_or(self.last_known_duration)
                    .max(0.0);
                let rate: f32 = unsafe { msg_send![self.player, rate] };
                self.update_now_playing_info_cached(if rate > 0.0 { 1.0 } else { 0.0 });
            }
            "clear" => {
                unsafe {
                    let _: () = msg_send![self.player, pause];
                    let nil_item: *mut Object = ptr::null_mut();
                    let _: () = msg_send![self.player, replaceCurrentItemWithPlayerItem: nil_item];
                }
                self.current_song_id = None;
                self.metadata = None;
                self.ended_sent_for_song = None;
                self.last_known_elapsed = 0.0;
                self.last_known_duration = 0.0;
                self.pending_seek_target = None;
                self.pending_seek_ticks = 0;
                observe_ios_item_end(ptr::null_mut());
                clear_ios_now_playing_info();
            }
            "loop" => {}
            _ => {}
        }
    }

    fn seek(&self, position: f64) {
        unsafe {
            let time = CMTimeMakeWithSeconds(position.max(0.0), 1000);
            let _: () = msg_send![self.player, seekToTime: time];
        }
    }

    fn snapshot(&mut self) -> NativeAudioSnapshot {
        let (current_time, duration) = self.current_time_and_duration();

        let paused = unsafe {
            let rate: f32 = msg_send![self.player, rate];
            rate <= 0.0
        };

        // Track metadata and AVPlayer timing can disagree by a few seconds.
        // Treat "near end + paused" as ended to avoid getting stuck at ~N-4s.
        let end_tolerance = if duration > 0.0 {
            (duration * 0.02).clamp(0.35, 5.0)
        } else {
            0.35
        };
        let ended = duration > 0.0 && current_time >= (duration - end_tolerance).max(0.0) && paused;
        let mut action = pop_ios_remote_action();

        if ended {
            let current_song = self.current_song_id.clone();
            if self.ended_sent_for_song != current_song {
                self.ended_sent_for_song = current_song;
                if action.is_none() {
                    action = Some("ended".to_string());
                }
            }
        } else {
            self.ended_sent_for_song = None;
        }

        // Keep lock-screen/command-center progress fresh while preserving
        // monotonic time through transient AVPlayer glitches.
        self.update_now_playing_info();

        NativeAudioSnapshot {
            current_time,
            duration,
            paused,
            ended,
            action,
        }
    }

    fn current_time_and_duration(&mut self) -> (f64, f64) {
        let mut current_time = unsafe {
            let current: CMTime = msg_send![self.player, currentTime];
            cmtime_seconds(current)
        };
        let playing = unsafe {
            let rate: f32 = msg_send![self.player, rate];
            rate > 0.0
        };

        let mut duration = unsafe {
            let current_item: *mut Object = msg_send![self.player, currentItem];
            if current_item.is_null() {
                0.0
            } else {
                let duration: CMTime = msg_send![current_item, duration];
                cmtime_seconds(duration)
            }
        };

        if duration <= 0.0 {
            if self.last_known_duration.is_finite() && self.last_known_duration > 0.0 {
                duration = self.last_known_duration;
            }
        }

        if duration <= 0.0 {
            if let Some(meta) = &self.metadata {
                if meta.duration.is_finite() && meta.duration > 0.0 {
                    duration = meta.duration;
                }
            }
        }

        if duration > 0.0 {
            current_time = current_time.min(duration);
        }

        if let Some(target) = self.pending_seek_target {
            let clamped_target = if duration > 0.0 {
                target.min(duration)
            } else {
                target.max(0.0)
            };
            let close_enough = (current_time - clamped_target).abs() <= 1.5;
            if !close_enough && self.pending_seek_ticks > 0 {
                current_time = clamped_target;
                self.last_known_elapsed = clamped_target.max(0.0);
                self.pending_seek_ticks = self.pending_seek_ticks.saturating_sub(1);
            } else {
                self.pending_seek_target = None;
                self.pending_seek_ticks = 0;
            }
        }

        // AVPlayer can briefly report 0 during app/background transitions.
        // Preserve elapsed position unless we have a trustworthy newer value.
        if current_time <= 0.05 && self.last_known_elapsed > 0.25 {
            current_time = if duration > 0.0 {
                self.last_known_elapsed.min(duration)
            } else {
                self.last_known_elapsed
            };
        } else if playing
            && self.last_known_elapsed > 1.0
            && current_time + 1.5 < self.last_known_elapsed
        {
            // Ignore abrupt backwards jumps while actively playing.
            current_time = if duration > 0.0 {
                self.last_known_elapsed.min(duration)
            } else {
                self.last_known_elapsed
            };
        } else if !playing
            && self.last_known_elapsed > 1.0
            && current_time + 1.5 < self.last_known_elapsed
        {
            // At track boundaries AVPlayer may briefly rewind a few seconds.
            // Keep the latest known position to prevent visible progress rollback.
            current_time = if duration > 0.0 {
                self.last_known_elapsed.min(duration)
            } else {
                self.last_known_elapsed
            };
        } else {
            self.last_known_elapsed = if duration > 0.0 {
                current_time.min(duration).max(0.0)
            } else {
                current_time.max(0.0)
            };
        }

        if duration.is_finite() && duration > 0.0 {
            self.last_known_duration = duration;
        }

        (current_time, duration)
    }

    fn update_now_playing_info_cached(&mut self, rate: f64) {
        let Some(meta) = self.metadata.clone() else {
            clear_ios_now_playing_info();
            return;
        };

        let mut duration = self.last_known_duration;
        if !(duration.is_finite() && duration > 0.0)
            && meta.duration.is_finite()
            && meta.duration > 0.0
        {
            duration = meta.duration;
        }

        let mut elapsed = self.last_known_elapsed.max(0.0);
        if duration.is_finite() && duration > 0.0 {
            elapsed = elapsed.min(duration);
            self.last_known_duration = duration;
        }

        set_ios_now_playing_info(&meta, elapsed, duration.max(0.0), rate.max(0.0));
    }

    fn update_now_playing_info(&mut self) {
        let Some(meta) = self.metadata.clone() else {
            clear_ios_now_playing_info();
            return;
        };

        let (elapsed, duration) = self.current_time_and_duration();
        let paused = unsafe {
            let rate: f32 = msg_send![self.player, rate];
            rate <= 0.0
        };
        set_ios_now_playing_info(&meta, elapsed, duration, if paused { 0.0 } else { 1.0 });
    }
}

