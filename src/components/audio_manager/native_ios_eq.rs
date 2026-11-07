// iOS equalizer: an MTAudioProcessingTap on each AVPlayerItem's audio mix
// runs `crate::equalizer::EqProcessor` on the audio thread. The setting
// reaches it through `equalizer::live` (atomics, no locks); the tap rebuilds
// its filters when the version or the sample rate changes.
//
// Taps are attached once an item is ready to play (its asset's tracks are
// known then), for the current item and the gapless next one, from the
// player's poll and on every "eq" command. Each item keeps its tap, so speed
// changes, seeks and gapless advances need nothing more. Offline `file://`
// items work the same way. HLS streams expose no asset tracks, so live radio
// over HLS plays without the EQ. Nothing is attached until the EQ is first
// turned on; turning it off leaves the taps passing audio through.
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
mod ios_eq {
    use crate::equalizer::{live, EqProcessor};
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    use std::ffi::c_void;
    use std::ptr;

    type TapRef = *mut c_void;

    #[repr(C)]
    struct AudioStreamBasicDescription {
        sample_rate: f64,
        format_id: u32,
        format_flags: u32,
        bytes_per_packet: u32,
        frames_per_packet: u32,
        bytes_per_frame: u32,
        channels_per_frame: u32,
        bits_per_channel: u32,
        reserved: u32,
    }

    #[repr(C)]
    struct AudioBuffer {
        number_channels: u32,
        data_byte_size: u32,
        data: *mut c_void,
    }

    #[repr(C)]
    struct AudioBufferList {
        number_buffers: u32,
        buffers: [AudioBuffer; 1],
    }

    /// `MTAudioProcessingTapCallbacks`; the header packs it to 4 bytes.
    #[repr(C, packed(4))]
    struct TapCallbacks {
        version: i32,
        client_info: *mut c_void,
        init: Option<unsafe extern "C" fn(TapRef, *mut c_void, *mut *mut c_void)>,
        finalize: Option<unsafe extern "C" fn(TapRef)>,
        prepare: Option<unsafe extern "C" fn(TapRef, isize, *const AudioStreamBasicDescription)>,
        unprepare: Option<unsafe extern "C" fn(TapRef)>,
        process: unsafe extern "C" fn(TapRef, isize, u32, *mut AudioBufferList, *mut isize, *mut u32),
    }

    const LINEAR_PCM: u32 = u32::from_be_bytes(*b"lpcm");
    const FLAG_FLOAT: u32 = 1;
    const FLAG_NON_INTERLEAVED: u32 = 1 << 5;
    const TAP_PRE_EFFECTS: u32 = 1;
    const TAP_START_OF_STREAM: u32 = 1 << 8;
    const ITEM_READY: isize = 1;

    #[link(name = "MediaToolbox", kind = "framework")]
    unsafe extern "C" {
        fn MTAudioProcessingTapCreate(
            allocator: *const c_void,
            callbacks: *const TapCallbacks,
            flags: u32,
            tap_out: *mut TapRef,
        ) -> i32;
        fn MTAudioProcessingTapGetStorage(tap: TapRef) -> *mut c_void;
        fn MTAudioProcessingTapGetSourceAudio(
            tap: TapRef,
            frames: isize,
            list: *mut AudioBufferList,
            flags_out: *mut u32,
            time_range_out: *mut c_void,
            frames_out: *mut isize,
        ) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    #[link(name = "AVFoundation", kind = "framework")]
    unsafe extern "C" {
        static AVMediaTypeAudio: *mut Object;
    }

    /// One tap's state, owned by the tap (`init` boxes it, `finalize` frees it).
    struct TapState {
        eq: EqProcessor,
        sample_rate: f64,
        /// Float32 PCM; anything else passes through untouched.
        usable: bool,
        interleaved: bool,
        channels: usize,
    }

    unsafe extern "C" fn tap_init(_tap: TapRef, _info: *mut c_void, storage_out: *mut *mut c_void) {
        let state = Box::new(TapState {
            eq: EqProcessor::default(),
            sample_rate: 0.0,
            usable: false,
            interleaved: false,
            channels: 0,
        });
        *storage_out = Box::into_raw(state) as *mut c_void;
    }

    unsafe extern "C" fn tap_finalize(tap: TapRef) {
        let storage = MTAudioProcessingTapGetStorage(tap) as *mut TapState;
        if !storage.is_null() {
            drop(Box::from_raw(storage));
        }
    }

    unsafe extern "C" fn tap_prepare(tap: TapRef, _max_frames: isize, format: *const AudioStreamBasicDescription) {
        let state = &mut *(MTAudioProcessingTapGetStorage(tap) as *mut TapState);
        let Some(format) = format.as_ref() else {
            state.usable = false;
            return;
        };
        state.sample_rate = format.sample_rate;
        state.channels = format.channels_per_frame as usize;
        state.interleaved = format.format_flags & FLAG_NON_INTERLEAVED == 0;
        state.usable = format.format_id == LINEAR_PCM
            && format.format_flags & FLAG_FLOAT != 0
            && format.bits_per_channel == 32;
        state.eq.reset();
    }

    unsafe extern "C" fn tap_unprepare(_tap: TapRef) {}

    /// The audio thread: no allocation, no locks.
    unsafe extern "C" fn tap_process(
        tap: TapRef,
        frames: isize,
        _flags: u32,
        list: *mut AudioBufferList,
        frames_out: *mut isize,
        flags_out: *mut u32,
    ) {
        let status = MTAudioProcessingTapGetSourceAudio(tap, frames, list, flags_out, ptr::null_mut(), frames_out);
        if status != 0 || list.is_null() {
            return;
        }
        let state = &mut *(MTAudioProcessingTapGetStorage(tap) as *mut TapState);
        if !state.usable {
            return;
        }
        if !flags_out.is_null() && *flags_out & TAP_START_OF_STREAM != 0 {
            state.eq.reset();
        }
        state.eq.sync(state.sample_rate);
        if !state.eq.is_active() {
            return;
        }
        let count = (*list).number_buffers as usize;
        let buffers = (*list).buffers.as_mut_ptr();
        for index in 0..count {
            let buffer = &mut *buffers.add(index);
            if buffer.data.is_null() {
                continue;
            }
            let len = buffer.data_byte_size as usize / std::mem::size_of::<f32>();
            let samples = std::slice::from_raw_parts_mut(buffer.data as *mut f32, len);
            if state.interleaved {
                let channels = (buffer.number_channels as usize).max(1);
                state.eq.process_interleaved(samples, channels);
            } else {
                state.eq.process_planar(index, samples);
            }
        }
        let _ = state.channels;
    }

    /// Puts an EQ tap on `item`'s audio track. False when the item isn't
    /// ready, has one already, or has no audio track to tap (HLS).
    pub(super) unsafe fn attach(item: *mut Object) -> bool {
        if item.is_null() {
            return false;
        }
        let status: isize = msg_send![item, status];
        if status != ITEM_READY {
            return false;
        }
        let existing: *mut Object = msg_send![item, audioMix];
        if !existing.is_null() {
            return false;
        }
        let asset: *mut Object = msg_send![item, asset];
        if asset.is_null() || AVMediaTypeAudio.is_null() {
            return false;
        }
        let tracks: *mut Object = msg_send![asset, tracksWithMediaType: AVMediaTypeAudio];
        let count: usize = if tracks.is_null() { 0 } else { msg_send![tracks, count] };
        if count == 0 {
            return false;
        }
        let track: *mut Object = msg_send![tracks, objectAtIndex: 0usize];

        let callbacks = TapCallbacks {
            version: 0,
            client_info: ptr::null_mut(),
            init: Some(tap_init),
            finalize: Some(tap_finalize),
            prepare: Some(tap_prepare),
            unprepare: Some(tap_unprepare),
            process: tap_process,
        };
        let mut tap: TapRef = ptr::null_mut();
        if MTAudioProcessingTapCreate(ptr::null(), &callbacks, TAP_PRE_EFFECTS, &mut tap) != 0 || tap.is_null() {
            return false;
        }
        let params: *mut Object =
            msg_send![class!(AVMutableAudioMixInputParameters), audioMixInputParametersWithTrack: track];
        let mix: *mut Object = msg_send![class!(AVMutableAudioMix), audioMix];
        if params.is_null() || mix.is_null() {
            CFRelease(tap);
            return false;
        }
        let _: () = msg_send![params, setAudioTapProcessor: tap];
        let list: *mut Object = msg_send![class!(NSArray), arrayWithObject: params];
        let _: () = msg_send![mix, setInputParameters: list];
        let _: () = msg_send![item, setAudioMix: mix];
        // The input parameters hold the tap now.
        CFRelease(tap);
        true
    }

    /// Whether taps should be attached: only once the EQ has been turned on.
    pub(super) fn wanted() -> bool {
        live::enabled()
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
impl IosAudioPlayer {
    /// Taps the current and the queued next item, when the EQ is on and
    /// they're ready. Cheap when there's nothing to do.
    fn ensure_eq_taps(&mut self) {
        if !ios_eq::wanted() {
            return;
        }
        unsafe {
            let current: *mut Object = msg_send![self.player, currentItem];
            if ios_eq::attach(current) {
                ios_diag_log("player.eq", "tap attached to current item");
            }
            if !self.next_item.is_null() && self.next_item != current && ios_eq::attach(self.next_item) {
                ios_diag_log("player.eq", "tap attached to next item");
            }
        }
    }
}
