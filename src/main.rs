use dioxus::prelude::*;

mod api;
mod components;
mod db;

use components::AppShell;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const APP_CSS: Asset = asset!("/assets/styling/app.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Stylesheet { href: APP_CSS }
        document::Stylesheet { href: TAILWIND_CSS }

        AppShell {}
    }
}