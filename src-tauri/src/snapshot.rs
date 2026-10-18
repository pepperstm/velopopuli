//! Full-window screenshots for the automated check: the web page as shown, HUD and all
//! (the check's own PNG is only the 3D canvas). macOS only; uses WebKit's own snapshot,
//! which needs no screen-recording permission.

use std::path::PathBuf;

/// Saves a PNG of the main window's web page to `path`, then calls `done` (with an error
/// message if it failed). Returns at once; the snapshot completes on the main thread.
#[cfg(target_os = "macos")]
pub fn window_png(window: &tauri::WebviewWindow, path: PathBuf, done: impl FnOnce(Result<(), String>) + Send + 'static) {
    use std::sync::Mutex;

    let done = std::sync::Arc::new(Mutex::new(Some(done)));
    let finish = {
        let done = done.clone();
        move |r: Result<(), String>| {
            if let Some(f) = done.lock().unwrap().take() {
                f(r);
            }
        }
    };
    let fail = finish.clone();
    let started = window.with_webview(move |webview| {
        use block2::RcBlock;
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
        use objc2_foundation::{NSDictionary, NSError};
        use objc2_web_kit::{WKSnapshotConfiguration, WKWebView};

        let Some(mtm) = MainThreadMarker::new() else { return finish(Err("not on the main thread".into())) };
        // SAFETY: on macOS Tauri's platform webview is a live WKWebView, used here on the main thread.
        let view: &WKWebView = unsafe { &*(webview.inner() as *const WKWebView) };
        let config = unsafe { WKSnapshotConfiguration::new(mtm) };
        let finish = finish.clone();
        let handler = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
            let result = (|| {
                if image.is_null() {
                    // SAFETY: WebKit passes either an image or an error.
                    let why = unsafe { error.as_ref() }.map_or_else(|| "no image".to_string(), |e| e.localizedDescription().to_string());
                    return Err(format!("snapshot failed: {why}"));
                }
                // SAFETY: non-null, owned by WebKit for the duration of the handler.
                let image = unsafe { &*image };
                let tiff = image.TIFFRepresentation().ok_or("no image data")?;
                let rep = NSBitmapImageRep::imageRepWithData(&tiff).ok_or("unreadable image data")?;
                let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }.ok_or("could not encode PNG")?;
                std::fs::write(&path, png.to_vec()).map_err(|e| e.to_string())
            })();
            finish(result);
        });
        unsafe { view.takeSnapshotWithConfiguration_completionHandler(Some(&config), &handler) };
    });
    if let Err(e) = started {
        fail(Err(e.to_string()));
    }
}

#[cfg(not(target_os = "macos"))]
pub fn window_png(_: &tauri::WebviewWindow, _: PathBuf, done: impl FnOnce(Result<(), String>) + Send + 'static) {
    done(Err("window snapshots are only available on macOS".into()));
}
