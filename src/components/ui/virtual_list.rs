//! Fixed-row-height windowing for long lists (queue, songs, big playlists).

use dioxus::prelude::*;

/// Scroll position of the shell's main scroller, published by the shell
/// through [`use_main_scroll_provider`]. Values are quantised (32px steps) so
/// subscribers re-render at most once per step, not on every scroll event.
#[derive(Clone, Copy)]
pub struct MainScroll {
    /// Scroll offset of the main scroller.
    pub top: Signal<f64>,
    /// Visible height of the main scroller.
    pub height: Signal<f64>,
    /// Viewport y of the main scroller's top edge.
    pub viewport_top: Signal<f64>,
}

/// Step the published scroll offset is quantised to.
const SCROLL_STEP_PX: f64 = 32.0;

/// Watches `.rs-main` from a passive, rAF-coalesced listener and sends the
/// quantised offset only when it changes. A Dioxus `onscroll` handler would
/// send every scroll event to Rust, and on the native webviews (iOS, macOS)
/// each one is a synchronous request that blocks the web process's main
/// thread until Rust answers: at 120 events a second during a fling, the page
/// stopped painting and fast scrolling showed black. This sends at most one
/// asynchronous message per frame and never blocks.
const MAIN_SCROLL_JS: &str = r#"
const STEP = __STEP__;
let el = null;
let sent = [NaN, NaN, NaN];
let frame = 0;
const report = () => {
  frame = 0;
  if (!el || !el.isConnected) el = document.querySelector(".rs-main");
  if (!el) return;
  const raw = el.scrollTop;
  const top = raw <= 0 ? 0 : Math.floor(raw / STEP) * STEP;
  const height = el.clientHeight;
  const viewTop = el.getBoundingClientRect().top;
  if (top !== sent[0] || Math.abs(height - sent[1]) > 1 || Math.abs(viewTop - sent[2]) > 1) {
    sent = [top, height, viewTop];
    dioxus.send(sent);
  }
};
const schedule = () => {
  if (!frame) frame = requestAnimationFrame(report);
};
document.addEventListener(
  "scroll",
  (event) => {
    const target = event.target;
    if (target instanceof Element && target.classList.contains("rs-main")) {
      if (target !== el) el = target;
      schedule();
    }
  },
  { capture: true, passive: true },
);
// A layout flip (a resize) replaces the scroller; the next report finds the
// new one, and any scroll event hands it over directly.
window.addEventListener("resize", schedule, { passive: true });
// The scroller mounts after this script starts: look for it for a few
// seconds (once per frame, stopping as soon as it's there).
for (let tries = 0; tries < 600 && !(el && el.isConnected); tries++) {
  await new Promise((resolve) => requestAnimationFrame(resolve));
  report();
}
await new Promise(() => {});
"#;

/// Creates the [`MainScroll`] context for the shell's `.rs-main` scroller.
pub fn use_main_scroll_provider() -> MainScroll {
    let top = use_signal(|| 0.0f64);
    let height = use_signal(|| 0.0f64);
    let viewport_top = use_signal(|| 0.0f64);
    let scroll = use_context_provider(|| MainScroll {
        top,
        height,
        viewport_top,
    });
    use_hook(move || {
        let MainScroll {
            mut top,
            mut height,
            mut viewport_top,
        } = scroll;
        spawn(async move {
            let script = MAIN_SCROLL_JS.replace("__STEP__", &SCROLL_STEP_PX.to_string());
            let mut probe = document::eval(&script);
            while let Ok([next_top, next_height, next_view_top]) = probe.recv::<[f64; 3]>().await {
                if next_top != *top.peek() {
                    top.set(next_top);
                }
                if (next_height - *height.peek()).abs() > 1.0 {
                    height.set(next_height);
                }
                if (next_view_top - *viewport_top.peek()).abs() > 1.0 {
                    viewport_top.set(next_view_top);
                }
            }
        });
    });
    scroll
}

/// Rows mounted above the viewport, in screens.
const LEAD_ABOVE_SCREENS: f64 = 0.5;
/// Rows mounted below the viewport, in screens (most flings go down).
const LEAD_BELOW_SCREENS: f64 = 1.0;

/// The rows to mount, `first..last`, for a viewport at `scroll_top` (relative
/// to the list's top) that is `view_height` tall.
///
/// The window reaches half a screen above and a screen below the viewport,
/// and its edges snap to chunks of a third of a screen. So it only moves when
/// the viewport crosses a chunk edge (a few times per screen scrolled, not on
/// every 32px step), and the viewport is always at least half a screen inside
/// it: a fling has that much room while Rust renders the next chunk.
pub(crate) fn window_for(
    scroll_top: f64,
    view_height: f64,
    row_height: f64,
    count: usize,
    min_overscan: usize,
) -> (usize, usize) {
    if count == 0 {
        return (0, 0);
    }
    let row_height = row_height.max(1.0);
    let view_height = if view_height > 0.0 { view_height } else { 1200.0 };
    let rows_per_view = (view_height / row_height).ceil().max(1.0) as usize;
    let chunk = (rows_per_view / 3).max(4);
    let above = ((rows_per_view as f64 * LEAD_ABOVE_SCREENS).ceil() as usize).max(min_overscan);
    let below = ((rows_per_view as f64 * LEAD_BELOW_SCREENS).ceil() as usize).max(min_overscan);

    // Both edges hang off one chunk-aligned anchor, so they move together.
    let first_visible = (scroll_top.max(0.0) / row_height).floor() as usize;
    let anchor = first_visible / chunk * chunk;
    let first = anchor.saturating_sub(above);
    let last = (anchor + chunk + rows_per_view + 1 + below).min(count);
    (first.min(last), last)
}

/// Only rows near the viewport are mounted; the list keeps its full height.
/// * `item_count`, `row_height` (px): every row must be exactly this tall.
/// * `render_row`: builds row `i` (usually an `li`, e.g. `TrackRow`).
/// * `height_px`: Some(h) makes the list its own scroller of that height
///   (sheets, panels). None windows against the main page scroll.
/// * `overscan`: minimum extra rows above and below the viewport (the
///   window already reaches half a screen up and a screen down).
/// * `ghost_rows`: placeholder rows after the last item, for a paged list
///   whose next page is on its way. Scrolling runs into rows that are about
///   to fill in, not into the end of the list.
/// * `class`: classes for the outer box (e.g. `"rs-list rs-list--tracks"`;
///   separators still work). `testid`: data-testid.
///
/// The window is a memo of the scroll position, so the list re-renders only
/// when the window moves. Page mode measures the list's offset when it mounts
/// and when `item_count` changes; content above it that changes height later
/// is absorbed by the lead.
#[component]
pub fn VirtualList(
    item_count: usize,
    row_height: f64,
    render_row: Callback<usize, Element>,
    #[props(default)] height_px: Option<f64>,
    #[props(default = 4)] overscan: usize,
    #[props(default)] ghost_rows: usize,
    #[props(default)] class: String,
    #[props(default)] testid: Option<String>,
) -> Element {
    let main_scroll = try_use_context::<MainScroll>();
    let mut own_top = use_signal(|| 0.0f64);
    let mut list_offset = use_signal(|| 0.0f64);
    let mut mounted = use_signal(|| None::<std::rc::Rc<MountedData>>);

    // Where the list starts inside the page scroller.
    let measure = move || {
        if height_px.is_some() {
            return;
        }
        let (Some(node), Some(scroll)) = (mounted.peek().clone(), main_scroll) else {
            return;
        };
        spawn(async move {
            if let Ok(rect) = node.get_client_rect().await {
                let offset = rect.origin.y - *scroll.viewport_top.peek() + *scroll.top.peek();
                if (offset - *list_offset.peek()).abs() > 0.5 {
                    list_offset.set(offset);
                }
            }
        });
    };
    use_effect(use_reactive!(|item_count| {
        let _ = item_count;
        measure();
    }));

    let total_rows = item_count + ghost_rows;
    let window = use_memo(use_reactive!(|(total_rows, row_height, height_px, overscan)| {
        let (scroll_top, view_height) = match (height_px, main_scroll) {
            (Some(h), _) => (own_top(), h),
            (None, Some(scroll)) => ((scroll.top)() - list_offset(), (scroll.height)()),
            (None, None) => (0.0, 1200.0),
        };
        window_for(scroll_top, view_height, row_height, total_rows, overscan)
    }));
    let (first, last) = window();

    let row_height = row_height.max(1.0);
    let total = total_rows as f64 * row_height;
    let window_top = first as f64 * row_height;
    // --rs-vrow sizes the placeholder rows CSS paints under inset lists.
    let outer_style = match height_px {
        Some(h) => format!("height:{h}px;--rs-vrow:{row_height}px"),
        None => format!("--rs-vrow:{row_height}px"),
    };
    let real_last = last.min(item_count);
    let ghost_first = first.max(item_count);

    rsx! {
        div {
            class: "rs-vlist {class}",
            class: if height_px.is_some() { "rs-vlist--scroll" },
            style: "{outer_style}",
            "data-testid": testid.clone(),
            onmounted: move |evt| {
                mounted.set(Some(evt.data()));
                measure();
            },
            onscroll: move |evt| {
                if height_px.is_some() {
                    let top = (evt.scroll_top() / SCROLL_STEP_PX).floor() * SCROLL_STEP_PX;
                    if top != *own_top.peek() {
                        own_top.set(top);
                    }
                }
            },
            div { class: "rs-vlist__sizer", style: "height:{total}px",
                ul {
                    class: "rs-vlist__window",
                    style: "top:{window_top}px",
                    for i in first..real_last {
                        Fragment { key: "{i}", {render_row.call(i)} }
                    }
                    for i in ghost_first..last {
                        li {
                            key: "ghost-{i}",
                            class: "rs-vlist__ghost",
                            aria_hidden: "true",
                            style: "height:{row_height}px",
                            div { class: "rs-skel rs-vlist__ghost-thumb" }
                            div { class: "rs-vlist__ghost-lines",
                                div {
                                    class: "rs-skel",
                                    style: "width:{40 + (i * 17) % 30}%;height:12px",
                                }
                                div {
                                    class: "rs-skel",
                                    style: "width:{25 + (i * 11) % 25}%;height:10px",
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::window_for;

    #[test]
    fn empty_list_has_no_window() {
        assert_eq!(window_for(0.0, 800.0, 58.0, 0, 4), (0, 0));
    }

    #[test]
    fn window_covers_viewport_with_lead() {
        // 800px viewport, 58px rows: 14 rows visible.
        for top in [0.0, 1000.0, 5_000.0, 33_333.0] {
            let (first, last) = window_for(top, 800.0, 58.0, 2_000, 4);
            let first_visible = (top / 58.0).floor() as usize;
            let last_visible = ((top + 800.0) / 58.0).ceil() as usize;
            assert!(first <= first_visible.saturating_sub(7), "top {top}: {first}");
            assert!(last >= last_visible + 14, "top {top}: {last}");
            // Bounded: never more than about three screens of rows.
            assert!(last - first <= 14 * 3 + 2 * 4, "top {top}: {}", last - first);
        }
    }

    #[test]
    fn window_is_stable_within_a_chunk() {
        let mut changes = 0;
        let mut previous = window_for(0.0, 800.0, 58.0, 2_000, 4);
        let mut top = 0.0;
        while top < 8_000.0 {
            top += 32.0;
            let next = window_for(top, 800.0, 58.0, 2_000, 4);
            if next != previous {
                changes += 1;
                previous = next;
            }
        }
        // 8,000px is 10 screens; the window moves about 3 times per screen,
        // not on each of the 250 steps.
        assert!(changes <= 40, "{changes} window changes");
    }

    #[test]
    fn window_is_clamped_to_count() {
        assert_eq!(window_for(100_000.0, 800.0, 58.0, 30, 4).1, 30);
        assert_eq!(window_for(0.0, 800.0, 58.0, 10, 4), (0, 10));
    }
}
