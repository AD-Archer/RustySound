//! Settings → About: version, project link, licence.

use super::shared::{Footnote, ValueRow};
use crate::components::ui::env::platform_name;
use crate::components::Icon;
use dioxus::prelude::*;

const PROJECT_URL: &str = "https://github.com/AD-Archer/RustySound";

#[component]
pub(super) fn AboutSection() -> Element {
    let platform = match platform_name() {
        "ios" => "iOS",
        "macos" => "macOS",
        "windows" => "Windows",
        "android" => "Android",
        "linux" => "Linux",
        _ => "Web",
    };
    rsx! {
        ul { class: "rs-list rs-list--text",
            ValueRow { title: "Version".to_string(), value: env!("CARGO_PKG_VERSION").to_string() }
            ValueRow { title: "Platform".to_string(), value: platform.to_string() }
            ValueRow { title: "Licence".to_string(), value: "GPL-3.0-or-later".to_string() }
            li {
                a {
                    class: "rs-row rs-row--compact rs-settings-action",
                    href: PROJECT_URL,
                    target: "_blank",
                    rel: "noopener",
                    Icon { name: "external".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                    span { class: "rs-settings-action__label", "Project on GitHub" }
                }
            }
            li {
                a {
                    class: "rs-row rs-row--compact rs-settings-action",
                    href: "{PROJECT_URL}/issues",
                    target: "_blank",
                    rel: "noopener",
                    Icon { name: "info".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                    span { class: "rs-settings-action__label", "Report a Problem" }
                }
            }
        }
        Footnote {
            "RustySound is free software: you can redistribute and modify it under the GNU General Public License, version 3 or later. It comes with no warranty."
        }
    }
}
