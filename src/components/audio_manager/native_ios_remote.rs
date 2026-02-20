// iOS remote-command center hooks and now-playing metadata synchronization.
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
static IOS_AUDIO_PLAYER: Lazy<Mutex<Option<IosAudioPlayer>>> = Lazy::new(|| Mutex::new(None));
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
static IOS_REMOTE_ACTIONS: Lazy<Mutex<VecDeque<String>>> =
    Lazy::new(|| Mutex::new(VecDeque::new()));
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
static IOS_REMOTE_INIT: Once = Once::new();
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
static IOS_REMOTE_OBSERVER: Lazy<Mutex<usize>> = Lazy::new(|| Mutex::new(0));
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
static IOS_REMOTE_CENTER: Lazy<Mutex<usize>> = Lazy::new(|| Mutex::new(0));
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
static IOS_NOW_PLAYING_CENTER: Lazy<Mutex<usize>> = Lazy::new(|| Mutex::new(0));
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
static IOS_NOW_PLAYING_SESSION: Lazy<Mutex<usize>> = Lazy::new(|| Mutex::new(0));
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
static IOS_LAST_REMOTE_NAV_ACTION: Lazy<Mutex<(String, u128)>> =
    Lazy::new(|| Mutex::new((String::new(), 0)));

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn with_ios_player<R>(f: impl FnOnce(&mut IosAudioPlayer) -> R) -> Option<R> {
    let mut guard = IOS_AUDIO_PLAYER.lock().ok()?;
    if guard.is_none() {
        *guard = IosAudioPlayer::new();
    }
    guard.as_mut().map(f)
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn push_ios_remote_action(action: &str) {
    if let Ok(mut actions) = IOS_REMOTE_ACTIONS.lock() {
        if matches!(action, "play" | "pause") {
            // Only the latest transport intent matters; stale play/pause entries
            // can force incorrect state once the app returns to foreground.
            actions.retain(|entry| entry != "play" && entry != "pause");
        } else if action.starts_with("seek:") {
            // Keep only the freshest seek target.
            actions.retain(|entry| !entry.starts_with("seek:"));
        }
        actions.push_back(action.to_string());
        ios_diag_log(
            "remote.queue.push",
            &format!("action={action} queued={}", actions.len()),
        );
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn clear_ios_transport_intents(reason: &str) {
    if let Ok(mut actions) = IOS_REMOTE_ACTIONS.lock() {
        let before = actions.len();
        actions.retain(|entry| {
            !(entry == "play" || entry == "pause" || entry.starts_with("seek:"))
        });
        let after = actions.len();
        if before != after {
            ios_diag_log(
                "remote.queue.clear",
                &format!("reason={reason} removed={} remaining={after}", before - after),
            );
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn pop_ios_remote_action() -> Option<String> {
    let mut actions = IOS_REMOTE_ACTIONS.lock().ok()?;
    let action = actions.pop_front();
    if let Some(name) = action.as_deref() {
        ios_diag_log(
            "remote.queue.pop",
            &format!("action={name} remaining={}", actions.len()),
        );
    }
    action
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_remote_queue_len() -> usize {
    IOS_REMOTE_ACTIONS.lock().map(|actions| actions.len()).unwrap_or(0)
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn set_ios_remote_observer(observer: *mut Object) {
    if let Ok(mut slot) = IOS_REMOTE_OBSERVER.lock() {
        *slot = observer as usize;
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn get_ios_remote_observer() -> *mut Object {
    IOS_REMOTE_OBSERVER
        .lock()
        .ok()
        .map(|slot| *slot as *mut Object)
        .unwrap_or(ptr::null_mut())
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn set_ios_remote_center(center: *mut Object) {
    if let Ok(mut slot) = IOS_REMOTE_CENTER.lock() {
        *slot = center as usize;
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn get_ios_remote_center() -> *mut Object {
    let configured = IOS_REMOTE_CENTER
        .lock()
        .ok()
        .map(|slot| *slot as *mut Object)
        .unwrap_or(ptr::null_mut());
    if !configured.is_null() {
        return configured;
    }
    unsafe {
        let center_cls = class!(MPRemoteCommandCenter);
        let shared: *mut Object = msg_send![center_cls, sharedCommandCenter];
        shared
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn set_ios_now_playing_center(center: *mut Object) {
    if let Ok(mut slot) = IOS_NOW_PLAYING_CENTER.lock() {
        *slot = center as usize;
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn get_ios_now_playing_center() -> *mut Object {
    let configured = IOS_NOW_PLAYING_CENTER
        .lock()
        .ok()
        .map(|slot| *slot as *mut Object)
        .unwrap_or(ptr::null_mut());
    if !configured.is_null() {
        return configured;
    }
    unsafe {
        let center_cls = class!(MPNowPlayingInfoCenter);
        let shared: *mut Object = msg_send![center_cls, defaultCenter];
        shared
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn set_ios_now_playing_session(session: *mut Object) {
    if let Ok(mut slot) = IOS_NOW_PLAYING_SESSION.lock() {
        *slot = session as usize;
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn get_ios_now_playing_session() -> *mut Object {
    IOS_NOW_PLAYING_SESSION
        .lock()
        .ok()
        .map(|slot| *slot as *mut Object)
        .unwrap_or(ptr::null_mut())
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn activate_ios_now_playing_session(reason: &str) {
    let session = get_ios_now_playing_session();
    if session.is_null() {
        return;
    }
    unsafe {
        let _: () = msg_send![
            session,
            becomeActiveIfPossibleWithCompletion: ptr::null_mut::<Object>()
        ];
    }
    ios_diag_log("remote.session", &format!("become-active reason={reason}"));
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn should_ignore_duplicate_nav_action(action: &str) -> bool {
    let now = ios_diag_now_ms();
    let Ok(mut last) = IOS_LAST_REMOTE_NAV_ACTION.lock() else {
        return false;
    };
    if last.0 == action && now.saturating_sub(last.1) < 650 {
        ios_diag_log(
            "remote.command",
            &format!("ignored duplicate action={action} dt={}ms", now.saturating_sub(last.1)),
        );
        return true;
    }
    last.0 = action.to_string();
    last.1 = now;
    false
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
unsafe fn ios_log_remote_command_enabled_state(center: *mut Object, label: &str) {
    if center.is_null() {
        ios_diag_log("remote.state", &format!("{label}: center=null"));
        return;
    }
    let play_cmd: *mut Object = msg_send![center, playCommand];
    let pause_cmd: *mut Object = msg_send![center, pauseCommand];
    let stop_cmd: *mut Object = msg_send![center, stopCommand];
    let toggle_cmd: *mut Object = msg_send![center, togglePlayPauseCommand];
    let next_cmd: *mut Object = msg_send![center, nextTrackCommand];
    let prev_cmd: *mut Object = msg_send![center, previousTrackCommand];
    let seek_cmd: *mut Object = msg_send![center, changePlaybackPositionCommand];
    let seekf_cmd: *mut Object = msg_send![center, seekForwardCommand];
    let seekb_cmd: *mut Object = msg_send![center, seekBackwardCommand];
    let play_enabled: BOOL = msg_send![play_cmd, isEnabled];
    let pause_enabled: BOOL = msg_send![pause_cmd, isEnabled];
    let stop_enabled: BOOL = msg_send![stop_cmd, isEnabled];
    let toggle_enabled: BOOL = msg_send![toggle_cmd, isEnabled];
    let next_enabled: BOOL = msg_send![next_cmd, isEnabled];
    let prev_enabled: BOOL = msg_send![prev_cmd, isEnabled];
    let seek_enabled: BOOL = msg_send![seek_cmd, isEnabled];
    let seekf_enabled: BOOL = msg_send![seekf_cmd, isEnabled];
    let seekb_enabled: BOOL = msg_send![seekb_cmd, isEnabled];
    ios_diag_log(
        "remote.state",
        &format!(
            "{label}: play={} pause={} stop={} toggle={} next={} prev={} seek={} seekf={} seekb={}",
            play_enabled == YES,
            pause_enabled == YES,
            stop_enabled == YES,
            toggle_enabled == YES,
            next_enabled == YES,
            prev_enabled == YES,
            seek_enabled == YES,
            seekf_enabled == YES,
            seekb_enabled == YES
        ),
    );
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn cmtime_seconds(time: CMTime) -> f64 {
    unsafe {
        let seconds = CMTimeGetSeconds(time);
        if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ns_string(value: &str) -> Option<*mut Object> {
    unsafe {
        let ns_string_cls = class!(NSString);
        let alloc: *mut Object = msg_send![ns_string_cls, alloc];
        if alloc.is_null() {
            return None;
        }

        // UTF-8 encoding.
        let encoded: *mut Object = msg_send![alloc,
            initWithBytes: value.as_ptr()
            length: value.len()
            encoding: 4usize
        ];

        if encoded.is_null() {
            None
        } else {
            Some(encoded)
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_file_path_from_url(src: &str) -> Option<String> {
    let raw = src.strip_prefix("file://")?;
    let normalized = if raw.starts_with('/') {
        raw.to_string()
    } else {
        format!("/{raw}")
    };
    // `path_to_file_url` does not currently percent-encode. Normalize common encoded spaces anyway.
    Some(normalized.replace("%20", " "))
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn make_player_item(src: &str) -> Option<*mut Object> {
    unsafe {
        let url_cls = class!(NSURL);
        let url: *mut Object = if let Some(file_path) = ios_file_path_from_url(src) {
            let path_str = ns_string(&file_path)?;
            let url: *mut Object = msg_send![url_cls, fileURLWithPath: path_str];
            let _: () = msg_send![path_str, release];
            url
        } else {
            let src_str = ns_string(src)?;
            let url: *mut Object = msg_send![url_cls, URLWithString: src_str];
            let _: () = msg_send![src_str, release];
            url
        };
        if url.is_null() {
            return None;
        }

        let item_cls = class!(AVPlayerItem);
        let item: *mut Object = msg_send![item_cls, playerItemWithURL: url];
        if item.is_null() {
            None
        } else {
            Some(item)
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn configure_ios_audio_session() {
    unsafe {
        let session_cls = class!(AVAudioSession);
        let session: *mut Object = msg_send![session_cls, sharedInstance];
        if session.is_null() {
            ios_diag_log("session.config", "AVAudioSession sharedInstance is null");
            return;
        }

        let Some(category) = ns_string("AVAudioSessionCategoryPlayback") else {
            ios_diag_log("session.config", "failed to allocate playback category string");
            return;
        };

        let _: BOOL =
            msg_send![session, setCategory: category error: ptr::null_mut::<*mut Object>()];
        let _: () = msg_send![category, release];

        let _: BOOL = msg_send![session, setActive: YES error: ptr::null_mut::<*mut Object>()];
        ios_diag_log("session.config", "configured category=playback active=true");
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_app_state_code() -> i64 {
    unsafe {
        let app_cls = class!(UIApplication);
        let app: *mut Object = msg_send![app_cls, sharedApplication];
        if app.is_null() {
            return -1;
        }
        let state: isize = msg_send![app, applicationState];
        state as i64
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_player_is_paused() -> bool {
    with_ios_player(|player| unsafe {
        let rate: f32 = msg_send![player.player, rate];
        rate <= 0.0
    })
    .unwrap_or(true)
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn remote_handler_class() -> &'static objc::runtime::Class {
    static REGISTER: Once = Once::new();
    static mut CLASS_PTR: *const objc::runtime::Class = std::ptr::null();

    REGISTER.call_once(|| unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("RustySoundRemoteCommandHandler", superclass)
            .expect("failed to create remote command handler class");

        decl.add_method(
            sel!(handlePlay:),
            ios_handle_play as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handlePause:),
            ios_handle_pause as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handleTogglePlayPause:),
            ios_handle_toggle_play_pause
                as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handleStop:),
            ios_handle_stop as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handleNext:),
            ios_handle_next as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handlePrevious:),
            ios_handle_previous as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handleSkipForward:),
            ios_handle_skip_forward as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handleSkipBackward:),
            ios_handle_skip_backward as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handleSeekForward:),
            ios_handle_seek_forward as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handleSeekBackward:),
            ios_handle_seek_backward as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handleSeek:),
            ios_handle_seek as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handleEnded:),
            ios_handle_ended as extern "C" fn(&Object, objc::runtime::Sel, *mut Object),
        );
        decl.add_method(
            sel!(handleDidEnterBackground:),
            ios_handle_did_enter_background as extern "C" fn(&Object, objc::runtime::Sel, *mut Object),
        );
        decl.add_method(
            sel!(handleWillEnterForeground:),
            ios_handle_will_enter_foreground as extern "C" fn(&Object, objc::runtime::Sel, *mut Object),
        );
        decl.add_method(
            sel!(handleDidBecomeActive:),
            ios_handle_did_become_active as extern "C" fn(&Object, objc::runtime::Sel, *mut Object),
        );
        decl.add_method(
            sel!(handleWillResignActive:),
            ios_handle_will_resign_active as extern "C" fn(&Object, objc::runtime::Sel, *mut Object),
        );

        let cls = decl.register();
        CLASS_PTR = cls;
    });

    unsafe { &*CLASS_PTR }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_play(_: &Object, _: objc::runtime::Sel, _: *mut Object) -> i64 {
    configure_ios_audio_session();
    ios_diag_log(
        "remote.command",
        &format!(
            "play app_state={} queued_actions={}",
            ios_app_state_code(),
            ios_remote_queue_len()
        ),
    );
    let _ = with_ios_player(|player| player.apply(serde_json::json!({ "type": "play" })));
    push_ios_remote_action("play");
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_pause(_: &Object, _: objc::runtime::Sel, _: *mut Object) -> i64 {
    configure_ios_audio_session();
    let paused = ios_player_is_paused();
    ios_diag_log(
        "remote.command",
        &format!(
            "pause app_state={} queued_actions={} paused={}",
            ios_app_state_code(),
            ios_remote_queue_len(),
            paused
        ),
    );
    if paused {
        // Some iOS lock-screen routes can emit pause while UI shows play.
        // Treat pause-while-paused as a toggle-to-play fallback.
        ios_diag_log("remote.command", "pause-while-paused -> treating as play");
        let _ = with_ios_player(|player| player.apply(serde_json::json!({ "type": "play" })));
        push_ios_remote_action("play");
    } else {
        let _ = with_ios_player(|player| player.apply(serde_json::json!({ "type": "pause" })));
        push_ios_remote_action("pause");
    }
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_toggle_play_pause(
    _: &Object,
    _: objc::runtime::Sel,
    _: *mut Object,
) -> i64 {
    configure_ios_audio_session();
    let paused = ios_player_is_paused();
    ios_diag_log(
        "remote.command",
        &format!(
            "toggle-play-pause app_state={} queued_actions={} paused={}",
            ios_app_state_code(),
            ios_remote_queue_len(),
            paused
        ),
    );
    if paused {
        let _ = with_ios_player(|player| player.apply(serde_json::json!({ "type": "play" })));
        push_ios_remote_action("play");
    } else {
        let _ = with_ios_player(|player| player.apply(serde_json::json!({ "type": "pause" })));
        push_ios_remote_action("pause");
    }
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_stop(_: &Object, _: objc::runtime::Sel, _: *mut Object) -> i64 {
    configure_ios_audio_session();
    ios_diag_log(
        "remote.command",
        &format!(
            "stop app_state={} queued_actions={}",
            ios_app_state_code(),
            ios_remote_queue_len()
        ),
    );
    let _ = with_ios_player(|player| player.apply(serde_json::json!({ "type": "pause" })));
    push_ios_remote_action("pause");
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_apply_immediate_transition(
    action: &str,
    queue_action: &str,
    clear_reason: &str,
) -> bool {
    if let Some(item) = ios_plan_take_transition(action) {
        if let Some(src) = item.src.clone() {
            clear_ios_transport_intents(clear_reason);
            ios_diag_log(
                "remote.immediate",
                &format!("action={action} song_id={}", item.song_id),
            );
            let _ = with_ios_player(|player| {
                player.apply(serde_json::json!({
                    "type": "load",
                    "src": src,
                    "song_id": item.song_id,
                    "position": 0.0,
                    "play": true,
                    "meta": item.meta,
                }));
                player.apply(serde_json::json!({ "type": "play" }));
            });
            return true;
        }
    }
    push_ios_remote_action(queue_action);
    false
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_next(_: &Object, _: objc::runtime::Sel, _: *mut Object) -> i64 {
    configure_ios_audio_session();
    ios_diag_log(
        "remote.command",
        &format!(
            "next app_state={} queued_actions={}",
            ios_app_state_code(),
            ios_remote_queue_len()
        ),
    );
    if should_ignore_duplicate_nav_action("next") {
        return 0;
    }
    let _ = ios_apply_immediate_transition("next", "next", "next-immediate");
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_previous(_: &Object, _: objc::runtime::Sel, _: *mut Object) -> i64 {
    configure_ios_audio_session();
    ios_diag_log(
        "remote.command",
        &format!(
            "previous app_state={} queued_actions={}",
            ios_app_state_code(),
            ios_remote_queue_len()
        ),
    );
    if should_ignore_duplicate_nav_action("previous") {
        return 0;
    }
    let _ = ios_apply_immediate_transition("previous", "previous", "previous-immediate");
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_skip_forward(_: &Object, _: objc::runtime::Sel, _: *mut Object) -> i64 {
    configure_ios_audio_session();
    ios_diag_log(
        "remote.command",
        &format!(
            "skip-forward app_state={} queued_actions={}",
            ios_app_state_code(),
            ios_remote_queue_len()
        ),
    );
    if should_ignore_duplicate_nav_action("next") {
        return 0;
    }
    let _ = ios_apply_immediate_transition("next", "next", "skip-forward-immediate");
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_skip_backward(_: &Object, _: objc::runtime::Sel, _: *mut Object) -> i64 {
    configure_ios_audio_session();
    ios_diag_log(
        "remote.command",
        &format!(
            "skip-backward app_state={} queued_actions={}",
            ios_app_state_code(),
            ios_remote_queue_len()
        ),
    );
    if should_ignore_duplicate_nav_action("previous") {
        return 0;
    }
    let _ = ios_apply_immediate_transition("previous", "previous", "skip-backward-immediate");
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_seek_forward(_: &Object, _: objc::runtime::Sel, event: *mut Object) -> i64 {
    configure_ios_audio_session();
    unsafe {
        let phase: i64 = if event.is_null() {
            -1
        } else {
            let ty: isize = msg_send![event, type];
            ty as i64
        };
        ios_diag_log(
            "remote.command",
            &format!(
                "seek-forward app_state={} queued_actions={} phase={phase}",
                ios_app_state_code(),
                ios_remote_queue_len()
            ),
        );
        if phase <= 0 && !should_ignore_duplicate_nav_action("next") {
            let _ = ios_apply_immediate_transition("next", "next", "seek-forward-immediate");
        }
    }
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_seek_backward(_: &Object, _: objc::runtime::Sel, event: *mut Object) -> i64 {
    configure_ios_audio_session();
    unsafe {
        let phase: i64 = if event.is_null() {
            -1
        } else {
            let ty: isize = msg_send![event, type];
            ty as i64
        };
        ios_diag_log(
            "remote.command",
            &format!(
                "seek-backward app_state={} queued_actions={} phase={phase}",
                ios_app_state_code(),
                ios_remote_queue_len()
            ),
        );
        if phase <= 0 && !should_ignore_duplicate_nav_action("previous") {
            let _ = ios_apply_immediate_transition("previous", "previous", "seek-backward-immediate");
        }
    }
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_seek(_: &Object, _: objc::runtime::Sel, event: *mut Object) -> i64 {
    configure_ios_audio_session();
    unsafe {
        if !event.is_null() {
            let position: f64 = msg_send![event, positionTime];
            let clamped = position.max(0.0);
            ios_diag_log(
                "remote.command",
                &format!(
                    "seek target={clamped:.3} app_state={} queued_actions={}",
                    ios_app_state_code(),
                    ios_remote_queue_len()
                ),
            );
            let _ = with_ios_player(|player| {
                player.apply(serde_json::json!({
                    "type": "seek",
                    "position": clamped,
                }));
            });
            push_ios_remote_action(&format!("seek:{clamped}"));
        }
    }
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_ended(_: &Object, _: objc::runtime::Sel, _: *mut Object) {
    ios_diag_log(
        "remote.command",
        &format!(
            "ended-notification app_state={} queued_actions={}",
            ios_app_state_code(),
            ios_remote_queue_len()
        ),
    );
    if let Some(item) = ios_plan_take_transition("ended") {
        if let Some(src) = item.src.clone() {
            clear_ios_transport_intents("ended-immediate");
            ios_diag_log(
                "remote.immediate",
                &format!("action=ended song_id={}", item.song_id),
            );
            let _ = with_ios_player(|player| {
                player.apply(serde_json::json!({
                    "type": "load",
                    "src": src,
                    "song_id": item.song_id,
                    "position": 0.0,
                    "play": true,
                    "meta": item.meta,
                }));
                player.apply(serde_json::json!({ "type": "play" }));
            });
            return;
        }
    }
    push_ios_remote_action("ended");
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_did_enter_background(_: &Object, _: objc::runtime::Sel, _: *mut Object) {
    ios_diag_log(
        "app.lifecycle",
        &format!("did-enter-background queued_actions={}", ios_remote_queue_len()),
    );
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_will_enter_foreground(_: &Object, _: objc::runtime::Sel, _: *mut Object) {
    ios_diag_log(
        "app.lifecycle",
        &format!("will-enter-foreground queued_actions={}", ios_remote_queue_len()),
    );
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_did_become_active(_: &Object, _: objc::runtime::Sel, _: *mut Object) {
    ios_diag_log(
        "app.lifecycle",
        &format!("did-become-active queued_actions={}", ios_remote_queue_len()),
    );
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_will_resign_active(_: &Object, _: objc::runtime::Sel, _: *mut Object) {
    ios_diag_log(
        "app.lifecycle",
        &format!("will-resign-active queued_actions={}", ios_remote_queue_len()),
    );
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
unsafe fn register_ios_remote_targets_on_center(center: *mut Object, observer: *mut Object, label: &str) {
    if center.is_null() {
        ios_diag_log("remote.init", &format!("{label}: center is null"));
        return;
    }

    let play_cmd: *mut Object = msg_send![center, playCommand];
    let pause_cmd: *mut Object = msg_send![center, pauseCommand];
    let stop_cmd: *mut Object = msg_send![center, stopCommand];
    let toggle_play_pause_cmd: *mut Object = msg_send![center, togglePlayPauseCommand];
    let next_cmd: *mut Object = msg_send![center, nextTrackCommand];
    let previous_cmd: *mut Object = msg_send![center, previousTrackCommand];
    let seek_cmd: *mut Object = msg_send![center, changePlaybackPositionCommand];
    let seek_forward_cmd: *mut Object = msg_send![center, seekForwardCommand];
    let seek_backward_cmd: *mut Object = msg_send![center, seekBackwardCommand];
    let skip_forward_cmd: *mut Object = msg_send![center, skipForwardCommand];
    let skip_backward_cmd: *mut Object = msg_send![center, skipBackwardCommand];

    let _: () = msg_send![play_cmd, removeTarget: observer];
    let _: () = msg_send![pause_cmd, removeTarget: observer];
    let _: () = msg_send![stop_cmd, removeTarget: observer];
    let _: () = msg_send![toggle_play_pause_cmd, removeTarget: observer];
    let _: () = msg_send![next_cmd, removeTarget: observer];
    let _: () = msg_send![previous_cmd, removeTarget: observer];
    let _: () = msg_send![seek_cmd, removeTarget: observer];
    let _: () = msg_send![seek_forward_cmd, removeTarget: observer];
    let _: () = msg_send![seek_backward_cmd, removeTarget: observer];
    let _: () = msg_send![skip_forward_cmd, removeTarget: observer];
    let _: () = msg_send![skip_backward_cmd, removeTarget: observer];

    let _: () = msg_send![play_cmd, addTarget: observer action: sel!(handlePlay:)];
    let _: () = msg_send![pause_cmd, addTarget: observer action: sel!(handlePause:)];
    let _: () = msg_send![stop_cmd, addTarget: observer action: sel!(handleStop:)];
    let _: () = msg_send![
        toggle_play_pause_cmd,
        addTarget: observer
        action: sel!(handleTogglePlayPause:)
    ];
    let _: () = msg_send![next_cmd, addTarget: observer action: sel!(handleNext:)];
    let _: () = msg_send![previous_cmd, addTarget: observer action: sel!(handlePrevious:)];
    let _: () = msg_send![
        seek_forward_cmd,
        addTarget: observer
        action: sel!(handleSeekForward:)
    ];
    let _: () = msg_send![
        seek_backward_cmd,
        addTarget: observer
        action: sel!(handleSeekBackward:)
    ];
    let _: () = msg_send![
        skip_forward_cmd,
        addTarget: observer
        action: sel!(handleSkipForward:)
    ];
    let _: () = msg_send![
        skip_backward_cmd,
        addTarget: observer
        action: sel!(handleSkipBackward:)
    ];
    let _: () = msg_send![seek_cmd, addTarget: observer action: sel!(handleSeek:)];

    let _: () = msg_send![play_cmd, setEnabled: YES];
    let _: () = msg_send![pause_cmd, setEnabled: YES];
    let _: () = msg_send![stop_cmd, setEnabled: YES];
    let _: () = msg_send![toggle_play_pause_cmd, setEnabled: YES];
    let _: () = msg_send![next_cmd, setEnabled: YES];
    let _: () = msg_send![previous_cmd, setEnabled: YES];
    let _: () = msg_send![seek_cmd, setEnabled: YES];
    let _: () = msg_send![seek_forward_cmd, setEnabled: YES];
    let _: () = msg_send![seek_backward_cmd, setEnabled: YES];

    // Use discrete track controls instead of interval skip controls so lock-screen
    // buttons map to previous/next track rather than +/- seconds.
    let no: BOOL = YES ^ YES;
    let _: () = msg_send![skip_forward_cmd, setEnabled: no];
    let _: () = msg_send![skip_backward_cmd, setEnabled: no];

    ios_diag_log("remote.init", &format!("{label}: targets registered"));
    ios_log_remote_command_enabled_state(center, &format!("{label}.post-register"));
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn configure_ios_remote_commands(player: *mut Object) {
    IOS_REMOTE_INIT.call_once(|| unsafe {
        let cls = remote_handler_class();
        let observer: *mut Object = msg_send![cls, new];
        if observer.is_null() {
            ios_diag_log("remote.init", "failed to allocate observer");
            return;
        }
        set_ios_remote_observer(observer);

        let mut center: *mut Object = ptr::null_mut();
        let mut now_playing_center: *mut Object = ptr::null_mut();

        if !player.is_null() {
            if let Some(session_cls) = objc::runtime::Class::get("MPNowPlayingSession") {
                let players_cls = class!(NSArray);
                let players: *mut Object = msg_send![players_cls, arrayWithObject: player];
                if !players.is_null() {
                    let session_alloc: *mut Object = msg_send![session_cls, alloc];
                    if !session_alloc.is_null() {
                        let session: *mut Object = msg_send![session_alloc, initWithPlayers: players];
                        if !session.is_null() {
                            set_ios_now_playing_session(session);
                            let no: BOOL = YES ^ YES;
                            let _: () =
                                msg_send![session, setAutomaticallyPublishesNowPlayingInfo: no];
                            let _: () = msg_send![
                                session,
                                becomeActiveIfPossibleWithCompletion: ptr::null_mut::<Object>()
                            ];
                            center = msg_send![session, remoteCommandCenter];
                            now_playing_center = msg_send![session, nowPlayingInfoCenter];
                            ios_diag_log("remote.init", "using MPNowPlayingSession");
                        }
                    }
                }
            }
        }

        if center.is_null() {
            let center_cls = class!(MPRemoteCommandCenter);
            center = msg_send![center_cls, sharedCommandCenter];
            set_ios_now_playing_session(ptr::null_mut());
            ios_diag_log("remote.init", "using shared MPRemoteCommandCenter");
        }

        if now_playing_center.is_null() {
            let now_playing_cls = class!(MPNowPlayingInfoCenter);
            now_playing_center = msg_send![now_playing_cls, defaultCenter];
        }

        set_ios_remote_center(center);
        set_ios_now_playing_center(now_playing_center);

        if center.is_null() {
            ios_diag_log("remote.init", "remote command center is null");
            return;
        }

        let app_cls = class!(UIApplication);
        let app: *mut Object = msg_send![app_cls, sharedApplication];
        if !app.is_null() {
            let _: () = msg_send![app, beginReceivingRemoteControlEvents];
            ios_diag_log("remote.init", "beginReceivingRemoteControlEvents");
        } else {
            ios_diag_log("remote.init", "UIApplication sharedApplication is null");
        }

        register_ios_remote_targets_on_center(center, observer, "primary-center");
        let center_cls = class!(MPRemoteCommandCenter);
        let shared_center: *mut Object = msg_send![center_cls, sharedCommandCenter];
        if !shared_center.is_null() && shared_center != center {
            register_ios_remote_targets_on_center(shared_center, observer, "shared-center");
        }

        let notification_center_cls = class!(NSNotificationCenter);
        let notification_center: *mut Object =
            msg_send![notification_center_cls, defaultCenter];
        if !notification_center.is_null() {
            if let Some(name) = ns_string("UIApplicationDidEnterBackgroundNotification") {
                let _: () = msg_send![notification_center,
                    addObserver: observer
                    selector: sel!(handleDidEnterBackground:)
                    name: name
                    object: ptr::null_mut::<Object>()
                ];
                let _: () = msg_send![name, release];
            }
            if let Some(name) = ns_string("UIApplicationWillEnterForegroundNotification") {
                let _: () = msg_send![notification_center,
                    addObserver: observer
                    selector: sel!(handleWillEnterForeground:)
                    name: name
                    object: ptr::null_mut::<Object>()
                ];
                let _: () = msg_send![name, release];
            }
            if let Some(name) = ns_string("UIApplicationDidBecomeActiveNotification") {
                let _: () = msg_send![notification_center,
                    addObserver: observer
                    selector: sel!(handleDidBecomeActive:)
                    name: name
                    object: ptr::null_mut::<Object>()
                ];
                let _: () = msg_send![name, release];
            }
            if let Some(name) = ns_string("UIApplicationWillResignActiveNotification") {
                let _: () = msg_send![notification_center,
                    addObserver: observer
                    selector: sel!(handleWillResignActive:)
                    name: name
                    object: ptr::null_mut::<Object>()
                ];
                let _: () = msg_send![name, release];
            }
        }

        ios_diag_log("remote.init", "command center + lifecycle observers configured");
    });
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn set_ios_remote_transport_state(is_playing: bool) {
    unsafe {
        let center = get_ios_remote_center();
        if center.is_null() {
            return;
        }

        let play_cmd: *mut Object = msg_send![center, playCommand];
        let pause_cmd: *mut Object = msg_send![center, pauseCommand];
        let stop_cmd: *mut Object = msg_send![center, stopCommand];
        let toggle_play_pause_cmd: *mut Object = msg_send![center, togglePlayPauseCommand];
        let next_cmd: *mut Object = msg_send![center, nextTrackCommand];
        let previous_cmd: *mut Object = msg_send![center, previousTrackCommand];
        let seek_cmd: *mut Object = msg_send![center, changePlaybackPositionCommand];
        let seek_forward_cmd: *mut Object = msg_send![center, seekForwardCommand];
        let seek_backward_cmd: *mut Object = msg_send![center, seekBackwardCommand];

        let yes: BOOL = YES;
        let no: BOOL = YES ^ YES;
        let play_enabled = if is_playing { no } else { yes };
        let pause_enabled = if is_playing { yes } else { no };
        // Match command availability to transport state; keep toggle enabled for
        // headset routes that only emit a single media key action.
        let _: () = msg_send![play_cmd, setEnabled: play_enabled];
        let _: () = msg_send![pause_cmd, setEnabled: pause_enabled];
        let _: () = msg_send![stop_cmd, setEnabled: pause_enabled];
        let _: () = msg_send![toggle_play_pause_cmd, setEnabled: yes];
        let _: () = msg_send![next_cmd, setEnabled: yes];
        let _: () = msg_send![previous_cmd, setEnabled: yes];
        let _: () = msg_send![seek_cmd, setEnabled: yes];
        let _: () = msg_send![seek_forward_cmd, setEnabled: yes];
        let _: () = msg_send![seek_backward_cmd, setEnabled: yes];
        ios_log_remote_command_enabled_state(center, "transport-sync.post-set");
        activate_ios_now_playing_session(if is_playing {
            "transport-playing"
        } else {
            "transport-paused"
        });
        let (session_present, session_active, session_can_become_active) = {
            let session = get_ios_now_playing_session();
            if session.is_null() {
                (false, false, false)
            } else {
                let active: BOOL = msg_send![session, isActive];
                let can_become: BOOL = msg_send![session, canBecomeActive];
                (true, active == YES, can_become == YES)
            }
        };
        ios_diag_log(
            "remote.transport",
            &format!(
                "state_sync playing={is_playing} play_enabled={} pause_enabled={} stop_enabled={} toggle+next+prev+seek+seekf+seekb=enabled session_present={session_present} session_active={session_active} can_become_active={session_can_become_active}",
                play_enabled == yes,
                pause_enabled == yes,
                pause_enabled == yes
            ),
        );
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn observe_ios_item_end(item: *mut Object) {
    unsafe {
        let observer = get_ios_remote_observer();
        if observer.is_null() {
            ios_diag_log("item.end.observe", "observer is null");
            return;
        }

        let center_cls = class!(NSNotificationCenter);
        let center: *mut Object = msg_send![center_cls, defaultCenter];
        if center.is_null() {
            ios_diag_log("item.end.observe", "notification center is null");
            return;
        }

        let Some(notification_name) = ns_string("AVPlayerItemDidPlayToEndTimeNotification") else {
            ios_diag_log("item.end.observe", "failed to allocate notification name");
            return;
        };

        let _: () = msg_send![center,
            removeObserver: observer
            name: notification_name
            object: ptr::null_mut::<Object>()
        ];
        let _: () = msg_send![center, addObserver: observer selector: sel!(handleEnded:) name: notification_name object: item];
        let _: () = msg_send![notification_name, release];
        ios_diag_log(
            "item.end.observe",
            if item.is_null() {
                "cleared item-end observer"
            } else {
                "attached item-end observer"
            },
        );
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn set_now_playing_string(dict: *mut Object, key: *mut Object, value: &str) {
    if value.is_empty() {
        return;
    }
    unsafe {
        if key.is_null() {
            return;
        }
        let Some(value_obj) = ns_string(value) else {
            return;
        };
        let _: () = msg_send![dict, setObject: value_obj forKey: key];
        let _: () = msg_send![value_obj, release];
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn set_now_playing_number(dict: *mut Object, key: *mut Object, value: f64) {
    unsafe {
        if key.is_null() {
            return;
        }
        let number_cls = class!(NSNumber);
        let value_obj: *mut Object = msg_send![number_cls, numberWithDouble: value];
        if !value_obj.is_null() {
            let _: () = msg_send![dict, setObject: value_obj forKey: key];
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn set_now_playing_usize(dict: *mut Object, key: *mut Object, value: usize) {
    unsafe {
        if key.is_null() {
            return;
        }
        let number_cls = class!(NSNumber);
        let value_obj: *mut Object = msg_send![number_cls, numberWithUnsignedLongLong: value as u64];
        if !value_obj.is_null() {
            let _: () = msg_send![dict, setObject: value_obj forKey: key];
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn set_now_playing_bool(dict: *mut Object, key: *mut Object, value: bool) {
    unsafe {
        if key.is_null() {
            return;
        }
        let number_cls = class!(NSNumber);
        let flag: BOOL = if value { YES } else { YES ^ YES };
        let value_obj: *mut Object = msg_send![number_cls, numberWithBool: flag];
        if !value_obj.is_null() {
            let _: () = msg_send![dict, setObject: value_obj forKey: key];
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn make_now_playing_artwork(artwork_url: &str) -> Option<*mut Object> {
    unsafe {
        let url_str = ns_string(artwork_url)?;
        let url_cls = class!(NSURL);
        let url: *mut Object = msg_send![url_cls, URLWithString: url_str];
        let _: () = msg_send![url_str, release];
        if url.is_null() {
            return None;
        }

        let data_cls = class!(NSData);
        let data: *mut Object = msg_send![data_cls, dataWithContentsOfURL: url];
        if data.is_null() {
            return None;
        }

        let image_cls = class!(UIImage);
        let image: *mut Object = msg_send![image_cls, imageWithData: data];
        if image.is_null() {
            return None;
        }

        let artwork_cls = class!(MPMediaItemArtwork);
        let artwork_alloc: *mut Object = msg_send![artwork_cls, alloc];
        if artwork_alloc.is_null() {
            return None;
        }

        let artwork: *mut Object = msg_send![artwork_alloc, initWithImage: image];
        if artwork.is_null() {
            None
        } else {
            Some(artwork)
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn set_ios_now_playing_info(
    meta: &NativeTrackMetadata,
    elapsed: f64,
    duration: f64,
    rate: f64,
    artwork_obj: *mut Object,
) {
    unsafe {
        let center = get_ios_now_playing_center();
        if center.is_null() {
            return;
        }

        let dict_cls = class!(NSMutableDictionary);
        let dict_alloc: *mut Object = msg_send![dict_cls, alloc];
        if dict_alloc.is_null() {
            return;
        }
        let dict: *mut Object = msg_send![dict_alloc, init];
        if dict.is_null() {
            return;
        }

        set_now_playing_string(dict, MPMediaItemPropertyTitle, &meta.title);
        set_now_playing_string(dict, MPMediaItemPropertyArtist, &meta.artist);
        set_now_playing_string(dict, MPMediaItemPropertyAlbumTitle, &meta.album);
        set_now_playing_number(
            dict,
            MPNowPlayingInfoPropertyElapsedPlaybackTime,
            elapsed.max(0.0),
        );
        if duration.is_finite() && duration > 0.0 {
            set_now_playing_number(dict, MPMediaItemPropertyPlaybackDuration, duration);
        }
        set_now_playing_number(dict, MPNowPlayingInfoPropertyPlaybackRate, rate.max(0.0));
        set_now_playing_number(dict, MPNowPlayingInfoPropertyDefaultPlaybackRate, 1.0);
        // Expose queue context so iOS can keep previous/next controls actionable.
        if let Some((queue_index, queue_len)) = ios_plan_queue_stats() {
            set_now_playing_usize(dict, MPNowPlayingInfoPropertyPlaybackQueueIndex, queue_index);
            set_now_playing_usize(dict, MPNowPlayingInfoPropertyPlaybackQueueCount, queue_len);
        }
        // MPNowPlayingInfoMediaTypeAudio == 1.
        set_now_playing_usize(dict, MPNowPlayingInfoPropertyMediaType, 1);
        set_now_playing_bool(dict, MPNowPlayingInfoPropertyIsLiveStream, meta.is_live);
        if !artwork_obj.is_null() && !MPMediaItemPropertyArtwork.is_null() {
            let _: () = msg_send![dict, setObject: artwork_obj forKey: MPMediaItemPropertyArtwork];
        }

        let _: () = msg_send![center, setNowPlayingInfo: dict];
        // Despite docs noting macOS focus, publishing playbackState on iOS improves
        // lock-screen transport consistency on some routes/devices.
        let playback_state: isize = if rate > 0.0 { 1 } else { 2 };
        let _: () = msg_send![center, setPlaybackState: playback_state];
        let _: () = msg_send![dict, release];
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn clear_ios_now_playing_info() {
    unsafe {
        let center = get_ios_now_playing_center();
        if center.is_null() {
            return;
        }
        let nil_info: *mut Object = ptr::null_mut();
        let _: () = msg_send![center, setNowPlayingInfo: nil_info];
        let stopped_state: isize = 3;
        let _: () = msg_send![center, setPlaybackState: stopped_state];
    }
}
