//! Overlay mode: during a ride the window turns into a small see-through panel that floats
//! above other apps, showing only the numbers and the map, so the rider can watch a video or
//! a stream in another window. Leaving it puts the window back the way it was.

use serde::{Deserialize, Serialize};
use tauri::{LogicalSize, Manager, PhysicalPosition, PhysicalSize, Size, WebviewWindow};

use crate::store::Store;

/// The panel's size the first time, in logical pixels.
const PANEL: (f64, f64) = (320.0, 600.0);
/// How far the panel stands in from the screen's top-right corner the first time.
const MARGIN: f64 = 24.0;
/// The panel can't be made smaller than this.
const PANEL_MIN: (f64, f64) = (240.0, 260.0);
/// The ordinary window's smallest size (as in `tauri.conf.json`).
const NORMAL_MIN: (f64, f64) = (1000.0, 640.0);
/// Where the panel was last left, so it comes back in the same place.
const STORE_KEY: &str = "overlay-window";

/// The ordinary window, to put back when the overlay closes.
pub struct Normal {
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    maximized: bool,
}

#[derive(Serialize, Deserialize)]
struct Bounds {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn err(e: tauri::Error) -> String {
    e.to_string()
}

/// Turns the window into the floating panel. Returns what to put back afterwards.
pub fn enter(window: &WebviewWindow, store: &Store) -> Result<Normal, String> {
    let normal = Normal { position: window.outer_position().map_err(err)?, size: window.inner_size().map_err(err)?, maximized: window.is_maximized().unwrap_or(false) };
    if window.is_fullscreen().unwrap_or(false) {
        window.set_fullscreen(false).map_err(err)?;
    }
    if normal.maximized {
        window.unmaximize().map_err(err)?;
    }
    window.set_min_size(Some(Size::Logical(LogicalSize::new(PANEL_MIN.0, PANEL_MIN.1)))).map_err(err)?;
    window.set_decorations(false).map_err(err)?;
    // Without a frame the shadow would outline the see-through parts.
    let _ = window.set_shadow(false);
    window.set_always_on_top(true).map_err(err)?;
    // On every desktop, so it stays with the video when the rider switches.
    let _ = window.set_visible_on_all_workspaces(true);

    let saved = store.get::<Bounds>(STORE_KEY).filter(|b| b.width > 0 && b.height > 0 && on_a_screen(window, b));
    match saved {
        Some(b) => {
            window.set_size(PhysicalSize::new(b.width, b.height)).map_err(err)?;
            window.set_position(PhysicalPosition::new(b.x, b.y)).map_err(err)?;
        }
        None => {
            window.set_size(LogicalSize::new(PANEL.0, PANEL.1)).map_err(err)?;
            // The top-right corner of the screen the window is on.
            if let Ok(Some(monitor)) = window.current_monitor() {
                let scale = monitor.scale_factor();
                let (at, size) = (monitor.position(), monitor.size());
                let x = at.x + size.width as i32 - ((PANEL.0 + MARGIN) * scale) as i32;
                let y = at.y + (MARGIN * 2.0 * scale) as i32;
                window.set_position(PhysicalPosition::new(x, y)).map_err(err)?;
            }
        }
    }
    let _ = window.set_focus();
    Ok(normal)
}

/// Puts the ordinary window back, and remembers where the panel was.
pub fn leave(window: &WebviewWindow, store: &Store, normal: Normal) -> Result<(), String> {
    if let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) {
        let _ = store.set(STORE_KEY, &Bounds { x: position.x, y: position.y, width: size.width, height: size.height });
    }
    let _ = window.set_visible_on_all_workspaces(false);
    window.set_always_on_top(false).map_err(err)?;
    window.set_decorations(true).map_err(err)?;
    let _ = window.set_shadow(true);
    window.set_size(normal.size).map_err(err)?;
    window.set_position(normal.position).map_err(err)?;
    window.set_min_size(Some(Size::Logical(LogicalSize::new(NORMAL_MIN.0, NORMAL_MIN.1)))).map_err(err)?;
    if normal.maximized {
        window.maximize().map_err(err)?;
    }
    let _ = window.set_focus();
    Ok(())
}

/// Whether some of the saved panel is on a screen that is still connected.
fn on_a_screen(window: &WebviewWindow, b: &Bounds) -> bool {
    let Ok(monitors) = window.available_monitors() else { return true };
    monitors.iter().any(|m| {
        let (at, size) = (m.position(), m.size());
        let (right, bottom) = (at.x + size.width as i32, at.y + size.height as i32);
        b.x + (b.width as i32) > at.x + 40 && b.x < right - 40 && b.y >= at.y - 10 && b.y < bottom - 40
    })
}

/// The main window, for the commands.
pub fn main_window(app: &tauri::AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("main").ok_or_else(|| "No main window.".to_string())
}
