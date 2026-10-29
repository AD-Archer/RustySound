//! Fixed-row-height windowing for long lists (queue, songs, big playlists).

use dioxus::prelude::*;

/// Scroll position of the shell's main scroller, published by the shell.
/// Values are quantised (32px steps) so subscribers re-render at most once per
/// step, not on every scroll event.
#[derive(Clone, Copy)]
pub struct MainScroll {
    /// Scroll offset of the main scroller.
    pub top: Signal<f64>,
    /// Visible height of the main scroller.
    pub height: Signal<f64>,
    /// Viewport y of the main scroller's top edge.
    pub viewport_top: Signal<f64>,
}

/// Only rows near the viewport are mounted; the list keeps its full height.
/// * `item_count`, `row_height` (px): every row must be exactly this tall.
/// * `render_row`: builds row `i` (usually an `li`, e.g. `TrackRow`).
/// * `height_px`: Some(h) makes the list its own scroller of that height
///   (sheets, panels). None windows against the main page scroll.
/// * `overscan`: extra rows above and below the viewport.
/// * `class`: classes for the outer box (e.g. `"rs-list rs-list--tracks"`;
///   separators still work). `testid`: data-testid.
///
/// Page mode measures the list's offset when it mounts and when
/// `item_count` changes; content above it that changes height later is
/// absorbed by the overscan.
#[component]
pub fn VirtualList(
    item_count: usize,
    row_height: f64,
    render_row: Callback<usize, Element>,
    #[props(default)] height_px: Option<f64>,
    #[props(default = 8)] overscan: usize,
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

    let row_height = row_height.max(1.0);
    let (scroll_top, view_height) = match (height_px, main_scroll) {
        (Some(h), _) => (own_top(), h),
        (None, Some(scroll)) => ((scroll.top)() - list_offset(), (scroll.height)()),
        (None, None) => (0.0, 1200.0),
    };
    let view_height = if view_height > 0.0 { view_height } else { 1200.0 };
    let first = ((scroll_top / row_height).floor().max(0.0) as usize).saturating_sub(overscan);
    let last = (((scroll_top + view_height) / row_height).ceil().max(0.0) as usize + overscan)
        .min(item_count);
    let first = first.min(last);
    let total = item_count as f64 * row_height;
    let window_top = first as f64 * row_height;

    let outer_style = match height_px {
        Some(h) => format!("height:{h}px"),
        None => String::new(),
    };

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
                    let top = (evt.scroll_top() / 32.0).floor() * 32.0;
                    if top != *own_top.peek() {
                        own_top.set(top);
                    }
                }
            },
            div { style: "position:relative;height:{total}px",
                ul {
                    class: "rs-vlist__window",
                    style: "top:{window_top}px",
                    for i in first..last {
                        Fragment { key: "{i}", {render_row.call(i)} }
                    }
                }
            }
        }
    }
}
