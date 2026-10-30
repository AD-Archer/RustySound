//! The single stylesheet registry.
//!
//! Every stylesheet the app loads is listed here, once. Native builds (desktop
//! and iOS) inline them with `include_str!`; the web and Android builds link
//! them with `asset!`. Order matters: tokens, base, the kit, then one file per
//! view area, so view rules win ties with kit rules.
//!
//! There is no Tailwind and no class-override theme any more: themes are token
//! sets in `tokens.css` (`[data-theme]`), and views edit their own
//! `views/*.css` file without touching this registry or `main.rs`.

use dioxus::prelude::*;

#[cfg(any(feature = "desktop", target_os = "ios"))]
mod inline {
    pub const STAGE: &str = concat!(
        include_str!("../assets/styling/tokens.css"),
        "\n",
        include_str!("../assets/styling/base.css"),
        "\n",
        include_str!("../assets/styling/components.css"),
        "\n",
        include_str!("../assets/styling/views/discover.css"),
        "\n",
        include_str!("../assets/styling/views/library.css"),
        "\n",
        include_str!("../assets/styling/views/personal.css"),
        "\n",
        include_str!("../assets/styling/views/player.css"),
        "\n",
        include_str!("../assets/styling/views/settings.css"),
        "\n",
        include_str!("../assets/styling/views/search.css")
    );
}

#[cfg(not(any(feature = "desktop", target_os = "ios")))]
mod linked {
    use dioxus::prelude::*;

    pub const TOKENS: Asset = asset!("/assets/styling/tokens.css");
    pub const BASE: Asset = asset!("/assets/styling/base.css");
    pub const COMPONENTS: Asset = asset!("/assets/styling/components.css");
    pub const VIEW_DISCOVER: Asset = asset!("/assets/styling/views/discover.css");
    pub const VIEW_LIBRARY: Asset = asset!("/assets/styling/views/library.css");
    pub const VIEW_PERSONAL: Asset = asset!("/assets/styling/views/personal.css");
    pub const VIEW_PLAYER: Asset = asset!("/assets/styling/views/player.css");
    pub const VIEW_SETTINGS: Asset = asset!("/assets/styling/views/settings.css");
    pub const VIEW_SEARCH: Asset = asset!("/assets/styling/views/search.css");
}

/// Injects every registered stylesheet into the document head.
#[component]
pub fn GlobalStyles() -> Element {
    #[cfg(any(feature = "desktop", target_os = "ios"))]
    {
        rsx! {
            document::Style { {inline::STAGE} }
        }
    }

    #[cfg(not(any(feature = "desktop", target_os = "ios")))]
    {
        rsx! {
            document::Stylesheet { href: linked::TOKENS }
            document::Stylesheet { href: linked::BASE }
            document::Stylesheet { href: linked::COMPONENTS }
            document::Stylesheet { href: linked::VIEW_DISCOVER }
            document::Stylesheet { href: linked::VIEW_LIBRARY }
            document::Stylesheet { href: linked::VIEW_PERSONAL }
            document::Stylesheet { href: linked::VIEW_PLAYER }
            document::Stylesheet { href: linked::VIEW_SETTINGS }
            document::Stylesheet { href: linked::VIEW_SEARCH }
        }
    }
}
