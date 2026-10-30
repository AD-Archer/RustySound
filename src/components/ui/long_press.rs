//! Touch long-press bookkeeping for rows and tiles that open a menu.
//!
//! Same rules as `TrackRow`: 450 ms, cancelled by moving more than 10px,
//! mouse pointers ignored (they use right-click), and the click that ends a
//! long-press is swallowed.

use crate::components::ui::env::sleep_ms;
use dioxus::prelude::*;

const LONG_PRESS_MS: u64 = 450;
const LONG_PRESS_SLOP_PX: f64 = 10.0;

/// Handle returned by `use_long_press`. Wire `down`/`moved`/`cancel` to the
/// pointer events and check `take_fired()` at the top of `onclick`. None of
/// its signals are read during render, so presses never re-render the owner.
#[derive(Clone, Copy)]
pub struct LongPress {
    generation: Signal<u64>,
    origin: Signal<Option<(f64, f64)>>,
    fired: Signal<bool>,
}

impl LongPress {
    /// `onpointerdown`: arms the timer; `on_fire` gets the press point.
    pub fn down(&self, evt: &PointerEvent, on_fire: impl FnOnce((f64, f64)) + 'static) {
        let (mut generation, mut origin, mut fired) = (self.generation, self.origin, self.fired);
        fired.set(false);
        if evt.pointer_type() == "mouse" {
            return;
        }
        let point = evt.client_coordinates();
        let current = *generation.peek() + 1;
        generation.set(current);
        origin.set(Some((point.x, point.y)));
        spawn(async move {
            sleep_ms(LONG_PRESS_MS).await;
            if *generation.peek() != current {
                return;
            }
            let start = *origin.peek();
            if let Some(start) = start {
                origin.set(None);
                fired.set(true);
                on_fire(start);
            }
        });
    }

    /// `onpointermove`: cancels when the finger travels.
    pub fn moved(&self, evt: &PointerEvent) {
        let mut origin = self.origin;
        let start = *origin.peek();
        if let Some((x0, y0)) = start {
            let point = evt.client_coordinates();
            if (point.x - x0).hypot(point.y - y0) > LONG_PRESS_SLOP_PX {
                origin.set(None);
            }
        }
    }

    /// `onpointerup` / `onpointercancel` / `oncontextmenu`.
    pub fn cancel(&self) {
        let mut origin = self.origin;
        if origin.peek().is_some() {
            origin.set(None);
        }
    }

    /// True once after a long-press fired: the following click should not
    /// also activate the row.
    pub fn take_fired(&self) -> bool {
        let mut fired = self.fired;
        if *fired.peek() {
            fired.set(false);
            true
        } else {
            false
        }
    }
}

/// Creates the long-press state for one row or tile.
pub fn use_long_press() -> LongPress {
    LongPress {
        generation: use_signal(|| 0),
        origin: use_signal(|| None),
        fired: use_signal(|| false),
    }
}
