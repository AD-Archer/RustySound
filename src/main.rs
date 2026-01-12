use dioxus::prelude::*;

mod api;
mod components;
mod db;

use components::AppShell;

const FAVICON: Asset = asset!("/assets/logo.png");
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
        document::Link { rel: "apple-touch-icon", href: FAVICON }
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
        document::Link { rel: "manifest", href: "/manifest.json" }

        // Theme color for mobile browsers
        document::Meta { name: "theme-color", content: "#1f2937" }
        document::Meta { name: "apple-mobile-web-app-capable", content: "yes" }
        document::Meta { name: "apple-mobile-web-app-status-bar-style", content: "default" }
        document::Meta { name: "apple-mobile-web-app-title", content: "RustySound" }

        document::Stylesheet { href: TAILWIND_CSS }
        document::Stylesheet { href: APP_CSS }

        AppShell {}
    }
}
