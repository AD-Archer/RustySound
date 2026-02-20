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

#[cfg(feature = "desktop")]
const FAVICON_ICO: &str = "/assets/favicon.ico";
#[cfg(not(feature = "desktop"))]
const FAVICON_ICO: Asset = asset!("/assets/favicon.ico");

#[cfg(feature = "desktop")]
const FAVICON_SVG: &str = "/assets/favicon.svg";
#[cfg(not(feature = "desktop"))]
const FAVICON_SVG: Asset = asset!("/assets/favicon.svg");

#[cfg(feature = "desktop")]
const FAVICON_PNG_96: &str = "/assets/favicon-96x96.png";
#[cfg(not(feature = "desktop"))]
const FAVICON_PNG_96: Asset = asset!("/assets/favicon-96x96.png");

#[cfg(feature = "desktop")]
const APP_ICON_192: &str = "/assets/web-app-manifest-192x192.png";
#[cfg(not(feature = "desktop"))]
const APP_ICON_192: Asset = asset!("/assets/web-app-manifest-192x192.png");

#[cfg(feature = "desktop")]
const APP_ICON_512: &str = "/assets/web-app-manifest-512x512.png";
#[cfg(not(feature = "desktop"))]
const APP_ICON_512: Asset = asset!("/assets/web-app-manifest-512x512.png");

#[cfg(feature = "desktop")]
const APPLE_TOUCH_ICON: &str = "/assets/apple-touch-icon.png";
#[cfg(not(feature = "desktop"))]
const APPLE_TOUCH_ICON: Asset = asset!("/assets/apple-touch-icon.png");

#[cfg(feature = "desktop")]
const WEB_MANIFEST: &str = "/assets/site.webmanifest";
#[cfg(not(feature = "desktop"))]
const WEB_MANIFEST: Asset = asset!("/assets/site.webmanifest");

#[cfg(not(feature = "desktop"))]
const APP_CSS: Asset = asset!("/assets/styling/app.css");

#[cfg(not(feature = "desktop"))]
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

#[cfg(feature = "desktop")]
const APP_CSS_INLINE: &str = include_str!("../assets/styling/app.css");
#[cfg(feature = "desktop")]
const TAILWIND_CSS_INLINE: &str = include_str!("../assets/tailwind.css");

#[cfg(feature = "desktop")]
fn desktop_app_icon() -> Option<dioxus::desktop::tao::window::Icon> {
    use dioxus::desktop::tao::window::Icon;
    use std::io::Cursor;

    let mut decoder = png::Decoder::new(Cursor::new(include_bytes!(
        "../assets/web-app-manifest-512x512.png"
    )));
    decoder.set_transformations(png::Transformations::normalize_to_color8());

    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).ok()?;
    let bytes = &buffer[..info.buffer_size()];

    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity((bytes.len() / 3) * 4);
            for rgb in bytes.chunks_exact(3) {
                rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
            rgba
        }
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity(bytes.len() * 4);
            for &gray in bytes {
                rgba.extend_from_slice(&[gray, gray, gray, 255]);
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity(bytes.len() * 2);
            for gray_alpha in bytes.chunks_exact(2) {
                rgba.extend_from_slice(&[
                    gray_alpha[0],
                    gray_alpha[0],
                    gray_alpha[0],
                    gray_alpha[1],
                ]);
            }
            rgba
        }
        png::ColorType::Indexed => return None,
    };

    Icon::from_rgba(rgba, info.width, info.height).ok()
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
        if let Some(icon) = desktop_app_icon() {
            window = window.with_window_icon(Some(icon.clone()));

            #[cfg(target_os = "windows")]
            {
                use dioxus::desktop::tao::platform::windows::WindowBuilderExtWindows;
                window = window.with_taskbar_icon(Some(icon));
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

        dioxus::LaunchBuilder::desktop()
            .with_cfg(config)
            .launch(App);
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
        document::Link { rel: "icon", r#type: "image/svg+xml", href: FAVICON_SVG }
        document::Link { rel: "shortcut icon", href: FAVICON_ICO }
        document::Link { rel: "icon", r#type: "image/x-icon", href: FAVICON_ICO }
        document::Link { rel: "icon", r#type: "image/png", sizes: "96x96", href: FAVICON_PNG_96 }
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
            href: APP_ICON_192,
        }
        document::Link {
            rel: "icon",
            r#type: "image/png",
            sizes: "512x512",
            href: APP_ICON_512,
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

        GlobalStyles {}

        Router::<AppView> {}
    }
}

#[component]
fn GlobalStyles() -> Element {
    #[cfg(feature = "desktop")]
    {
        return rsx! {
            document::Style { {TAILWIND_CSS_INLINE} }
            document::Style { {APP_CSS_INLINE} }
        };
    }

    #[cfg(not(feature = "desktop"))]
    {
        return rsx! {
            document::Stylesheet { href: TAILWIND_CSS }
            document::Stylesheet { href: APP_CSS }
        };
    }
}
