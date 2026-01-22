use dioxus::prelude::*;

mod api;
mod components;
mod db;

use components::AppView;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const APPLE_TOUCH_ICON: Asset = asset!("/assets/apple-touch-icon.png");
const APP_CSS: Asset = asset!("/assets/styling/app.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
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
        document::Link { rel: "manifest", href: "/assets/site.webmanifest" }

        // Theme color for mobile browsers
        document::Meta { name: "theme-color", content: "#a38449" }
        document::Meta { name: "mobile-web-app-capable", content: "yes" }
        document::Meta { name: "apple-mobile-web-app-status-bar-style", content: "default" }
        document::Meta { name: "apple-mobile-web-app-title", content: "RustySound" }

        document::Stylesheet { href: TAILWIND_CSS }
        document::Stylesheet { href: APP_CSS }

        Router::<AppView> {}
    }
}
