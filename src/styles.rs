//! The single stylesheet registry.
//!
//! Every stylesheet the app loads is listed here, once. Native builds (desktop
//! and iOS) inline them with `include_str!`; the web and Android builds link
//! them with `asset!`. Order matters: legacy first, then the Stage system, so
//! Stage rules win ties.
//!
//! * LEGACY: Tailwind plus the 11 class-override themes and `app.css`. Views
//!   that phase 2 has not rebuilt still depend on them. The last phase-2 merge
//!   deletes the LEGACY entries here (both halves) and the files.
//! * STAGE: `tokens.css`, `base.css`, `components.css`, then one file per
//!   phase-2 view area. Phase-2 agents edit their own `views/*.css` file and
//!   never need to touch this registry or `main.rs`.

use dioxus::prelude::*;

#[cfg(any(feature = "desktop", target_os = "ios"))]
mod inline {
    // ---- LEGACY (delete when Tailwind goes) ----
    pub const LEGACY_TAILWIND: &str = include_str!("../assets/tailwind.css");
    pub const LEGACY_THEMES: &str = concat!(
        include_str!("../assets/styling/themes/shared/base.css"),
        "\n",
        include_str!("../assets/styling/themes/rusty/theme.css"),
        "\n",
        include_str!("../assets/styling/themes/spot/theme.css"),
        "\n",
        include_str!("../assets/styling/themes/fruit/theme.css"),
        "\n",
        include_str!("../assets/styling/themes/navi/theme.css"),
        "\n",
        include_str!("../assets/styling/themes/y2k/theme.css"),
        "\n",
        include_str!("../assets/styling/themes/aero/theme.css"),
        "\n",
        include_str!("../assets/styling/themes/aqua/theme.css"),
        "\n",
        include_str!("../assets/styling/themes/material/theme.css"),
        "\n",
        include_str!("../assets/styling/themes/fluent/theme.css"),
        "\n",
        include_str!("../assets/styling/themes/hig/theme.css")
    );
    pub const LEGACY_APP: &str = include_str!("../assets/styling/app.css");

    // ---- STAGE ----
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
        include_str!("../assets/styling/views/settings.css")
    );
}

#[cfg(not(any(feature = "desktop", target_os = "ios")))]
mod linked {
    use dioxus::prelude::*;

    // ---- LEGACY (delete when Tailwind goes) ----
    pub const LEGACY_TAILWIND: Asset = asset!("/assets/tailwind.css");
    pub const LEGACY_THEMES: Asset = asset!("/assets/styling/themes.css");
    pub const LEGACY_APP: Asset = asset!("/assets/styling/app.css");

    // ---- STAGE ----
    pub const TOKENS: Asset = asset!("/assets/styling/tokens.css");
    pub const BASE: Asset = asset!("/assets/styling/base.css");
    pub const COMPONENTS: Asset = asset!("/assets/styling/components.css");
    pub const VIEW_DISCOVER: Asset = asset!("/assets/styling/views/discover.css");
    pub const VIEW_LIBRARY: Asset = asset!("/assets/styling/views/library.css");
    pub const VIEW_PERSONAL: Asset = asset!("/assets/styling/views/personal.css");
    pub const VIEW_PLAYER: Asset = asset!("/assets/styling/views/player.css");
    pub const VIEW_SETTINGS: Asset = asset!("/assets/styling/views/settings.css");
}

/// Injects every registered stylesheet into the document head.
#[component]
pub fn GlobalStyles() -> Element {
    #[cfg(any(feature = "desktop", target_os = "ios"))]
    {
        rsx! {
            document::Style { {inline::LEGACY_TAILWIND} }
            document::Style { {inline::LEGACY_THEMES} }
            document::Style { {inline::LEGACY_APP} }
            document::Style { {inline::STAGE} }
        }
    }

    #[cfg(not(any(feature = "desktop", target_os = "ios")))]
    {
        rsx! {
            document::Stylesheet { href: linked::LEGACY_TAILWIND }
            document::Stylesheet { href: linked::LEGACY_THEMES }
            document::Stylesheet { href: linked::LEGACY_APP }
            document::Stylesheet { href: linked::TOKENS }
            document::Stylesheet { href: linked::BASE }
            document::Stylesheet { href: linked::COMPONENTS }
            document::Stylesheet { href: linked::VIEW_DISCOVER }
            document::Stylesheet { href: linked::VIEW_LIBRARY }
            document::Stylesheet { href: linked::VIEW_PERSONAL }
            document::Stylesheet { href: linked::VIEW_PLAYER }
            document::Stylesheet { href: linked::VIEW_SETTINGS }
        }
    }
}
