use dioxus::prelude::*;

mod api;
mod backup;
mod cache;
mod cache_service;
// Casting to network speakers (UPnP/Sonos, Chromecast): Mac and iPhone.
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod cast;
// Refactor in progress: the views still call helpers marked #[deprecated]
// in api/, cache and offline_audio. Remove this allow to list them.
#[allow(deprecated)]
mod components;
mod db;
#[cfg(feature = "desktop")]
mod desktop_window;
mod diagnostics;
mod equalizer;
// The Mac menu bar item (current song, Play/Pause, Next, Previous).
#[cfg(any(test, all(feature = "desktop", target_os = "macos")))]
mod menu_bar;
// Now Playing widgets (iPhone and Mac): the snapshot the widget reads and
// the commands its buttons send.
#[cfg(any(test, target_os = "ios", all(feature = "desktop", target_os = "macos")))]
mod now_playing_widget;
// iOS 27 SDK: UIScene life-cycle bridge for tao 0.34 (see the module docs).
#[cfg(target_os = "ios")]
mod ios_scene;
mod local_music;
mod offline_art;
mod offline_audio;
mod format;
mod storage;
mod styles;
mod time;

use components::AppView;

#[cfg(feature = "desktop")]
const FAVICON_ICO: &str = "/assets/favicon.ico";
#[cfg(not(feature = "desktop"))]
const FAVICON_ICO: Asset = asset!("/assets/favicon.ico");

#[cfg(feature = "desktop")]
const FAVICON_SVG: &str = "/assets/favicon.svg";
#[cfg(not(feature = "desktop"))]
const FAVICON_SVG: Asset = asset!("/assets/favicon.svg");

#[cfg(feature = "desktop")]
const FAVICON_PNG_96: &str = "/assets/favicon-96x96.png";
#[cfg(not(feature = "desktop"))]
const FAVICON_PNG_96: Asset = asset!("/assets/favicon-96x96.png");

#[cfg(feature = "desktop")]
const APP_ICON_192: &str = "/assets/web-app-manifest-192x192.png";
#[cfg(not(feature = "desktop"))]
const APP_ICON_192: Asset = asset!("/assets/web-app-manifest-192x192.png");

#[cfg(feature = "desktop")]
const APP_ICON_512: &str = "/assets/web-app-manifest-512x512.png";
#[cfg(not(feature = "desktop"))]
const APP_ICON_512: Asset = asset!("/assets/web-app-manifest-512x512.png");

#[cfg(feature = "desktop")]
const APPLE_TOUCH_ICON: &str = "/assets/apple-touch-icon.png";
#[cfg(not(feature = "desktop"))]
const APPLE_TOUCH_ICON: Asset = asset!("/assets/apple-touch-icon.png");

#[cfg(feature = "desktop")]
const WEB_MANIFEST: &str = "/assets/site.webmanifest";
#[cfg(not(feature = "desktop"))]
const WEB_MANIFEST: Asset = asset!("/assets/site.webmanifest");

#[cfg(feature = "desktop")]
fn desktop_app_icon() -> Option<dioxus::desktop::tao::window::Icon> {
    use dioxus::desktop::tao::window::Icon;
    use std::io::Cursor;

    let mut decoder = png::Decoder::new(Cursor::new(include_bytes!(
        "../assets/web-app-manifest-512x512.png"
    )));
    decoder.set_transformations(png::Transformations::normalize_to_color8());

    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).ok()?;
    let bytes = &buffer[..info.buffer_size()];

    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity((bytes.len() / 3) * 4);
            for rgb in bytes.chunks_exact(3) {
                rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
            rgba
        }
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity(bytes.len() * 4);
            for &gray in bytes {
                rgba.extend_from_slice(&[gray, gray, gray, 255]);
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity(bytes.len() * 2);
            for gray_alpha in bytes.chunks_exact(2) {
                rgba.extend_from_slice(&[
                    gray_alpha[0],
                    gray_alpha[0],
                    gray_alpha[0],
                    gray_alpha[1],
                ]);
            }
            rgba
        }
        png::ColorType::Indexed => return None,
    };

    Icon::from_rgba(rgba, info.width, info.height).ok()
}

fn main() {
    #[cfg(feature = "desktop")]
    {
        use dioxus::desktop::Config;
        #[cfg(target_os = "linux")]
        {
            // Linux WebKit can stutter on some Wayland/driver combinations.
            // Keep conservative defaults unless user already configured them.
            if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
                unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
            }
        }

        let mut window = desktop_window::main_window();
        if let Some(icon) = desktop_app_icon() {
            window = window.with_window_icon(Some(icon.clone()));

            #[cfg(target_os = "windows")]
            {
                use dioxus::desktop::tao::platform::windows::WindowBuilderExtWindows;
                window = window.with_taskbar_icon(Some(icon));
            }
        }

        let mut config = Config::new();

        #[cfg(target_os = "macos")]
        {
            // Clicking the menu bar item opens its menu, without also
            // bringing the window forward ("Show RustySound" does that).
            config = config
                .with_menu(macos_menu_bar())
                .with_tray_icon_show_window_on_click(false);
        }

        #[cfg(not(target_os = "macos"))]
        {
            config = config.with_menu(None);
        }

        config = config
            .with_window(window)
            // The stock index titles the page "Dioxus app"; Mission Control and
            // the web inspector show document.title.
            .with_custom_index(DESKTOP_INDEX_HTML.to_string())
            .with_on_window(|window, _| desktop_window::keep_on_screen(&window))
            // Set native WebView background before HTML/CSS load to avoid startup white flash.
            .with_background_color((9, 9, 11, 255))
            // Keep this explicit on Linux to avoid known DMA-BUF rendering glitches.
            .with_disable_dma_buf_on_wayland(true);

        #[cfg(target_os = "linux")]
        {
            config = config.with_custom_head(
                r#"<style>
                /* WebKitGTK: keep scrolling cheap. */
                .main-scroll {
                  scroll-behavior: auto;
                  overscroll-behavior: contain;
                }
                </style>"#
                    .to_string(),
            );
        }

        dioxus::LaunchBuilder::desktop()
            .with_cfg(config)
            .launch(App);
        return;
    }

    // Must run before launch: UIKit instantiates the scene delegate by class name.
    #[cfg(target_os = "ios")]
    ios_scene::install();

    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App);
}

/// dioxus-desktop's index page with our title and language.
#[cfg(feature = "desktop")]
const DESKTOP_INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
    <head>
        <title>RustySound</title>
        <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
    </head>
    <body>
        <div id="main"></div>
    </body>
</html>
"#;

#[cfg(all(feature = "desktop", target_os = "macos"))]
fn macos_menu_bar() -> dioxus::desktop::muda::Menu {
    use crate::components::command_palette::{menu_accelerator, menu_item, ActionId};
    use dioxus::desktop::muda::{
        accelerator::{Accelerator, Code, Modifiers, CMD_OR_CTRL},
        Menu, MenuItem, PredefinedMenuItem, Submenu,
    };

    let menu = Menu::new();

    let app_menu = Submenu::new("RustySound", true);
    // Accelerators that are also page shortcuts come from the shortcut
    // registry (command_palette/shortcuts.rs), so menu and page agree.
    let settings_item = MenuItem::with_id(
        "rustysound-open-settings",
        "Settings...",
        true,
        menu_accelerator(ActionId::GoSettings),
    );
    app_menu
        .append_items(&[
            &PredefinedMenuItem::about(None, None),
            &PredefinedMenuItem::separator(),
            &settings_item,
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ])
        .expect("failed to build macOS app menu");

    let edit_menu = Submenu::new("Edit", true);
    edit_menu
        .append_items(&[
            &PredefinedMenuItem::undo(None),
            &PredefinedMenuItem::redo(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::cut(None),
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::paste(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::select_all(None),
            &PredefinedMenuItem::separator(),
            // Find jumps to Search, as Cmd+F does in the web build.
            &MenuItem::with_id("rustysound-find", "Find…", true, menu_accelerator(ActionId::GoSearch)),
        ])
        .expect("failed to build macOS edit menu");

    // Same items and order as the sidebar (Queue lives under Library there).
    let go_item = |id: &str, label: &str, code: Code| {
        MenuItem::with_id(id, label, true, Some(Accelerator::new(Some(CMD_OR_CTRL), code)))
    };
    let go_menu = Submenu::new("Go", true);
    go_menu
        .append_items(&[
            &menu_item(ActionId::Palette, "Command Palette…"),
            &PredefinedMenuItem::separator(),
            &go_item("rustysound-go-home", "Home", Code::Digit1),
            // ⌘L is Lyrics now; Edit › Find (⌘F) goes to Search.
            &MenuItem::with_id("rustysound-go-search", "Search", true, None),
            &PredefinedMenuItem::separator(),
            &go_item("rustysound-go-playlists", "Playlists", Code::Digit2),
            &go_item("rustysound-go-artists", "Artists", Code::Digit3),
            &go_item("rustysound-go-albums", "Albums", Code::Digit4),
            &go_item("rustysound-go-songs", "Songs", Code::Digit5),
            &go_item("rustysound-go-favorites", "Favorites", Code::Digit6),
            &go_item("rustysound-go-downloads", "Downloads", Code::Digit7),
            &PredefinedMenuItem::separator(),
            &go_item("rustysound-go-radio", "Radio", Code::Digit8),
            &go_item("rustysound-go-stats", "Stats", Code::Digit9),
            &PredefinedMenuItem::separator(),
            &go_item("rustysound-go-queue", "Queue", Code::Digit0),
        ])
        .expect("failed to build macOS Go menu");

    // Playback drives the player bar's own buttons (by id, through the app),
    // so it behaves exactly like clicking them. Space stays with the page's
    // key handler (it must not fire while typing); ⌘→ / ⌘← are taken by the
    // menu before the page sees them, so they never double-fire.
    let playback_menu = Submenu::new("Playback", true);
    playback_menu
        .append_items(&[
            &MenuItem::with_id(
                "rustysound-play-pause",
                "Play/Pause",
                true,
                Some(Accelerator::new(None, Code::Space)),
            ),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id("rustysound-next", "Next", true, menu_accelerator(ActionId::Next)),
            &MenuItem::with_id(
                "rustysound-previous",
                "Previous",
                true,
                menu_accelerator(ActionId::Previous),
            ),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(
                "rustysound-shuffle",
                "Shuffle",
                true,
                Some(Accelerator::new(Some(CMD_OR_CTRL | Modifiers::ALT), Code::KeyS)),
            ),
            &MenuItem::with_id(
                "rustysound-repeat",
                "Repeat",
                true,
                Some(Accelerator::new(Some(CMD_OR_CTRL | Modifiers::ALT), Code::KeyR)),
            ),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(
                "rustysound-now-playing",
                "Now Playing",
                true,
                Some(Accelerator::new(Some(CMD_OR_CTRL | Modifiers::ALT), Code::KeyN)),
            ),
            &menu_item(ActionId::Lyrics, "Lyrics"),
            &menu_item(ActionId::UpNext, "Up Next"),
        ])
        .expect("failed to build macOS Playback menu");

    let window_menu = Submenu::new("Window", true);
    window_menu
        .append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(None),
            &PredefinedMenuItem::fullscreen(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::close_window(None),
            &PredefinedMenuItem::bring_all_to_front(None),
        ])
        .expect("failed to build macOS window menu");
    window_menu.set_as_windows_menu_for_nsapp();

    // "?" has no ⌘ form, so the item shows no accelerator.
    let help_menu = Submenu::new("Help", true);
    help_menu
        .append_items(&[&menu_item(ActionId::Shortcuts, "Keyboard Shortcuts")])
        .expect("failed to build macOS Help menu");
    help_menu.set_as_help_menu_for_nsapp();

    menu.append_items(&[&app_menu, &edit_menu, &go_menu, &playback_menu, &window_menu, &help_menu])
        .expect("failed to attach macOS menu items");

    menu
}

#[component]
fn App() -> Element {
    #[cfg(feature = "desktop")]
    desktop_window::use_window_geometry_persistence();
    // iOS: give tao's window to the UIScene bridge (see ios_scene.rs).
    #[cfg(target_os = "ios")]
    ios_scene::use_scene_window();

    rsx! {
        document::Title { "RustySound" }
        // Favicon and icons
        document::Link { rel: "icon", r#type: "image/svg+xml", href: FAVICON_SVG }
        document::Link { rel: "shortcut icon", href: FAVICON_ICO }
        document::Link { rel: "icon", r#type: "image/x-icon", href: FAVICON_ICO }
        document::Link { rel: "icon", r#type: "image/png", sizes: "96x96", href: FAVICON_PNG_96 }
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
            href: APP_ICON_192,
        }
        document::Link {
            rel: "icon",
            r#type: "image/png",
            sizes: "512x512",
            href: APP_ICON_512,
        }

        // Web app manifest (web/mobile only)
        if !cfg!(feature = "desktop") {
            document::Link { rel: "manifest", href: WEB_MANIFEST }
        }

        // The native apps behave like apps (no pinch zoom; Dynamic Type covers text size).
        // The web build must allow zoom.
        document::Meta {
            name: "viewport",
            content: if cfg!(target_arch = "wasm32") {
                "width=device-width, initial-scale=1, viewport-fit=cover"
            } else {
                "width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover"
            },
        }
        // Theme color for mobile browsers: the Stage's black.
        document::Meta { name: "theme-color", content: "#000000" }
        document::Meta { name: "mobile-web-app-capable", content: "yes" }
        document::Meta { name: "apple-mobile-web-app-status-bar-style", content: "default" }
        document::Meta { name: "apple-mobile-web-app-title", content: "RustySound" }

        styles::GlobalStyles {}

        // Switching profiles mounts the app again under the new profile.
        components::profiles::ProfileRoot {
            Router::<AppView> {}
        }
    }
}
