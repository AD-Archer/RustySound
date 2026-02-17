use dioxus::prelude::*;

mod api;
mod components;
mod db;

use components::AppView;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const APPLE_TOUCH_ICON: Asset = asset!("/assets/apple-touch-icon.png");
const WEB_MANIFEST: Asset = asset!("/assets/site.webmanifest");
const APP_CSS: Asset = asset!("/assets/styling/app.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

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

        dioxus::LaunchBuilder::desktop()
            .with_cfg(Config::new().with_menu(None).with_window(window))
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
