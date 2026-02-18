#[cfg(target_arch = "wasm32")]
#[component]
pub fn AudioController() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();
    let volume = use_context::<VolumeSignal>().0;
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let repeat_mode = use_context::<Signal<RepeatMode>>();
    let shuffle_enabled = use_context::<Signal<bool>>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let mut seek_request = use_context::<SeekRequestSignal>().0;
    let mut audio_state = use_context::<Signal<AudioState>>();
    let preview_playback = use_context::<PreviewPlaybackSignal>().0;

    let mut last_song_id = use_signal(|| None::<String>);
    let mut last_src = use_signal(|| None::<String>);
    let mut last_bookmark = use_signal(|| None::<(String, u64)>);
    let mut last_song_for_bookmark = use_signal(|| None::<Song>);

    thread_local! {
        static USER_INTERACTED: Cell<bool> = Cell::new(false);
    }
    let has_user_interacted = || USER_INTERACTED.with(|c| c.get());

    include!("audio_controller_wasm/setup_and_polling.rs");
    include!("audio_controller_wasm/track_and_control_sync.rs");

    rsx! {}
}
