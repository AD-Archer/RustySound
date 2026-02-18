#[cfg(not(target_arch = "wasm32"))]
#[component]
pub fn AudioController() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<Signal<bool>>();
    let volume = use_context::<VolumeSignal>().0;
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let repeat_mode = use_context::<Signal<RepeatMode>>();
    let shuffle_enabled = use_context::<Signal<bool>>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let seek_request = use_context::<SeekRequestSignal>().0;
    let audio_state = use_context::<Signal<AudioState>>();
    let preview_playback = use_context::<PreviewPlaybackSignal>().0;

    let last_song_id = use_signal(|| None::<String>);
    let last_src = use_signal(|| None::<String>);
    let last_bookmark = use_signal(|| None::<(String, u64)>);
    let last_song_for_bookmark = use_signal(|| None::<Song>);
    let last_ended_song = use_signal(|| None::<String>);
    let repeat_one_replayed_song = use_signal(|| None::<String>);

    include!("audio_controller_native/polling_and_remote_actions.rs");
    include!("audio_controller_native/track_and_queue_sync.rs");
    include!("audio_controller_native/playback_state_and_bookmarks.rs");

    rsx! {}
}
