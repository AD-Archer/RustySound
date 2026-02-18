// Native shared transport metadata plus Windows media backend implementation.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct NativeAudioSnapshot {
    current_time: f64,
    duration: f64,
    paused: bool,
    ended: bool,
    #[serde(default)]
    action: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, Deserialize, Serialize)]
struct NativeTrackMetadata {
    title: String,
    artist: String,
    album: String,
    artwork: Option<String>,
    duration: f64,
    is_live: bool,
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
fn seconds_to_timespan(seconds: f64) -> TimeSpan {
    let clamped = if seconds.is_finite() {
        seconds.max(0.0)
    } else {
        0.0
    };
    TimeSpan {
        Duration: (clamped * 10_000_000.0).round() as i64,
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
fn timespan_to_seconds(span: TimeSpan) -> f64 {
    (span.Duration as f64 / 10_000_000.0).max(0.0)
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
fn push_windows_remote_action(actions: &Arc<Mutex<VecDeque<String>>>, action: &str) {
    if let Ok(mut queue) = actions.lock() {
        queue.push_back(action.to_string());
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
struct WindowsAudioPlayer {
    player: MediaPlayer,
    remote_actions: Arc<Mutex<VecDeque<String>>>,
    ended_flag: Arc<AtomicBool>,
    has_source: bool,
    current_song_id: Option<String>,
    metadata: Option<NativeTrackMetadata>,
    _button_pressed_handler: Option<
        TypedEventHandler<
            SystemMediaTransportControls,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >,
    >,
    _media_ended_handler: Option<TypedEventHandler<MediaPlayer, IInspectable>>,
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
impl WindowsAudioPlayer {
    fn new() -> Option<Self> {
        let player = MediaPlayer::new().ok()?;
        let remote_actions = Arc::new(Mutex::new(VecDeque::new()));
        let ended_flag = Arc::new(AtomicBool::new(false));
        let mut button_pressed_handler: Option<
            TypedEventHandler<
                SystemMediaTransportControls,
                SystemMediaTransportControlsButtonPressedEventArgs,
            >,
        > = None;

        if let Ok(command_manager) = player.CommandManager() {
            let _ = command_manager.SetIsEnabled(false);
        }

        if let Ok(smtc) = player.SystemMediaTransportControls() {
            let _ = smtc.SetIsEnabled(true);
            let _ = smtc.SetIsPlayEnabled(true);
            let _ = smtc.SetIsPauseEnabled(true);
            let _ = smtc.SetIsNextEnabled(true);
            let _ = smtc.SetIsPreviousEnabled(true);
            let _ = smtc.SetPlaybackStatus(MediaPlaybackStatus::Closed);

            let actions = remote_actions.clone();
            let handler: TypedEventHandler<
                SystemMediaTransportControls,
                SystemMediaTransportControlsButtonPressedEventArgs,
            > = TypedEventHandler::new(
                move |_sender,
                      args: windows::core::Ref<
                    '_,
                    SystemMediaTransportControlsButtonPressedEventArgs,
                >| {
                    match args.ok()?.Button()? {
                        SystemMediaTransportControlsButton::Play => {
                            push_windows_remote_action(&actions, "play")
                        }
                        SystemMediaTransportControlsButton::Pause => {
                            push_windows_remote_action(&actions, "pause")
                        }
                        SystemMediaTransportControlsButton::Next => {
                            push_windows_remote_action(&actions, "next")
                        }
                        SystemMediaTransportControlsButton::Previous => {
                            push_windows_remote_action(&actions, "previous")
                        }
                        _ => {}
                    }
                    Ok(())
                },
            );
            let _ = smtc.ButtonPressed(&handler);
            button_pressed_handler = Some(handler);
        }

        let media_ended_handler: TypedEventHandler<MediaPlayer, IInspectable> =
            TypedEventHandler::new({
                let actions = remote_actions.clone();
                let ended = ended_flag.clone();
                move |_, _| {
                    ended.store(true, Ordering::SeqCst);
                    push_windows_remote_action(&actions, "ended");
                    Ok(())
                }
            });
        {
            let _ = player.MediaEnded(&media_ended_handler);
        }

        Some(Self {
            player,
            remote_actions,
            ended_flag,
            has_source: false,
            current_song_id: None,
            metadata: None,
            _button_pressed_handler: button_pressed_handler,
            _media_ended_handler: Some(media_ended_handler),
        })
    }

    fn set_playback_status(&self, status: MediaPlaybackStatus) {
        if let Ok(smtc) = self.player.SystemMediaTransportControls() {
            let _ = smtc.SetPlaybackStatus(status);
        }
    }

    fn apply_metadata(&mut self, meta: Option<NativeTrackMetadata>) {
        self.metadata = meta.clone();
        let Some(meta) = meta else {
            return;
        };

        let Ok(smtc) = self.player.SystemMediaTransportControls() else {
            return;
        };
        let Ok(updater) = smtc.DisplayUpdater() else {
            return;
        };

        let _ = updater.ClearAll();
        let _ = updater.SetType(MediaPlaybackType::Music);
        let _ = updater.SetAppMediaId(&HSTRING::from("RustySound"));
        if let Ok(music) = updater.MusicProperties() {
            let _ = music.SetTitle(&HSTRING::from(meta.title));
            let _ = music.SetArtist(&HSTRING::from(meta.artist));
            let _ = music.SetAlbumTitle(&HSTRING::from(meta.album));
        }
        if let Some(artwork_url) = meta.artwork.as_ref() {
            if let Ok(uri) = Uri::CreateUri(&HSTRING::from(artwork_url)) {
                if let Ok(thumbnail) = RandomAccessStreamReference::CreateFromUri(&uri) {
                    let _ = updater.SetThumbnail(&thumbnail);
                }
            }
        }
        let _ = updater.Update();
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
                let volume = cmd.get("volume").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let should_play = cmd.get("play").and_then(|v| v.as_bool()).unwrap_or(false);
                let position = cmd
                    .get("position")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    .max(0.0);
                let song_id = cmd
                    .get("song_id")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string());
                let metadata = cmd
                    .get("meta")
                    .cloned()
                    .and_then(|value| serde_json::from_value::<NativeTrackMetadata>(value).ok());

                if let Ok(uri) = Uri::CreateUri(&HSTRING::from(src)) {
                    if let Ok(source) = MediaSource::CreateFromUri(&uri) {
                        let _ = self.player.SetSource(&source);
                        self.has_source = true;
                    } else {
                        self.has_source = false;
                    }
                } else {
                    self.has_source = false;
                }

                self.current_song_id = song_id;
                self.ended_flag.store(false, Ordering::SeqCst);
                self.apply_metadata(metadata);
                let _ = self.player.SetVolume(volume.clamp(0.0, 1.0));

                if let Ok(session) = self.player.PlaybackSession() {
                    let _ = session.SetPosition(seconds_to_timespan(position));
                }
                if should_play {
                    let _ = self.player.Play();
                    self.set_playback_status(MediaPlaybackStatus::Playing);
                } else {
                    let _ = self.player.Pause();
                    self.set_playback_status(MediaPlaybackStatus::Paused);
                }
            }
            "play" => {
                let _ = self.player.Play();
                self.ended_flag.store(false, Ordering::SeqCst);
                self.set_playback_status(MediaPlaybackStatus::Playing);
            }
            "pause" => {
                let _ = self.player.Pause();
                self.set_playback_status(MediaPlaybackStatus::Paused);
            }
            "seek" => {
                let position = cmd
                    .get("position")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    .max(0.0);
                if let Ok(session) = self.player.PlaybackSession() {
                    let _ = session.SetPosition(seconds_to_timespan(position));
                }
                self.ended_flag.store(false, Ordering::SeqCst);
            }
            "volume" => {
                let volume = cmd.get("value").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let _ = self.player.SetVolume(volume.clamp(0.0, 1.0));
            }
            "loop" => {
                let enabled = cmd
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let _ = self.player.SetIsLoopingEnabled(enabled);
            }
            "metadata" => {
                let metadata = cmd
                    .get("meta")
                    .cloned()
                    .and_then(|value| serde_json::from_value::<NativeTrackMetadata>(value).ok());
                self.apply_metadata(metadata);
            }
            "clear" => {
                let _ = self.player.Pause();
                self.has_source = false;
                self.current_song_id = None;
                self.metadata = None;
                self.ended_flag.store(false, Ordering::SeqCst);
                if let Ok(mut actions) = self.remote_actions.lock() {
                    actions.clear();
                }
                self.set_playback_status(MediaPlaybackStatus::Closed);
            }
            _ => {}
        }
    }

    fn snapshot(&self) -> NativeAudioSnapshot {
        let action = self
            .remote_actions
            .lock()
            .ok()
            .and_then(|mut actions| actions.pop_front());

        if !self.has_source {
            return NativeAudioSnapshot {
                current_time: 0.0,
                duration: 0.0,
                paused: true,
                ended: self.ended_flag.swap(false, Ordering::SeqCst),
                action,
            };
        }

        let mut current_time = 0.0;
        let mut duration = 0.0;
        let mut paused = true;

        if let Ok(session) = self.player.PlaybackSession() {
            if let Ok(position) = session.Position() {
                current_time = timespan_to_seconds(position);
            }
            if let Ok(natural_duration) = session.NaturalDuration() {
                duration = timespan_to_seconds(natural_duration);
            } else if let Some(meta) = &self.metadata {
                duration = meta.duration.max(0.0);
            }
            paused = session
                .PlaybackState()
                .map(|state| state != MediaPlaybackState::Playing)
                .unwrap_or(true);
        }

        NativeAudioSnapshot {
            current_time,
            duration,
            paused,
            ended: self.ended_flag.swap(false, Ordering::SeqCst),
            action,
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
thread_local! {
    static WINDOWS_AUDIO_PLAYER: RefCell<Option<WindowsAudioPlayer>> = RefCell::new(None);
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
fn with_windows_player<R>(f: impl FnOnce(&mut WindowsAudioPlayer) -> R) -> Option<R> {
    WINDOWS_AUDIO_PLAYER.with(|slot| {
        let mut guard = slot.borrow_mut();
        if guard.is_none() {
            *guard = WindowsAudioPlayer::new();
        }
        guard.as_mut().map(f)
    })
}

