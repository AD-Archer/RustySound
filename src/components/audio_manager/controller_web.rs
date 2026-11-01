#[cfg(target_arch = "wasm32")]
#[component]
pub fn AudioController() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<crate::components::IsPlayingSignal>().0;
    let volume = use_context::<VolumeSignal>().0;
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let repeat_mode = use_context::<Signal<RepeatMode>>();
    let shuffle_enabled = use_context::<crate::components::ShuffleEnabledSignal>().0;
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let seek_request = use_context::<SeekRequestSignal>().0;
    let audio_state = use_context::<Signal<AudioState>>();
    let preview_playback = use_context::<PreviewPlaybackSignal>().0;
    let sleep_timer = use_context::<SleepTimer>();
    // Playback speed from the settings (1x for radio).
    use_playback_rate_sync(app_settings, now_playing);
    // Volume levelling (ReplayGain) on the way out; VolumeSignal stays the user's.
    use_volume_levelling(app_settings, now_playing, queue, queue_index, volume);
    // The equalizer (Web Audio on Mac and web, a processing tap on iPhone).
    use_equalizer_sync(app_settings);
    // Autoplay: similar songs join the queue before it runs out.
    use_autoplay(servers, app_settings, queue, queue_index, now_playing, repeat_mode);

    let last_song_id = use_signal(|| None::<String>);
    let last_src = use_signal(|| None::<String>);
    let last_bookmark = use_signal(|| None::<(String, u64)>);
    let last_song_for_bookmark = use_signal(|| None::<Song>);

    thread_local! {
        static USER_INTERACTED: Cell<bool> = Cell::new(false);
    }
    let has_user_interacted =
        || USER_INTERACTED.with(|c| c.get()) || web_has_sticky_activation();

    include!("audio_controller_wasm/setup_and_polling.rs");
    include!("audio_controller_wasm/track_and_control_sync.rs");

    rsx! {}
}
