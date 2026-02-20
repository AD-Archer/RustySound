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
    static MPNowPlayingInfoPropertyPlaybackQueueIndex: *mut Object;
    static MPNowPlayingInfoPropertyPlaybackQueueCount: *mut Object;
    static MPNowPlayingInfoPropertyIsLiveStream: *mut Object;
    static MPNowPlayingInfoPropertyMediaType: *mut Object;
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
struct IosAudioPlayer {
    player: *mut Object,
    now_playing_session: *mut Object,
    current_song_id: Option<String>,
    metadata: Option<NativeTrackMetadata>,
    ended_sent_for_song: Option<String>,
    last_known_elapsed: f64,
    last_known_duration: f64,
    pending_seek_target: Option<f64>,
    pending_seek_ticks: u8,
    now_playing_artwork: *mut Object,
    now_playing_artwork_url: Option<String>,
    last_snapshot_paused: Option<bool>,
    last_snapshot_ended: Option<bool>,
    last_snapshot_log_ms: u128,
    last_time_guard_code: u8,
    last_progress_sample: Option<f64>,
    near_end_stall_ticks: u8,
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
unsafe impl Send for IosAudioPlayer {}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
impl Drop for IosAudioPlayer {
    fn drop(&mut self) {
        self.clear_cached_artwork();
        unsafe {
            let _: () = msg_send![self.player, pause];
            let _: () = msg_send![self.player, release];
            if !self.now_playing_session.is_null() {
                let _: () = msg_send![self.now_playing_session, release];
            }
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
impl IosAudioPlayer {
    fn new() -> Option<Self> {
        configure_ios_audio_session();
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
        configure_ios_remote_commands(player);

        Some(Self {
            player,
            now_playing_session: get_ios_now_playing_session(),
            current_song_id: None,
            metadata: None,
            ended_sent_for_song: None,
            last_known_elapsed: 0.0,
            last_known_duration: 0.0,
            pending_seek_target: None,
            pending_seek_ticks: 0,
            now_playing_artwork: ptr::null_mut(),
            now_playing_artwork_url: None,
            last_snapshot_paused: None,
            last_snapshot_ended: None,
            last_snapshot_log_ms: 0,
            last_time_guard_code: u8::MAX,
            last_progress_sample: None,
            near_end_stall_ticks: 0,
        })
    }

    fn apply(&mut self, cmd: serde_json::Value) {
        let Some(cmd_type) = cmd.get("type").and_then(|v| v.as_str()) else {
            return;
        };
        ios_diag_log("player.apply", &format!("type={cmd_type}"));

        match cmd_type {
            "load" => {
                let src = cmd.get("src").and_then(|v| v.as_str()).unwrap_or_default();
                if src.is_empty() {
                    ios_diag_log("player.load", "empty src");
                    return;
                }

                let position = cmd
                    .get("position")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    .max(0.0);
                let volume = cmd
                    .get("volume")
                    .and_then(|v| v.as_f64())
                    .unwrap_or_else(|| unsafe {
                        let current_volume: f32 = msg_send![self.player, volume];
                        current_volume as f64
                    });
                let should_play = cmd.get("play").and_then(|v| v.as_bool()).unwrap_or(false);
                let song_id = cmd
                    .get("song_id")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string());
                let metadata = cmd
                    .get("meta")
                    .cloned()
                    .and_then(|value| serde_json::from_value::<NativeTrackMetadata>(value).ok());
                ios_diag_log(
                    "player.load",
                    &format!(
                        "song_id={:?} play={} position={position:.3} volume={:.3} src_prefix={}",
                        song_id,
                        should_play,
                        volume.clamp(0.0, 1.0),
                        src.chars().take(80).collect::<String>()
                    ),
                );

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
                    } else {
                        ios_diag_log("player.load", "failed to create AVPlayerItem");
                    }
                }

                self.current_song_id = song_id;
                ios_plan_sync_current_song(self.current_song_id.as_deref());
                self.metadata = metadata;
                self.refresh_cached_artwork();
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
                self.last_progress_sample = Some(position.max(0.0));
                self.near_end_stall_ticks = 0;
                set_ios_remote_transport_state(should_play);
                self.update_now_playing_info_cached(if should_play { 1.0 } else { 0.0 });
                self.log_player_diagnostics("after-load", Some(position), None);
            }
            "play" => unsafe {
                let _: () = msg_send![self.player, play];
                self.near_end_stall_ticks = 0;
                set_ios_remote_transport_state(true);
                self.update_now_playing_info_cached(1.0);
                ios_diag_log("player.transport", "play");
                self.log_player_diagnostics("after-play", None, None);
            },
            "pause" => unsafe {
                let _: () = msg_send![self.player, pause];
                self.near_end_stall_ticks = 0;
                set_ios_remote_transport_state(false);
                self.update_now_playing_info_cached(0.0);
                ios_diag_log("player.transport", "pause");
                self.log_player_diagnostics("after-pause", None, None);
            },
            "seek" => {
                let target = cmd
                    .get("position")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    .max(0.0);
                ios_diag_log("player.transport", &format!("seek target={target:.3}"));
                self.seek(target);
                self.ended_sent_for_song = None;
                self.last_known_elapsed = target;
                self.pending_seek_target = Some(target);
                self.pending_seek_ticks = 20;
                self.last_progress_sample = Some(target.max(0.0));
                self.near_end_stall_ticks = 0;
                let rate: f32 = unsafe { msg_send![self.player, rate] };
                self.update_now_playing_info_cached(if rate > 0.0 { 1.0 } else { 0.0 });
                self.log_player_diagnostics("after-seek", Some(target), None);
            }
            "volume" => {
                let volume = cmd.get("value").and_then(|v| v.as_f64()).unwrap_or(1.0);
                unsafe {
                    let _: () = msg_send![self.player, setVolume: volume.clamp(0.0, 1.0) as f32];
                }
                ios_diag_log(
                    "player.transport",
                    &format!("volume={:.3}", volume.clamp(0.0, 1.0)),
                );
            }
            "metadata" => {
                self.metadata = cmd
                    .get("meta")
                    .cloned()
                    .and_then(|value| serde_json::from_value::<NativeTrackMetadata>(value).ok());
                self.refresh_cached_artwork();
                self.last_known_duration = self
                    .metadata
                    .as_ref()
                    .map(|m| m.duration)
                    .unwrap_or(self.last_known_duration)
                    .max(0.0);
                let rate: f32 = unsafe { msg_send![self.player, rate] };
                self.update_now_playing_info_cached(if rate > 0.0 { 1.0 } else { 0.0 });
                ios_diag_log(
                    "player.metadata",
                    &format!(
                        "title={} duration={:.3}",
                        self.metadata
                            .as_ref()
                            .map(|meta| meta.title.as_str())
                            .unwrap_or(""),
                        self.last_known_duration
                    ),
                );
            }
            "clear" => {
                unsafe {
                    let _: () = msg_send![self.player, pause];
                    let nil_item: *mut Object = ptr::null_mut();
                    let _: () = msg_send![self.player, replaceCurrentItemWithPlayerItem: nil_item];
                }
                self.current_song_id = None;
                ios_plan_sync_current_song(None);
                self.metadata = None;
                self.ended_sent_for_song = None;
                self.last_known_elapsed = 0.0;
                self.last_known_duration = 0.0;
                self.pending_seek_target = None;
                self.pending_seek_ticks = 0;
                self.last_progress_sample = None;
                self.near_end_stall_ticks = 0;
                set_ios_remote_transport_state(false);
                self.clear_cached_artwork();
                observe_ios_item_end(ptr::null_mut());
                clear_ios_now_playing_info();
                ios_diag_log("player.transport", "clear");
                self.log_player_diagnostics("after-clear", Some(0.0), Some(0.0));
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

    fn time_control_status_name(status: i64) -> &'static str {
        match status {
            0 => "paused",
            1 => "waiting",
            2 => "playing",
            _ => "unknown",
        }
    }

    fn item_status_name(status: i64) -> &'static str {
        match status {
            0 => "unknown",
            1 => "ready",
            2 => "failed",
            _ => "n/a",
        }
    }

    fn log_player_diagnostics(
        &self,
        reason: &str,
        sampled_time: Option<f64>,
        sampled_duration: Option<f64>,
    ) {
        unsafe {
            let rate: f32 = msg_send![self.player, rate];
            let time_control_status_raw: isize = msg_send![self.player, timeControlStatus];
            let time_control_status = time_control_status_raw as i64;
            let waiting_reason_obj: *mut Object = msg_send![self.player, reasonForWaitingToPlay];
            let waiting_reason = ns_string_to_rust(waiting_reason_obj)
                .unwrap_or_else(|| "none".to_string())
                .replace('\n', " ");

            let current_item: *mut Object = msg_send![self.player, currentItem];
            let mut elapsed = sampled_time.unwrap_or_else(|| {
                let current: CMTime = msg_send![self.player, currentTime];
                cmtime_seconds(current)
            });
            let mut duration = sampled_duration.unwrap_or(0.0);

            let (
                item_status,
                playback_likely_to_keep_up,
                playback_buffer_empty,
                playback_buffer_full,
                item_error,
            ) = if current_item.is_null() {
                (-1, false, false, false, "none".to_string())
            } else {
                let status_raw: isize = msg_send![current_item, status];
                let likely_to_keep_up: BOOL = msg_send![current_item, isPlaybackLikelyToKeepUp];
                let buffer_empty: BOOL = msg_send![current_item, isPlaybackBufferEmpty];
                let buffer_full: BOOL = msg_send![current_item, isPlaybackBufferFull];
                let error_obj: *mut Object = msg_send![current_item, error];
                let error_text = if error_obj.is_null() {
                    "none".to_string()
                } else {
                    let description: *mut Object = msg_send![error_obj, localizedDescription];
                    ns_string_to_rust(description)
                        .unwrap_or_else(|| "present".to_string())
                        .replace('\n', " ")
                };
                if duration <= 0.0 {
                    let item_duration: CMTime = msg_send![current_item, duration];
                    duration = cmtime_seconds(item_duration);
                }
                (
                    status_raw as i64,
                    likely_to_keep_up == YES,
                    buffer_empty == YES,
                    buffer_full == YES,
                    error_text,
                )
            };

            if duration > 0.0 {
                elapsed = elapsed.min(duration).max(0.0);
            } else {
                elapsed = elapsed.max(0.0);
                duration = duration.max(0.0);
            }

            ios_diag_log(
                "player.av",
                &format!(
                    "reason={reason} song_id={:?} rate={rate:.3} time_ctrl={time_control_status}({}) wait_reason={} item_status={item_status}({}) keep_up={} buffer_empty={} buffer_full={} error={} elapsed={elapsed:.3} duration={duration:.3}",
                    self.current_song_id,
                    Self::time_control_status_name(time_control_status),
                    waiting_reason,
                    Self::item_status_name(item_status),
                    playback_likely_to_keep_up,
                    playback_buffer_empty,
                    playback_buffer_full,
                    item_error
                ),
            );
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
        let near_end_window = if duration > 0.0 {
            (duration * 0.015).clamp(0.75, 3.0)
        } else {
            0.75
        };
        let near_end = duration > 0.0 && current_time >= (duration - near_end_window).max(0.0);
        let stalled_near_end = if !paused && near_end {
            let delta = self
                .last_progress_sample
                .map(|previous| (current_time - previous).abs())
                .unwrap_or(f64::INFINITY);
            if delta <= 0.01 {
                self.near_end_stall_ticks = self.near_end_stall_ticks.saturating_add(1);
            } else {
                self.near_end_stall_ticks = 0;
            }
            if self.near_end_stall_ticks == 8 {
                ios_diag_log(
                    "player.ended",
                    &format!(
                        "forcing ended from near-end stall time={current_time:.3} duration={duration:.3}"
                    ),
                );
            }
            self.near_end_stall_ticks >= 8
        } else {
            self.near_end_stall_ticks = 0;
            false
        };
        self.last_progress_sample = Some(current_time);

        let ended = duration > 0.0
            && current_time >= (duration - end_tolerance).max(0.0)
            && (paused || stalled_near_end);
        let mut action = pop_ios_remote_action();

        if matches!(action.as_deref(), Some("ended")) {
            // iOS can occasionally deliver a delayed ended notification after a source switch.
            // Only trust ended actions when playback is plausibly at the end of the current item.
            let near_end_for_action = if duration > 0.0 {
                current_time >= (duration - (end_tolerance * 2.0)).max(0.0)
            } else {
                true
            };
            if !near_end_for_action {
                ios_diag_log(
                    "player.ended",
                    &format!(
                        "ignored stale ended notification time={current_time:.3} duration={duration:.3}"
                    ),
                );
                action = None;
            }
        }

        if ended {
            let current_song = self.current_song_id.clone();
            if self.ended_sent_for_song != current_song {
                self.ended_sent_for_song = current_song;
                if action.is_none() {
                    action = Some("ended".to_string());
                }
                ios_diag_log(
                    "player.ended",
                    &format!("song_id={:?} time={current_time:.3} duration={duration:.3}", self.current_song_id),
                );
            }
        } else {
            self.ended_sent_for_song = None;
        }

        let now_ms = ios_diag_now_ms();
        let heartbeat_due =
            self.last_snapshot_log_ms == 0 || now_ms.saturating_sub(self.last_snapshot_log_ms) >= 5000;
        let state_changed = self.last_snapshot_paused != Some(paused)
            || self.last_snapshot_ended != Some(ended);
        if heartbeat_due || state_changed || action.is_some() {
            ios_diag_log(
                "player.snapshot",
                &format!(
                    "time={current_time:.3} duration={duration:.3} paused={paused} ended={ended} action={:?} queued_actions={}",
                    action,
                    ios_remote_queue_len()
                ),
            );
            self.log_player_diagnostics("snapshot", Some(current_time), Some(duration));
            self.last_snapshot_log_ms = now_ms;
            self.last_snapshot_paused = Some(paused);
            self.last_snapshot_ended = Some(ended);
        }

        // Keep lock-screen/command-center progress in sync with the sampled
        // player snapshot for this poll tick.
        self.update_now_playing_info_from_snapshot(current_time, duration, paused);

        NativeAudioSnapshot {
            current_time,
            duration,
            paused,
            ended,
            action,
            song_id: self.current_song_id.clone(),
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
        let mut time_guard_code = 0u8;
        if current_time <= 0.05 && self.last_known_elapsed > 0.25 {
            time_guard_code = 1;
            current_time = if duration > 0.0 {
                self.last_known_elapsed.min(duration)
            } else {
                self.last_known_elapsed
            };
        } else if playing
            && self.last_known_elapsed > 1.0
            && current_time + 1.5 < self.last_known_elapsed
        {
            time_guard_code = 2;
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
            time_guard_code = 3;
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

        if time_guard_code != self.last_time_guard_code {
            self.last_time_guard_code = time_guard_code;
            ios_diag_log(
                "player.time-guard",
                &format!(
                    "code={time_guard_code} current={current_time:.3} last={:.3} duration={duration:.3} playing={playing}",
                    self.last_known_elapsed
                ),
            );
            self.log_player_diagnostics("time-guard", Some(current_time), Some(duration));
        }

        if duration.is_finite() && duration > 0.0 {
            self.last_known_duration = duration;
        }

        (current_time, duration)
    }

    fn update_now_playing_info_cached(&mut self, rate: f64) {
        let mut duration = self.last_known_duration;

        // When app polling is throttled in background, `last_known_elapsed` can lag behind
        // real AVPlayer time. Sample live timing here so lock-screen progress does not jump
        // backwards (or to 0) on play/pause/metadata updates.
        let live_elapsed = unsafe {
            let current: CMTime = msg_send![self.player, currentTime];
            cmtime_seconds(current)
        };
        let live_duration = unsafe {
            let current_item: *mut Object = msg_send![self.player, currentItem];
            if current_item.is_null() {
                0.0
            } else {
                let item_duration: CMTime = msg_send![current_item, duration];
                cmtime_seconds(item_duration)
            }
        };
        if live_duration.is_finite() && live_duration > 0.0 {
            duration = live_duration;
        }

        let mut elapsed = self.last_known_elapsed.max(0.0);
        if live_elapsed.is_finite() && live_elapsed >= 0.0 {
            let mut candidate = live_elapsed;
            if duration.is_finite() && duration > 0.0 {
                candidate = candidate.min(duration);
            }
            // Guard against transient AVPlayer rewinds while still accepting forward progress.
            let backwards_glitch =
                elapsed > 1.0 && candidate + 1.5 < elapsed && candidate <= 0.05;
            if !backwards_glitch && (candidate > elapsed || elapsed <= 0.05) {
                elapsed = candidate;
            }
        }

        if !(duration.is_finite() && duration > 0.0) {
            if let Some(meta) = self.metadata.as_ref() {
                if meta.duration.is_finite() && meta.duration > 0.0 {
                    duration = meta.duration;
                }
            }
        }

        if duration.is_finite() && duration > 0.0 {
            elapsed = elapsed.min(duration);
            self.last_known_duration = duration;
        }
        self.last_known_elapsed = elapsed.max(0.0);

        let Some(meta) = self.metadata.clone() else {
            return;
        };

        set_ios_now_playing_info(
            &meta,
            elapsed,
            duration.max(0.0),
            rate.max(0.0),
            self.now_playing_artwork,
        );
    }

    fn update_now_playing_info_from_snapshot(
        &mut self,
        elapsed: f64,
        duration: f64,
        paused: bool,
    ) {
        let Some(meta) = self.metadata.clone() else {
            clear_ios_now_playing_info();
            return;
        };

        let mut bounded_elapsed = elapsed.max(0.0);
        let mut bounded_duration = duration.max(0.0);
        if !bounded_duration.is_finite() {
            bounded_duration = 0.0;
        }
        if bounded_duration > 0.0 {
            bounded_elapsed = bounded_elapsed.min(bounded_duration);
            self.last_known_duration = bounded_duration;
        }
        self.last_known_elapsed = bounded_elapsed;

        set_ios_now_playing_info(
            &meta,
            bounded_elapsed,
            bounded_duration,
            if paused { 0.0 } else { 1.0 },
            self.now_playing_artwork,
        );
    }

    fn clear_cached_artwork(&mut self) {
        unsafe {
            if !self.now_playing_artwork.is_null() {
                let _: () = msg_send![self.now_playing_artwork, release];
                self.now_playing_artwork = ptr::null_mut();
            }
        }
        self.now_playing_artwork_url = None;
    }

    fn refresh_cached_artwork(&mut self) {
        let artwork_url = self
            .metadata
            .as_ref()
            .and_then(|meta| meta.artwork.clone())
            .filter(|url| !url.trim().is_empty());

        if artwork_url == self.now_playing_artwork_url {
            return;
        }

        self.clear_cached_artwork();
        self.now_playing_artwork_url = artwork_url.clone();

        if let Some(url) = artwork_url {
            if let Some(artwork) = make_now_playing_artwork(&url) {
                self.now_playing_artwork = artwork;
                ios_diag_log("player.artwork", "cached now-playing artwork");
            } else {
                ios_diag_log("player.artwork", "failed to cache now-playing artwork");
            }
        }
    }
}
