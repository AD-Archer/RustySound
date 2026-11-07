//! "Keyboard Shortcuts": every binding in the registry, by where it works.
//! Opens from `?`, the palette, Settings › Advanced and the macOS Help menu.

use super::shortcuts::{label, Does, Scope, BINDINGS};
use super::CommandPalette;
use crate::components::ui::Sheet;
use dioxus::prelude::*;

#[component]
pub(super) fn CheatSheet(on_close: EventHandler<()>) -> Element {
    let apple = use_context::<CommandPalette>().apple();
    // Give focus back to where it was, as the palette does.
    use_hook(|| {
        let _ = document::eval("window.__rsCheatReturn = document.activeElement;");
    });
    use_drop(|| {
        let _ = document::eval(
            r#"(() => {
  const el = window.__rsCheatReturn;
  window.__rsCheatReturn = null;
  if (el && el.isConnected && typeof el.focus === "function") el.focus({ preventScroll: true });
})();"#,
        );
    });

    rsx! {
        Sheet {
            title: "Keyboard Shortcuts".to_string(),
            testid: "shortcut-sheet".to_string(),
            on_close: move |_| on_close.call(()),
            for scope in [Scope::Global, Scope::NowPlaying, Scope::Overlay, Scope::Palette] {
                section { key: "{scope.label()}", class: "rs-shortcuts",
                    h3 { class: "rs-shortcuts__group", "{scope.label()}" }
                    dl { class: "rs-shortcuts__list",
                        for (name, keys) in rows(scope) {
                            div { key: "{name}", class: "rs-shortcuts__row",
                                dt { "{name}" }
                                dd {
                                    for (index, combo) in keys.iter().enumerate() {
                                        if index > 0 {
                                            span { class: "rs-shortcuts__or", " or " }
                                        }
                                        kbd { "{label(combo, apple)}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A scope's bindings as (name, keys), one row per action.
fn rows(scope: Scope) -> Vec<(&'static str, Vec<&'static str>)> {
    let mut out: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    for binding in BINDINGS.iter().filter(|b| b.scope == scope) {
        let name = match binding.does {
            Does::Action(id) => id.name(),
            Does::Palette(name) => name,
        };
        match out.iter_mut().find(|(n, _)| *n == name) {
            Some((_, keys)) => keys.extend(binding.keys.iter().copied()),
            None => out.push((name, binding.keys.to_vec())),
        }
    }
    out
}
