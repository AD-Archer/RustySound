//! Desktop window setup: default and minimum size, the macOS unified
//! titlebar, and the last window size and position restored at launch.
//!
//! The geometry is a small JSON file next to the database
//! (`window.json`), saved after the window stops moving or resizing.

use crate::storage::app_data_dir;
use dioxus::desktop::tao::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
use dioxus::desktop::tao::window::Window;
use dioxus::desktop::WindowBuilder;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Below this the wide layout (sidebar + player bar) stops fitting.
pub const MIN_WIDTH: f64 = 900.0;
pub const MIN_HEIGHT: f64 = 600.0;
const DEFAULT_WIDTH: f64 = 1280.0;
const DEFAULT_HEIGHT: f64 = 820.0;
/// Quiet period after the last move/resize event before the file is written.
const SAVE_DEBOUNCE_MS: u64 = 600;

/// Logical (point) geometry of the window's content area and outer origin.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
}

impl WindowGeometry {
    /// Clamps the size to the minimum; None for nonsense values.
    fn sanitized(self) -> Option<Self> {
        let finite = [self.width, self.height, self.x, self.y]
            .iter()
            .all(|value| value.is_finite());
        if !finite || self.width <= 0.0 || self.height <= 0.0 {
            return None;
        }
        Some(Self {
            width: self.width.max(MIN_WIDTH),
            height: self.height.max(MIN_HEIGHT),
            ..self
        })
    }
}

fn geometry_path() -> Option<PathBuf> {
    app_data_dir().map(|dir| dir.join("window.json"))
}

fn load_geometry() -> Option<WindowGeometry> {
    let raw = std::fs::read_to_string(geometry_path()?).ok()?;
    serde_json::from_str::<WindowGeometry>(&raw)
        .ok()?
        .sanitized()
}

fn save_geometry(geometry: WindowGeometry) {
    let Some(path) = geometry_path() else {
        return;
    };
    if let Ok(json) = serde_json::to_string(&geometry) {
        let _ = std::fs::write(path, json);
    }
}

/// The main window: title, size (restored or default), minimum size, and on
/// macOS a transparent, title-less titlebar over a full-size content view so
/// the sidebar runs up under the traffic lights.
pub fn main_window() -> WindowBuilder {
    let saved = load_geometry();
    let size = saved
        .map(|g| LogicalSize::new(g.width, g.height))
        .unwrap_or_else(|| LogicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT));

    let mut window = WindowBuilder::new()
        .with_title("RustySound")
        .with_min_inner_size(LogicalSize::new(MIN_WIDTH, MIN_HEIGHT))
        .with_inner_size(size);
    if let Some(geometry) = saved {
        window = window.with_position(LogicalPosition::new(geometry.x, geometry.y));
    }

    #[cfg(target_os = "macos")]
    {
        use dioxus::desktop::tao::platform::macos::WindowBuilderExtMacOS;
        window = window
            .with_titlebar_transparent(true)
            .with_title_hidden(true)
            .with_fullsize_content_view(true);
    }

    window
}

/// Runs once the window exists (before it is shown): a restored position
/// that no longer lands on any screen (a monitor was unplugged) is dropped
/// and the window centred on the primary monitor instead.
pub fn keep_on_screen(window: &Window) {
    let Ok(origin) = window.outer_position() else {
        return;
    };
    let size = window.outer_size();
    // At least a 120x40 px patch of the titlebar must be on some monitor.
    let visible = window.available_monitors().any(|monitor| {
        let (mx, my) = (monitor.position().x, monitor.position().y);
        let (mw, mh) = (monitor.size().width as i32, monitor.size().height as i32);
        let right = origin.x + (size.width as i32).min(120);
        origin.x < mx + mw && right > mx && origin.y >= my - 8 && origin.y + 40 < my + mh
    });
    if visible {
        return;
    }
    if let Some(monitor) = window.primary_monitor().or_else(|| window.current_monitor()) {
        let x = monitor.position().x + (monitor.size().width as i32 - size.width as i32).max(0) / 2;
        let y = monitor.position().y + (monitor.size().height as i32 - size.height as i32).max(0) / 2;
        window.set_outer_position(dioxus::desktop::tao::dpi::PhysicalPosition::new(x, y));
    }
}

/// Saves the window's size and position after it settles from a move or
/// resize. Skips fullscreen and zoomed (maximized) states, so un-zooming
/// later returns to the user's own size.
pub fn use_window_geometry_persistence() {
    use dioxus::desktop::tao::event::{Event, WindowEvent};
    use dioxus::desktop::{use_wry_event_handler, window};

    let mut generation = use_signal(|| 0u64);
    // The size from the last Resized event. Under Stage Manager tao's
    // inner_size() can disagree with what the window was actually given.
    let mut last_size = use_signal(|| None::<PhysicalSize<u32>>);
    use_wry_event_handler(move |event, _| {
        let Event::WindowEvent { event, .. } = event else {
            return;
        };
        match event {
            WindowEvent::Resized(size) => last_size.set(Some(*size)),
            WindowEvent::Moved(_) => {}
            _ => return,
        }
        let ticket = *generation.peek() + 1;
        generation.set(ticket);
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(SAVE_DEBOUNCE_MS)).await;
            if *generation.peek() != ticket {
                return;
            }
            let desktop = window();
            let tao_window = &desktop.window;
            if tao_window.fullscreen().is_some() || tao_window.is_maximized() || tao_window.is_minimized() {
                return;
            }
            let scale = tao_window.scale_factor();
            let physical = last_size.peek().unwrap_or_else(|| tao_window.inner_size());
            let size: LogicalSize<f64> = physical.to_logical(scale);
            let Ok(origin) = tao_window.outer_position() else {
                return;
            };
            let origin: LogicalPosition<f64> = origin.to_logical(scale);
            save_geometry(WindowGeometry {
                width: size.width,
                height: size.height,
                x: origin.x,
                y: origin.y,
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_is_clamped_to_the_minimum() {
        let small = WindowGeometry { width: 360.0, height: 300.0, x: 10.0, y: 20.0 };
        let fixed = small.sanitized().unwrap();
        assert_eq!((fixed.width, fixed.height), (MIN_WIDTH, MIN_HEIGHT));
        assert_eq!((fixed.x, fixed.y), (10.0, 20.0));
    }

    #[test]
    fn nonsense_geometry_is_ignored() {
        let bad = WindowGeometry { width: f64::NAN, height: 700.0, x: 0.0, y: 0.0 };
        assert!(bad.sanitized().is_none());
        let zero = WindowGeometry { width: 0.0, height: 700.0, x: 0.0, y: 0.0 };
        assert!(zero.sanitized().is_none());
        let parsed: WindowGeometry =
            serde_json::from_str(r#"{"width":1400,"height":900,"x":-1200,"y":40}"#).unwrap();
        assert_eq!(parsed.sanitized().unwrap().x, -1200.0);
    }
}
