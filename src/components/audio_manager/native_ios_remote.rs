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
        actions.push_back(action.to_string());
        ios_diag_log(
            "remote.queue.push",
            &format!("action={action} queued={}", actions.len()),
        );
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
            sel!(handleNext:),
            ios_handle_next as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handlePrevious:),
            ios_handle_previous as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
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
    ios_diag_log("remote.command", "play");
    let _ = with_ios_player(|player| player.apply(serde_json::json!({ "type": "play" })));
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_pause(_: &Object, _: objc::runtime::Sel, _: *mut Object) -> i64 {
    ios_diag_log("remote.command", "pause");
    let _ = with_ios_player(|player| player.apply(serde_json::json!({ "type": "pause" })));
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_next(_: &Object, _: objc::runtime::Sel, _: *mut Object) -> i64 {
    ios_diag_log("remote.command", "next");
    if let Some(item) = ios_plan_take_transition("next") {
        if let Some(src) = item.src.clone() {
            ios_diag_log(
                "remote.immediate",
                &format!("action=next song_id={}", item.song_id),
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
            return 0;
        }
    }
    push_ios_remote_action("next");
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_previous(_: &Object, _: objc::runtime::Sel, _: *mut Object) -> i64 {
    ios_diag_log("remote.command", "previous");
    if let Some(item) = ios_plan_take_transition("previous") {
        if let Some(src) = item.src.clone() {
            ios_diag_log(
                "remote.immediate",
                &format!("action=previous song_id={}", item.song_id),
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
            return 0;
        }
    }
    push_ios_remote_action("previous");
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_seek(_: &Object, _: objc::runtime::Sel, event: *mut Object) -> i64 {
    unsafe {
        if !event.is_null() {
            let position: f64 = msg_send![event, positionTime];
            let clamped = position.max(0.0);
            ios_diag_log("remote.command", &format!("seek target={clamped:.3}"));
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
    ios_diag_log("remote.command", "ended-notification");
    if let Some(item) = ios_plan_take_transition("ended") {
        if let Some(src) = item.src.clone() {
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
fn configure_ios_remote_commands() {
    IOS_REMOTE_INIT.call_once(|| unsafe {
        let cls = remote_handler_class();
        let observer: *mut Object = msg_send![cls, new];
        if observer.is_null() {
            ios_diag_log("remote.init", "failed to allocate observer");
            return;
        }
        set_ios_remote_observer(observer);

        let center_cls = class!(MPRemoteCommandCenter);
        let center: *mut Object = msg_send![center_cls, sharedCommandCenter];
        if center.is_null() {
            ios_diag_log("remote.init", "MPRemoteCommandCenter sharedCommandCenter is null");
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

        let play_cmd: *mut Object = msg_send![center, playCommand];
        let pause_cmd: *mut Object = msg_send![center, pauseCommand];
        let next_cmd: *mut Object = msg_send![center, nextTrackCommand];
        let previous_cmd: *mut Object = msg_send![center, previousTrackCommand];
        let seek_cmd: *mut Object = msg_send![center, changePlaybackPositionCommand];
        let skip_forward_cmd: *mut Object = msg_send![center, skipForwardCommand];
        let skip_backward_cmd: *mut Object = msg_send![center, skipBackwardCommand];

        let _: () = msg_send![play_cmd, addTarget: observer action: sel!(handlePlay:)];
        let _: () = msg_send![pause_cmd, addTarget: observer action: sel!(handlePause:)];
        let _: () = msg_send![next_cmd, addTarget: observer action: sel!(handleNext:)];
        let _: () = msg_send![previous_cmd, addTarget: observer action: sel!(handlePrevious:)];
        let _: () = msg_send![seek_cmd, addTarget: observer action: sel!(handleSeek:)];

        let _: () = msg_send![play_cmd, setEnabled: YES];
        let _: () = msg_send![pause_cmd, setEnabled: YES];
        let _: () = msg_send![next_cmd, setEnabled: YES];
        let _: () = msg_send![previous_cmd, setEnabled: YES];
        let _: () = msg_send![seek_cmd, setEnabled: YES];

        let no: BOOL = false;
        let _: () = msg_send![skip_forward_cmd, setEnabled: no];
        let _: () = msg_send![skip_backward_cmd, setEnabled: no];

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
        let center_cls = class!(MPNowPlayingInfoCenter);
        let center: *mut Object = msg_send![center_cls, defaultCenter];
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
        if !artwork_obj.is_null() && !MPMediaItemPropertyArtwork.is_null() {
            let _: () = msg_send![dict, setObject: artwork_obj forKey: MPMediaItemPropertyArtwork];
        }

        let _: () = msg_send![center, setNowPlayingInfo: dict];
        let _: () = msg_send![dict, release];
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn clear_ios_now_playing_info() {
    unsafe {
        let center_cls = class!(MPNowPlayingInfoCenter);
        let center: *mut Object = msg_send![center_cls, defaultCenter];
        if center.is_null() {
            return;
        }
        let nil_info: *mut Object = ptr::null_mut();
        let _: () = msg_send![center, setNowPlayingInfo: nil_info];
    }
}
