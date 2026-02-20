use dioxus::prelude::*;

mod api;
mod cache;
mod cache_service;
mod components;
mod db;
mod diagnostics;
mod offline_art;
mod offline_audio;

use components::AppView;

const FAVICON: &str = "/assets/favicon.ico";
const APPLE_TOUCH_ICON: &str = "/assets/apple-touch-icon.png";
const WEB_MANIFEST: &str = "/assets/site.webmanifest";
const APP_CSS: &str = "/assets/styling/app.css";
const TAILWIND_CSS: &str = "/assets/tailwind.css";

#[cfg(all(feature = "desktop", target_os = "windows"))]
fn windows_app_icon() -> Option<dioxus::desktop::tao::window::Icon> {
    use dioxus::desktop::tao::dpi::PhysicalSize;
    use dioxus::desktop::tao::platform::windows::IconExtWindows;
    use dioxus::desktop::tao::window::Icon;

    let icon_path = std::env::temp_dir().join("rustysound-window-icon.ico");
    std::fs::write(&icon_path, include_bytes!("../assets/favicon.ico")).ok()?;
    Icon::from_path(icon_path, Some(PhysicalSize::new(256, 256))).ok()
}

fn main() {
    #[cfg(feature = "desktop")]
    {
        use dioxus::desktop::{Config, WindowBuilder};
        #[cfg(target_os = "linux")]
        {
            // Linux WebKit can stutter on some Wayland/driver combinations.
            // Keep conservative defaults unless user already configured them.
            if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
                unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
            }
        }

        let mut window = WindowBuilder::new().with_title("RustySound");
        #[cfg(target_os = "windows")]
        {
            use dioxus::desktop::tao::platform::windows::WindowBuilderExtWindows;
            if let Some(icon) = windows_app_icon() {
                window = window
                    .with_window_icon(Some(icon.clone()))
                    .with_taskbar_icon(Some(icon));
            }
        }

        let mut config = Config::new()
            .with_menu(None)
            .with_window(window)
            // Set native WebView background before HTML/CSS load to avoid startup white flash.
            .with_background_color((9, 9, 11, 255))
            // Keep this explicit on Linux to avoid known DMA-BUF rendering glitches.
            .with_disable_dma_buf_on_wayland(true);

        #[cfg(target_os = "linux")]
        {
            config = config.with_custom_head(
                r#"<style>
                html, body { background: #09090b !important; }
                .app-container {
                  background: linear-gradient(180deg, #09090b 0%, #12141a 100%) !important;
                }
                .app-container::before { display: none !important; }
                .glass {
                  background: rgba(24, 24, 27, 0.96) !important;
                  backdrop-filter: none !important;
                  -webkit-backdrop-filter: none !important;
                }
                .main-scroll {
                  scroll-behavior: auto !important;
                  overscroll-behavior: contain !important;
                }
                @media (min-width: 1200px) {
                  .app-container {
                    background: #09090b !important;
                  }
                  .main-scroll [class*="transition"],
                  .main-scroll [class*="animate-"] {
                    transition: none !important;
                    animation: none !important;
                  }
                  .main-scroll [class*="shadow"] {
                    box-shadow: none !important;
                  }
                }
                </style>"#
                    .to_string(),
            );
        }

        dioxus::LaunchBuilder::desktop().with_cfg(config).launch(App);
        return;
    }

    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Title { "RustySound" }
        // Favicon and icons
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "apple-touch-icon-precomposed", href: APPLE_TOUCH_ICON }
        document::Link {
            rel: "apple-touch-icon",
            sizes: "180x180",
            href: APPLE_TOUCH_ICON,
        }
        document::Link {
            rel: "apple-touch-icon",
            sizes: "152x152",
            href: APPLE_TOUCH_ICON,
        }
        document::Link {
            rel: "apple-touch-icon",
            sizes: "120x120",
            href: APPLE_TOUCH_ICON,
        }
        document::Link { rel: "apple-touch-icon", sizes: "76x76", href: APPLE_TOUCH_ICON }
        document::Link {
            rel: "icon",
            r#type: "image/png",
            sizes: "192x192",
            href: FAVICON,
        }
        document::Link {
            rel: "icon",
            r#type: "image/png",
            sizes: "512x512",
            href: FAVICON,
        }

        // Web app manifest
        document::Link { rel: "manifest", href: WEB_MANIFEST }

        // Theme color for mobile browsers
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover",
        }
        document::Meta { name: "theme-color", content: "#a38449" }
        document::Meta { name: "mobile-web-app-capable", content: "yes" }
        document::Meta { name: "apple-mobile-web-app-status-bar-style", content: "default" }
        document::Meta { name: "apple-mobile-web-app-title", content: "RustySound" }

        document::Stylesheet { href: TAILWIND_CSS }
        document::Stylesheet { href: APP_CSS }

        Router::<AppView> {}
    }
}
