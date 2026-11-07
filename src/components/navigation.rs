use dioxus::prelude::*;
use dioxus_router::Navigator;

use crate::components::app_view::AppView;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys::{window, HtmlElement};

/// How a route change should move on screen (see `ui/page_transition.js`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageMotion {
    /// Deeper into the app: an iOS push on compact, a short fade on wide.
    Push,
    /// Back up a level: the reverse of `Push`.
    Back,
    /// Forward again through history; plays like `Push`.
    Forward,
    /// Switching tabs (tab bar, sidebar): a quick cross-fade, no push.
    Tab,
}

impl PageMotion {
    fn as_str(self) -> &'static str {
        match self {
            PageMotion::Push => "push",
            PageMotion::Back => "back",
            PageMotion::Forward => "forward",
            PageMotion::Tab => "tab",
        }
    }
}

/// Tell the page transition script a route change is coming, so it can keep
/// a copy of the page on screen to animate from. Must run before the router
/// changes route (and before any scroll reset). On the web `eval` runs
/// synchronously; native webviews run it ahead of the render's DOM edits.
fn announce_page_motion(motion: PageMotion) {
    let _ = document::eval(&format!(
        "window.__rsPageTx&&window.__rsPageTx.prepare('{}');",
        motion.as_str()
    ));
}

#[derive(Clone, Copy)]
pub struct Navigation(
    pub Navigator,
    pub Signal<AppView>,
    pub Signal<Option<AppView>>,
);

fn should_refresh_detail_route(current: &AppView, target: &AppView) -> bool {
    match (current, target) {
        (
            AppView::AlbumDetailView {
                album_id: current_album_id,
                server_id: current_server_id,
            },
            AppView::AlbumDetailView {
                album_id: target_album_id,
                server_id: target_server_id,
            },
        ) => current_album_id != target_album_id || current_server_id != target_server_id,
        (
            AppView::ArtistDetailView {
                artist_id: current_artist_id,
                server_id: current_server_id,
            },
            AppView::ArtistDetailView {
                artist_id: target_artist_id,
                server_id: target_server_id,
            },
        ) => current_artist_id != target_artist_id || current_server_id != target_server_id,
        (
            AppView::PlaylistDetailView {
                playlist_id: current_playlist_id,
                server_id: current_server_id,
            },
            AppView::PlaylistDetailView {
                playlist_id: target_playlist_id,
                server_id: target_server_id,
            },
        ) => current_playlist_id != target_playlist_id || current_server_id != target_server_id,
        _ => false,
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn route_refresh_pause() {
    tokio::time::sleep(std::time::Duration::from_millis(16)).await;
}

#[cfg(target_arch = "wasm32")]
async fn route_refresh_pause() {
    gloo_timers::future::TimeoutFuture::new(16).await;
}

fn reset_main_scroll_position() {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = window() {
            if let Some(document) = win.document() {
                if let Ok(Some(main)) = document.query_selector("main.main-scroll, .main-scroll") {
                    if let Some(element) = main.dyn_ref::<HtmlElement>() {
                        element.set_scroll_top(0);
                    }
                }
            }
            win.scroll_to_with_x_and_y(0.0, 0.0);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = document::eval(
            r#"
(() => {
  const main = document.querySelector("main.main-scroll, .main-scroll");
  if (main && typeof main.scrollTo === "function") {
    main.scrollTo({ top: 0, left: 0, behavior: "auto" });
  } else if (main) {
    main.scrollTop = 0;
  }
  if (typeof window !== "undefined" && typeof window.scrollTo === "function") {
    window.scrollTo(0, 0);
  }
  return true;
})();
            "#,
        );
    }
}

impl Navigation {
    pub fn new(
        navigator: Navigator,
        current_view: Signal<AppView>,
        pending_target: Signal<Option<AppView>>,
    ) -> Self {
        Self(navigator, current_view, pending_target)
    }

    pub fn navigate_to(&self, target: AppView) {
        self.navigate_with(target, PageMotion::Push);
    }

    /// `navigate_to` with an explicit transition: `Tab` for the tab bar and
    /// sidebar, `Back` for a "‹ Back" button with no history to go back
    /// through (a deep link), which pushes the parent page instead.
    pub fn navigate_with(&self, target: AppView, motion: PageMotion) {
        let current_view = self.1();
        if current_view != target {
            announce_page_motion(motion);
        }
        if should_refresh_detail_route(&current_view, &target) {
            let mut current_view_signal = self.1;
            let mut pending_target = self.2;
            eprintln!(
                "[nav.refresh.start] current={} target={}",
                current_view, target
            );
            pending_target.set(Some(target));
            self.0.replace(AppView::HomeView {});
            current_view_signal.set(AppView::HomeView {});
            reset_main_scroll_position();
            return;
        }

        self.0.push(target.clone());
        let mut current_view_signal = self.1;
        current_view_signal.set(target);
        reset_main_scroll_position();
    }

    /// True while `navigate_to` is detouring through Home to remount a
    /// detail page (see `should_refresh_detail_route`). Doesn't subscribe.
    pub fn is_detouring(&self) -> bool {
        self.2.peek().is_some()
    }

    pub fn resume_pending_navigation(&self) {
        if !matches!(self.1(), AppView::HomeView {}) {
            return;
        }

        let Some(target) = self.2() else {
            return;
        };

        let navigator = self.0;
        let mut current_view_signal = self.1;
        let mut pending_target = self.2;
        pending_target.set(None);
        eprintln!("[nav.refresh.resume] target={}", target);
        spawn(async move {
            route_refresh_pause().await;
            navigator.replace(target.clone());
            current_view_signal.set(target);
            reset_main_scroll_position();
        });
    }

    /// Whether a back step stays inside the app. Native builds ask the
    /// router's MemoryHistory. On the web the router always says yes, and
    /// `history.back()` from the first page would leave the app, so ask the
    /// browser instead (see `web_history_can_go`).
    pub fn can_go_back(&self) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            web_history_can_go(true)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.0.can_go_back()
        }
    }

    pub fn go_back(&self) -> Option<AppView> {
        announce_page_motion(PageMotion::Back);
        self.0.go_back();
        reset_main_scroll_position();
        None // Router handles the navigation, we don't need to return the view
    }

    pub fn can_go_forward(&self) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            web_history_can_go(false)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.0.can_go_forward()
        }
    }

    pub fn go_forward(&self) -> Option<AppView> {
        announce_page_motion(PageMotion::Forward);
        self.0.go_forward();
        reset_main_scroll_position();
        None // Router handles the navigation, we don't need to return the view
    }
}

/// Browser history check: the Navigation API's canGoBack / canGoForward where
/// it exists, otherwise the entry index that `ui/edge_swipe.js` stamps into
/// each history state (`[scrollX, scrollY, index]`).
#[cfg(target_arch = "wasm32")]
fn web_history_can_go(back: bool) -> bool {
    use js_sys::Reflect;
    use wasm_bindgen::JsValue;

    let Some(win) = window() else {
        return false;
    };
    let get = |target: &JsValue, key: &str| Reflect::get(target, &JsValue::from_str(key)).ok();
    if let Some(navigation) = get(win.as_ref(), "navigation").filter(|value| value.is_object()) {
        let key = if back { "canGoBack" } else { "canGoForward" };
        if let Some(flag) = get(&navigation, key).and_then(|value| value.as_bool()) {
            return flag;
        }
    }
    let index = get(win.as_ref(), "history")
        .and_then(|history| get(&history, "state"))
        .and_then(|state| get(&state, "2"))
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);
    if back {
        return index > 0.0;
    }
    let newest = win
        .session_storage()
        .ok()
        .flatten()
        .and_then(|storage| storage.get_item("rs.history.max").ok().flatten())
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0);
    index < newest
}
