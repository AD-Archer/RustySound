//! Profiles in the UI (`db::profiles` keeps them): several people on one
//! device, each with their own servers, settings, queue and favourites.
//!
//! * [`ProfileRoot`] wraps the router. Switching profiles stops playback,
//!   unmounts the whole app for a frame, makes the new profile the one
//!   storage reads, and mounts it again, so every signal, task and provider
//!   (playback sync, the widgets' snapshot, play-queue sync) starts over
//!   with the new profile's servers.
//! * [`ProfileButton`]: the avatar menu in Settings and the sidebar footer.
//!   It only appears once a second profile exists, so one person never sees
//!   profiles at all.
//! * [`ProfilesGroup`]: Settings › Advanced: add, switch and remove.
//!
//! A PIN per profile is out of scope for now.

use crate::components::ui::env::sleep_ms;
use crate::components::ui::{Button, ButtonVariant, Menu, MenuAction, Sheet};
use crate::components::{use_player, Icon, PlayerCtx};
use crate::db::profiles::{self, Profile, ProfileList, DEFAULT_PROFILE_ID};
use dioxus::prelude::*;

/// Lets playback stop and the last saves land before the app unmounts.
const SETTLE_MS: u64 = 200;

/// The profile list and the switch, provided by [`ProfileRoot`].
#[derive(Clone, Copy)]
pub struct ProfileHost {
    pub list: Signal<ProfileList>,
    reloading: Signal<bool>,
}

impl ProfileHost {
    /// Stops playback, then reloads the app as profile `id`.
    pub fn switch_to(self, player: PlayerCtx, id: String) {
        if self.list.peek().active == id {
            return;
        }
        let (mut list, mut reloading) = (self.list, self.reloading);
        stop_playback(player);
        dioxus::core::spawn_forever(async move {
            sleep_ms(SETTLE_MS).await;
            // Unmount first: tasks still saving the old profile go with it.
            reloading.set(true);
            sleep_ms(16).await;
            let mut next = list.peek().clone();
            next.active = id;
            profiles::save_profiles(&next);
            list.set(next);
            reloading.set(false);
        });
    }

    /// Adds a profile and switches to it.
    pub fn create(self, player: PlayerCtx, name: &str) {
        let mut list = self.list;
        let mut next = list.peek().clone();
        let profile = next.add(name, chrono::Utc::now().timestamp_millis());
        profiles::save_profiles(&next);
        list.set(next);
        self.switch_to(player, profile.id);
    }

    /// Removes a profile that isn't the active one, with everything it saved.
    pub fn remove(self, id: &str) {
        let mut list = self.list;
        let mut next = list.peek().clone();
        if !next.remove(id) {
            return;
        }
        crate::offline_audio::remove_profile_downloads(id);
        profiles::delete_profile_data(id);
        profiles::save_profiles(&next);
        list.set(next);
    }
}

fn stop_playback(player: PlayerCtx) {
    let PlayerCtx { mut is_playing, mut now_playing, mut queue, mut queue_index, .. } = player;
    is_playing.set(false);
    now_playing.set(None);
    queue.set(Vec::new());
    queue_index.set(0);
}

/// Wraps the router; renders nothing for the frame a profile switch
/// takes, so the app mounts again from scratch.
#[component]
pub fn ProfileRoot(children: Element) -> Element {
    let list = use_signal(profiles::load_profiles);
    let reloading = use_signal(|| false);
    use_context_provider(|| ProfileHost { list, reloading });
    if reloading() {
        return rsx! {};
    }
    rsx! {
        {children}
    }
}

/// The active profile's avatar, opening a menu to switch. Nothing while
/// there is only one profile. `sidebar` adds the name beside it.
#[component]
pub fn ProfileButton(#[props(default)] sidebar: bool) -> Element {
    let host = use_context::<ProfileHost>();
    let player = use_player();
    let mut menu_at = use_signal(|| None::<(f64, f64)>);
    let mut adding = use_signal(|| false);

    let list = host.list.read().clone();
    if !list.is_shared() {
        return rsx! {};
    }
    let active = list.active_profile().clone();
    let mut actions: Vec<MenuAction> = list
        .profiles
        .iter()
        .map(|p| MenuAction::new(&format!("switch-{}", p.id), &p.name, "").checked(p.id == list.active))
        .collect();
    actions.push(MenuAction::new("add", "Add Profile…", "plus"));

    rsx! {
        button {
            r#type: "button",
            class: "rs-profile-btn",
            class: if sidebar { "rs-profile-btn--side" },
            "data-testid": if sidebar { "profile-button-side" } else { "profile-button" },
            aria_label: "Profile: {active.name}. Switch profile",
            aria_haspopup: "menu",
            onclick: move |evt| {
                let point = evt.client_coordinates();
                menu_at.set(Some((point.x, point.y + 8.0)));
            },
            Avatar { profile: active.clone() }
            if sidebar {
                span { class: "rs-truncate", "{active.name}" }
            }
        }
        if let Some(anchor) = menu_at() {
            Menu {
                title: "Profiles".to_string(),
                anchor: Some(anchor),
                testid: "profile-menu".to_string(),
                actions,
                on_action: move |key: String| {
                    if key == "add" {
                        adding.set(true);
                    } else if let Some(id) = key.strip_prefix("switch-") {
                        host.switch_to(player, id.to_string());
                    }
                },
                on_close: move |_| menu_at.set(None),
            }
        }
        if adding() {
            AddProfileSheet { on_close: move |_| adding.set(false) }
        }
    }
}

#[component]
fn Avatar(profile: Profile) -> Element {
    // A steady colour per profile, from its id.
    let hue = profile.id.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) % 360;
    rsx! {
        span {
            class: "rs-avatar",
            style: "--avatar-hue:{hue}",
            aria_hidden: "true",
            "{profile.initial()}"
        }
    }
}

/// Name a new profile; adding it switches to it.
#[component]
fn AddProfileSheet(on_close: EventHandler<()>) -> Element {
    let host = use_context::<ProfileHost>();
    let player = use_player();
    let mut name = use_signal(String::new);
    let submit = move |_| {
        host.create(player, &name.peek());
        on_close.call(());
    };
    rsx! {
        Sheet {
            title: "Add Profile".to_string(),
            testid: "profile-add-sheet".to_string(),
            on_close: move |_| on_close.call(()),
            form {
                class: "rs-library-form",
                onsubmit: move |evt| {
                    evt.prevent_default();
                    host.create(player, &name.peek());
                    on_close.call(());
                },
                label { class: "rs-library-form__label", r#for: "rs-profile-name", "Name" }
                div { class: "rs-field",
                    input {
                        id: "rs-profile-name",
                        r#type: "text",
                        value: "{name}",
                        placeholder: "Who's listening?",
                        autocomplete: "off",
                        "data-testid": "profile-name-input",
                        onmounted: move |evt| async move {
                            let _ = evt.set_focus(true).await;
                        },
                        oninput: move |evt| name.set(evt.value()),
                    }
                }
                p { class: "rs-library-form__hint",
                    "Each profile has its own servers, settings, queue and favourites. Downloads are shared."
                }
                div { class: "rs-library-form__actions",
                    Button {
                        label: "Cancel".to_string(),
                        variant: ButtonVariant::Tinted,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: "Add and Switch".to_string(),
                        variant: ButtonVariant::Filled,
                        testid: "profile-add-submit".to_string(),
                        onclick: submit,
                    }
                }
            }
        }
    }
}

/// Settings › Advanced: the profiles on this device. One quiet row until
/// a second profile exists.
#[component]
pub fn ProfilesGroup() -> Element {
    let host = use_context::<ProfileHost>();
    let player = use_player();
    let mut adding = use_signal(|| false);
    let mut confirm_remove = use_signal(|| None::<Profile>);
    let list = host.list.read().clone();

    rsx! {
        h3 { class: "rs-settings-group", "Profiles" }
        ul { class: "rs-list rs-list--text", aria_label: "Profiles",
            if list.is_shared() {
                for profile in list.profiles.iter().cloned() {
                    li { key: "{profile.id}", "data-testid": "profile-row",
                        div { class: "rs-row rs-row--compact rs-settings-row",
                            Avatar { profile: profile.clone() }
                            span { class: "rs-row__meta",
                                span { class: "rs-settings-row__title", "{profile.name}" }
                            }
                            if profile.id == list.active {
                                span { class: "rs-row__detail", "This profile" }
                            } else {
                                button {
                                    r#type: "button",
                                    class: "rs-btn rs-btn--plain rs-btn--small",
                                    "data-testid": "profile-switch",
                                    onclick: {
                                        let id = profile.id.clone();
                                        move |_| host.switch_to(player, id.clone())
                                    },
                                    "Switch"
                                }
                                if profile.id != DEFAULT_PROFILE_ID {
                                    button {
                                        r#type: "button",
                                        class: "rs-btn rs-btn--plain rs-btn--small rs-btn--danger",
                                        aria_label: "Remove {profile.name}",
                                        "data-testid": "profile-remove",
                                        onclick: {
                                            let profile = profile.clone();
                                            move |_| confirm_remove.set(Some(profile.clone()))
                                        },
                                        Icon { name: "trash".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            li {
                button {
                    r#type: "button",
                    class: "rs-row rs-row--compact rs-settings-action",
                    "data-testid": "profile-add",
                    onclick: move |_| adding.set(true),
                    Icon { name: "plus".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                    span { class: "rs-settings-action__label", "Add Profile…" }
                }
            }
        }
        p { class: "rs-list-foot rs-settings-foot",
            "Profiles keep servers, settings, the queue and favourites apart for each person using this device. Downloaded music is shared."
        }
        if adding() {
            AddProfileSheet { on_close: move |_| adding.set(false) }
        }
        if let Some(profile) = confirm_remove() {
            Menu {
                message: format!("Remove “{}”? Its servers, settings and download list are deleted from this device. Music other profiles downloaded stays.", profile.name),
                testid: "profile-remove-confirm".to_string(),
                actions: vec![MenuAction::new("remove", "Remove Profile", "trash").danger()],
                on_action: {
                    let id = profile.id.clone();
                    move |_| host.remove(&id)
                },
                on_close: move |_| confirm_remove.set(None),
            }
        }
    }
}
