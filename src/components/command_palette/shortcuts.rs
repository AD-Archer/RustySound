//! The one keyboard shortcut registry. The page listener (`listener.js`)
//! gets the global bindings from [`listener_table`]; the palette's hints, the
//! cheat sheet and the macOS menu's accelerators read the same list; and the
//! keys other components handle themselves (Escape on every sheet, A over
//! Now Playing) are listed here too, so nothing else can claim them.
//!
//! A combo is written `mod+alt+shift+key`, in that order: `mod` is ⌘ on
//! Apple platforms and Ctrl elsewhere, `key` is the lowercased `KeyboardEvent
//! .key` (`space` for " "). Symbols that need Shift ("?") are written without
//! it; only letters and named keys carry `shift+`.

use super::actions::ActionId;

/// Where a binding works.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Anywhere in the app; the page listener runs it.
    Global,
    /// While Now Playing has focus (its own key handler).
    NowPlaying,
    /// Any open sheet, menu or overlay (their own handlers).
    Overlay,
    /// Inside the palette (its input's handler).
    Palette,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Scope::Global => "Anywhere",
            Scope::NowPlaying => "Now Playing",
            Scope::Overlay => "Sheets and Menus",
            Scope::Palette => "Command Palette",
        }
    }
}

/// What a binding does: an app action, or a key the palette handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Does {
    Action(ActionId),
    /// Palette-only keys, named for the cheat sheet.
    Palette(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub does: Does,
    /// Combos; the first is the one shown.
    pub keys: &'static [&'static str],
    pub scope: Scope,
    /// Still fires while typing in a field (only ⌘ combos that text fields
    /// don't use).
    pub while_typing: bool,
    /// Plain arrows: left to sliders, tabs, lists and other widgets that
    /// move with them.
    pub arrows: bool,
    /// Fires again while the key is held (seek, volume).
    pub repeats: bool,
}

const fn global(id: ActionId, keys: &'static [&'static str]) -> Binding {
    Binding { does: Does::Action(id), keys, scope: Scope::Global, while_typing: false, arrows: false, repeats: false }
}

const fn typing(mut b: Binding) -> Binding {
    b.while_typing = true;
    b
}

const fn held_arrow(mut b: Binding) -> Binding {
    b.arrows = true;
    b.repeats = true;
    b
}

const fn local(does: Does, keys: &'static [&'static str], scope: Scope) -> Binding {
    Binding { does, keys, scope, while_typing: true, arrows: false, repeats: false }
}

/// Every binding, in cheat sheet order.
pub const BINDINGS: &[Binding] = &[
    typing(global(ActionId::Palette, &["mod+k"])),
    // "/" opens it too, but not while typing.
    global(ActionId::Palette, &["/"]),
    global(ActionId::PlayPause, &["space"]),
    global(ActionId::Next, &["shift+arrowright", "mod+arrowright"]),
    global(ActionId::Previous, &["shift+arrowleft", "mod+arrowleft"]),
    held_arrow(global(ActionId::SeekForward, &["arrowright"])),
    held_arrow(global(ActionId::SeekBack, &["arrowleft"])),
    held_arrow(global(ActionId::VolumeUp, &["arrowup"])),
    held_arrow(global(ActionId::VolumeDown, &["arrowdown"])),
    global(ActionId::Like, &["l"]),
    global(ActionId::Shuffle, &["s"]),
    global(ActionId::Repeat, &["r"]),
    typing(global(ActionId::GoSearch, &["mod+f"])),
    typing(global(ActionId::GoSettings, &["mod+,"])),
    typing(global(ActionId::Lyrics, &["mod+l"])),
    typing(global(ActionId::UpNext, &["mod+u"])),
    global(ActionId::Shortcuts, &["?"]),
    local(Does::Action(ActionId::Ambient), &["a"], Scope::NowPlaying),
    local(Does::Action(ActionId::Close), &["escape"], Scope::Overlay),
    local(Does::Palette("Move"), &["arrowdown", "arrowup"], Scope::Palette),
    local(Does::Palette("Run"), &["enter"], Scope::Palette),
    local(Does::Palette("Alternate action (Play Next)"), &["mod+enter"], Scope::Palette),
    local(Does::Palette("Next section"), &["tab", "shift+tab"], Scope::Palette),
    local(Does::Palette("Back, then close"), &["escape"], Scope::Palette),
];

/// The combo `KeyboardEvent` fields make (`listener.js` builds the same
/// string). `primary` is ⌘ on Apple platforms, Ctrl elsewhere.
pub fn combo(key: &str, primary: bool, alt: bool, shift: bool) -> String {
    let mut key = key.to_lowercase();
    if key == " " || key == "spacebar" {
        key = "space".to_string();
    }
    let mut chars = key.chars();
    let single = chars.next().is_some() && chars.next().is_none();
    // "?" already says Shift; "L" with Shift is "shift+l".
    let shift = shift && !(single && !key.chars().all(char::is_alphabetic));
    let mut out = String::new();
    if primary {
        out.push_str("mod+");
    }
    if alt {
        out.push_str("alt+");
    }
    if shift {
        out.push_str("shift+");
    }
    out.push_str(&key);
    out
}

/// The global binding a combo runs, unless the user is typing and the
/// binding stays out of fields.
pub fn resolve(combo: &str, typing: bool) -> Option<ActionId> {
    BINDINGS
        .iter()
        .filter(|b| b.scope == Scope::Global && (!typing || b.while_typing))
        .find(|b| b.keys.contains(&combo))
        .and_then(|b| match b.does {
            Does::Action(id) => Some(id),
            Does::Palette(_) => None,
        })
}

/// Whether a component's key event is `id`'s binding (for the keys
/// components handle themselves, like A over Now Playing). Either ⌘ or Ctrl
/// counts as `mod` here.
pub fn event_runs(id: ActionId, evt: &dioxus::prelude::KeyboardEvent) -> bool {
    use dioxus::prelude::{Modifiers, ModifiersInteraction};
    let mods = evt.modifiers();
    let pressed = combo(
        &evt.key().to_string(),
        mods.intersects(Modifiers::META | Modifiers::CONTROL),
        mods.contains(Modifiers::ALT),
        mods.contains(Modifiers::SHIFT),
    );
    BINDINGS
        .iter()
        .any(|b| b.does == Does::Action(id) && b.keys.contains(&pressed.as_str()))
}

/// The shown shortcut of an action, if it has one.
pub fn primary_combo(id: ActionId) -> Option<&'static str> {
    BINDINGS
        .iter()
        .find(|b| b.does == Does::Action(id))
        .and_then(|b| b.keys.first().copied())
}

/// A combo as the platform writes it: "⌘K", "⇧→" on Apple platforms;
/// "Ctrl+K", "Shift+→" elsewhere.
pub fn label(combo: &str, apple: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    for part in combo.split('+').filter(|p| !p.is_empty()) {
        let text = match (part, apple) {
            ("mod", true) => "⌘".to_string(),
            ("mod", false) => "Ctrl".to_string(),
            ("alt", true) => "⌥".to_string(),
            ("alt", false) => "Alt".to_string(),
            ("shift", true) => "⇧".to_string(),
            ("shift", false) => "Shift".to_string(),
            ("arrowleft", _) => "←".to_string(),
            ("arrowright", _) => "→".to_string(),
            ("arrowup", _) => "↑".to_string(),
            ("arrowdown", _) => "↓".to_string(),
            ("enter", true) => "↩".to_string(),
            ("enter", false) => "Enter".to_string(),
            ("escape", _) => "Esc".to_string(),
            ("space", _) => "Space".to_string(),
            ("tab", _) => "Tab".to_string(),
            (other, _) => other.to_uppercase(),
        };
        parts.push(text);
    }
    if apple {
        parts.concat()
    } else {
        parts.join("+")
    }
}

/// The global bindings as JSON for `listener.js`:
/// `{ combo: [action key, while typing, arrows, repeats] }`.
pub fn listener_table() -> String {
    let mut entries = Vec::new();
    for binding in BINDINGS.iter().filter(|b| b.scope == Scope::Global) {
        let Does::Action(id) = binding.does else { continue };
        for key in binding.keys {
            entries.push(format!(
                "{}:[{},{},{},{}]",
                serde_json::to_string(key).unwrap_or_default(),
                serde_json::to_string(id.key()).unwrap_or_default(),
                binding.while_typing,
                binding.arrows,
                binding.repeats
            ));
        }
    }
    format!("{{{}}}", entries.join(","))
}

/// The macOS menu accelerator for an action: its first ⌘ combo. Plain keys
/// never go in the menu, which would take them from text fields.
#[cfg(all(feature = "desktop", target_os = "macos"))]
pub fn menu_accelerator(id: ActionId) -> Option<dioxus::desktop::muda::accelerator::Accelerator> {
    use dioxus::desktop::muda::accelerator::{Accelerator, Code, Modifiers, CMD_OR_CTRL};
    let combo = BINDINGS
        .iter()
        .find(|b| b.does == Does::Action(id))?
        .keys
        .iter()
        .find(|k| k.starts_with("mod+"))?;
    let mut mods = CMD_OR_CTRL;
    let mut key = "";
    for part in combo.split('+') {
        match part {
            "mod" => {}
            "alt" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            other => key = other,
        }
    }
    let code = match key {
        "," => Code::Comma,
        "arrowleft" => Code::ArrowLeft,
        "arrowright" => Code::ArrowRight,
        "arrowup" => Code::ArrowUp,
        "arrowdown" => Code::ArrowDown,
        letter if letter.len() == 1 => {
            let upper = letter.to_ascii_uppercase();
            format!("Key{upper}").parse::<Code>().ok()?
        }
        _ => return None,
    };
    Some(Accelerator::new(Some(mods), code))
}

/// A macOS menu item that runs `id` (`rustysound-action-<key>`, picked up
/// by `CommandPaletteHost`), with the registry's accelerator.
#[cfg(all(feature = "desktop", target_os = "macos"))]
pub fn menu_item(id: ActionId, label: &str) -> dioxus::desktop::muda::MenuItem {
    dioxus::desktop::muda::MenuItem::with_id(
        format!("rustysound-action-{}", id.key()),
        label,
        true,
        menu_accelerator(id),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn no_combo_is_bound_twice_where_both_could_fire() {
        // Within a scope every combo is unique; a global combo is live
        // everywhere, so no other scope may reuse it either.
        let mut global = HashSet::new();
        for b in BINDINGS.iter().filter(|b| b.scope == Scope::Global) {
            for key in b.keys {
                assert!(global.insert(*key), "{key} bound twice globally");
            }
        }
        for scope in [Scope::NowPlaying, Scope::Overlay, Scope::Palette] {
            let mut seen = HashSet::new();
            for b in BINDINGS.iter().filter(|b| b.scope == scope) {
                for key in b.keys {
                    assert!(seen.insert(*key), "{key} bound twice in {scope:?}");
                    if scope != Scope::Palette {
                        // The palette takes every key while open (the listener
                        // stands down), so only it may shadow global keys.
                        assert!(!global.contains(key), "{key} in {scope:?} shadows a global key");
                    }
                }
            }
        }
    }

    #[test]
    fn typing_keeps_plain_keys_for_the_field() {
        assert_eq!(resolve("space", false), Some(ActionId::PlayPause));
        assert_eq!(resolve("space", true), None);
        assert_eq!(resolve("l", true), None);
        assert_eq!(resolve("/", true), None);
        assert_eq!(resolve("arrowleft", true), None);
        assert_eq!(resolve("mod+arrowleft", true), None, "⌘← moves the caret");
        assert_eq!(resolve("mod+k", true), Some(ActionId::Palette));
        assert_eq!(resolve("mod+f", true), Some(ActionId::GoSearch));
        // Keys other components handle never reach the global listener.
        assert_eq!(resolve("a", false), None);
        assert_eq!(resolve("escape", false), None);
        // Every plain key is typing-safe: only ⌘ combos fire in fields.
        for b in BINDINGS.iter().filter(|b| b.scope == Scope::Global && b.while_typing) {
            assert!(b.keys.iter().all(|k| k.starts_with("mod+")), "{:?}", b.does);
        }
    }

    #[test]
    fn combos_match_what_the_listener_builds() {
        assert_eq!(combo(" ", false, false, false), "space");
        assert_eq!(combo("K", true, false, false), "mod+k");
        assert_eq!(combo("?", false, false, true), "?");
        assert_eq!(combo("L", false, false, true), "shift+l");
        assert_eq!(combo("ArrowRight", false, false, true), "shift+arrowright");
        assert_eq!(combo(",", true, false, false), "mod+,");
        assert_eq!(resolve(&combo("?", false, false, true), false), Some(ActionId::Shortcuts));
        assert_eq!(resolve(&combo("ArrowRight", false, false, true), false), Some(ActionId::Next));
    }

    #[test]
    fn labels_per_platform() {
        assert_eq!(label("mod+k", true), "⌘K");
        assert_eq!(label("mod+k", false), "Ctrl+K");
        assert_eq!(label("shift+arrowright", true), "⇧→");
        assert_eq!(label("mod+,", true), "⌘,");
        assert_eq!(label("mod+,", false), "Ctrl+,");
        assert_eq!(label("?", true), "?");
        assert_eq!(label("space", false), "Space");
        assert_eq!(primary_combo(ActionId::Next), Some("shift+arrowright"));
        assert_eq!(primary_combo(ActionId::GoHome), None);
    }

    #[test]
    fn listener_table_is_json_of_global_keys() {
        let table: serde_json::Value = serde_json::from_str(&listener_table()).unwrap();
        assert_eq!(table["space"][0], "play-pause");
        assert_eq!(table["mod+k"][1], true);
        assert_eq!(table["arrowup"][2], true);
        assert!(table.get("escape").is_none());
        for value in table.as_object().unwrap().values() {
            assert!(ActionId::from_key(value[0].as_str().unwrap()).is_some());
        }
    }
}
