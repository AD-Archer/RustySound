use dioxus::prelude::*;
use dioxus_router::Navigator;

use crate::components::app_view::AppView;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys::{window, HtmlElement};

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
        let current_view = self.1();
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

    pub fn can_go_back(&self) -> bool {
        self.0.can_go_back()
    }

    pub fn go_back(&self) -> Option<AppView> {
        self.0.go_back();
        reset_main_scroll_position();
        None // Router handles the navigation, we don't need to return the view
    }

    #[cfg(target_arch = "wasm32")]
    pub fn can_go_forward(&self) -> bool {
        self.0.can_go_forward()
    }

    #[cfg(target_arch = "wasm32")]
    pub fn go_forward(&self) -> Option<AppView> {
        self.0.go_forward();
        reset_main_scroll_position();
        None // Router handles the navigation, we don't need to return the view
    }
}
