//! Now Playing (the song-details sheet), its Lyrics / Up Next / Info panels,
//! and the queue actions the queue page shares.
//!
//! Module map:
//! - `overlay`   the sheet (compact) or large overlay (wide); one tree only
//! - `art`       big cover with reflection, up-next covers waiting to its right
//! - `meta`      title, artist links, star, "…", rating stars, volume
//! - `transport` glossy scrubber, times, format badge, transport buttons
//! - `panels`    Lyrics | Up Next | Info segmented control
//! - `lyrics/*`  lyrics source, lines, search sheet, Shot share mode
//! - `up_next`   Up Next panel, queue edit rows, quick create, save queue
//! - `info`      details list, related songs
//! - `actions`   playback/queue/library actions (`PlayerCtx`)
//! - `types`     `SongDetailsController` and friends

pub(crate) mod actions;
mod art;
mod info;
mod lyrics;
mod meta;
mod overlay;
mod panels;
mod transport;
mod types;
pub(crate) mod up_next;

pub use overlay::SongDetailsOverlay;
pub use types::{SongDetailsController, SongDetailsState, SongDetailsTab};

// TEMP perf probe (agent F); removed before the final commit.
#[allow(dead_code)]
pub(crate) fn np_probe(name: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(win) = web_sys::window() else { return };
        let key = wasm_bindgen::JsValue::from_str("__rsProbe");
        let obj = match js_sys::Reflect::get(&win, &key) {
            Ok(v) if v.is_object() => v,
            _ => {
                let o: wasm_bindgen::JsValue = js_sys::Object::new().into();
                let _ = js_sys::Reflect::set(&win, &key, &o);
                o
            }
        };
        let k = wasm_bindgen::JsValue::from_str(name);
        let cur = js_sys::Reflect::get(&obj, &k).ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
        let _ = js_sys::Reflect::set(&obj, &k, &wasm_bindgen::JsValue::from_f64(cur + 1.0));
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = name;
}
