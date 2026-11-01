//! Reading and writing the app settings from components.
//!
//! * [`update_settings`]: the one way to change settings. It skips changes
//!   that change nothing, writes the shell's signal and saves in the
//!   background. [`update_cache_settings`] also hands the cache settings to
//!   the cache service.
//! * [`use_setting`]: one field (or a few) as a memo, so a component
//!   re-renders when that changes, not on every settings write.
//! * [`use_servers`]: the configured servers as a memo, for the same reason.

use crate::api::ServerConfig;
use crate::cache_service::apply_settings as apply_cache_settings;
use crate::db::{save_settings, AppSettings};
use dioxus::prelude::*;

/// Changes the settings with `change` and saves them. Returns false (and
/// writes nothing) when nothing changed. Safe to call from handlers and
/// effects: it reads with `peek`, so it subscribes to nothing.
pub fn update_settings(settings: Signal<AppSettings>, change: impl FnOnce(&mut AppSettings)) -> bool {
    apply(settings, change, false)
}

/// [`update_settings`], then pushes the cache settings (offline mode, cache
/// sizes) to the cache service.
pub fn update_cache_settings(
    settings: Signal<AppSettings>,
    change: impl FnOnce(&mut AppSettings),
) -> bool {
    apply(settings, change, true)
}

fn apply(mut settings: Signal<AppSettings>, change: impl FnOnce(&mut AppSettings), cache: bool) -> bool {
    let mut next = settings.peek().clone();
    change(&mut next);
    if next == *settings.peek() {
        return false;
    }
    if cache {
        apply_cache_settings(&next);
    }
    settings.set(next.clone());
    spawn(async move {
        let _ = save_settings(next).await;
    });
    true
}

/// A value read from the settings, as a memo: readers re-render only when it
/// changes. `use_setting(|s| s.offline_mode)`.
pub fn use_setting<T: PartialEq + 'static>(select: impl Fn(&AppSettings) -> T + 'static) -> Memo<T> {
    let settings = use_context::<Signal<AppSettings>>();
    use_memo(move || select(&settings.read()))
}

/// The configured servers as a memo: readers re-run only when the list
/// really changes, not every time the shell rewrites the signal.
pub fn use_servers() -> Memo<Vec<ServerConfig>> {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    use_memo(move || servers())
}
