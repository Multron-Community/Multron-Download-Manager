use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use tauri::window::Color;
use tauri::{AppHandle, Manager, WebviewUrl, WindowEvent};

use crate::model::AddRequest;

pub const PREFIX: &str = "ask-";
const WIDTH: f64 = 560.0;
const HEIGHT: f64 = 600.0;

static NEXT: AtomicU64 = AtomicU64::new(1);
static REQUESTS: Mutex<Option<HashMap<u64, AddRequest>>> = Mutex::new(None);
static PROGRESS: Mutex<Option<HashMap<u64, String>>> = Mutex::new(None);

pub fn progress(key: u64) -> Option<String> {
    PROGRESS.lock().as_ref().and_then(|m| m.get(&key).cloned())
}

pub fn open_progress(app: &AppHandle, id: &str) {
    let key = NEXT.fetch_add(1, Ordering::Relaxed);
    PROGRESS
        .lock()
        .get_or_insert_with(HashMap::new)
        .insert(key, id.to_string());
    let see_through = transparent_wanted(app);
    let background = if see_through {
        Color(0, 0, 0, 0)
    } else if crate::notice::dark(app) {
        Color(0x1c, 0x1c, 0x1e, 0xff)
    } else {
        Color(0xf6, 0xf6, 0xf7, 0xff)
    };
    let app = app.clone();
    std::thread::spawn(move || {
        let label = format!("{PREFIX}{key}");
        let built = tauri::WebviewWindowBuilder::new(
            &app,
            &label,
            WebviewUrl::App(format!("index.html#ask={key}").into()),
        )
        .title("Multron Download Manager")
        .inner_size(560.0, 420.0)
        .min_inner_size(440.0, 300.0)
        .background_color(background)
        .transparent(see_through)
        .always_on_top(true)
        .focused(false)
        .visible(true)
        .build();
        match built {
            Ok(window) => {
                window.on_window_event(move |e| {
                    if let WindowEvent::Destroyed = e {
                        forget(key);
                    }
                });
            }
            Err(e) => {
                forget(key);
                tracing::warn!("could not open the progress window: {e}");
            }
        }
    });
}

static CONFIRM: Mutex<Option<HashMap<u64, String>>> = Mutex::new(None);

pub fn confirm(key: u64) -> Option<String> {
    CONFIRM.lock().as_ref().and_then(|m| m.get(&key).cloned())
}

pub fn open_confirm(app: &AppHandle, id: &str) {
    let existing = CONFIRM
        .lock()
        .as_ref()
        .and_then(|m| m.iter().find(|(_, v)| *v == id).map(|(k, _)| *k));
    if let Some(window) = existing.and_then(|k| app.get_webview_window(&format!("{PREFIX}{k}"))) {
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    let key = NEXT.fetch_add(1, Ordering::Relaxed);
    CONFIRM
        .lock()
        .get_or_insert_with(HashMap::new)
        .insert(key, id.to_string());
    let see_through = transparent_wanted(app);
    let background = if see_through {
        Color(0, 0, 0, 0)
    } else if crate::notice::dark(app) {
        Color(0x1c, 0x1c, 0x1e, 0xff)
    } else {
        Color(0xf6, 0xf6, 0xf7, 0xff)
    };
    let app = app.clone();
    std::thread::spawn(move || {
        let label = format!("{PREFIX}{key}");
        let built = tauri::WebviewWindowBuilder::new(
            &app,
            &label,
            WebviewUrl::App(format!("index.html#confirm={key}").into()),
        )
        .title("Multron Download Manager")
        .inner_size(460.0, 440.0)
        .min_inner_size(380.0, 320.0)
        .center()
        .background_color(background)
        .transparent(see_through)
        .always_on_top(true)
        .focused(true)
        .visible(true)
        .build();
        match built {
            Ok(window) => {
                let _ = window.set_focus();
                window.on_window_event(move |e| {
                    if let WindowEvent::Destroyed = e {
                        forget(key);
                    }
                });
            }
            Err(e) => {
                let id = confirm(key);
                forget(key);
                tracing::warn!("could not open the confirmation window: {e}");
                crate::show_main_window(&app);
                let _ = tauri::Emitter::emit_to(
                    &app,
                    "main",
                    "app://select",
                    serde_json::json!({ "id": id, "start": true, "report": false }),
                );
            }
        }
    });
}

fn store(key: u64, req: AddRequest) {
    REQUESTS
        .lock()
        .get_or_insert_with(HashMap::new)
        .insert(key, req);
}

pub fn request(key: u64) -> Option<AddRequest> {
    REQUESTS.lock().as_ref().and_then(|m| m.get(&key).cloned())
}

pub fn forget(key: u64) {
    if let Some(m) = REQUESTS.lock().as_mut() {
        m.remove(&key);
    }
    if let Some(m) = PROGRESS.lock().as_mut() {
        m.remove(&key);
    }
    if let Some(m) = CONFIRM.lock().as_mut() {
        m.remove(&key);
    }
}

fn transparent_wanted(app: &AppHandle) -> bool {
    cfg!(any(windows, target_os = "macos"))
        && app
            .try_state::<std::sync::Arc<crate::manager::Manager>>()
            .is_some_and(|mgr| mgr.settings().transparency)
}

pub fn key_of(label: &str) -> Option<u64> {
    label.strip_prefix(PREFIX).and_then(|k| k.parse().ok())
}

pub fn open(app: &AppHandle, req: AddRequest) {
    let key = NEXT.fetch_add(1, Ordering::Relaxed);
    store(key, req);
    let see_through = transparent_wanted(app);
    let background = if see_through {
        Color(0, 0, 0, 0)
    } else if crate::notice::dark(app) {
        Color(0x1c, 0x1c, 0x1e, 0xff)
    } else {
        Color(0xf6, 0xf6, 0xf7, 0xff)
    };
    let app = app.clone();
    std::thread::spawn(move || {
        let label = format!("{PREFIX}{key}");
        let built = tauri::WebviewWindowBuilder::new(
            &app,
            &label,
            WebviewUrl::App(format!("index.html#ask={key}").into()),
        )
        .title("Multron Download Manager")
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(440.0, 300.0)
        .center()
        .background_color(background)
        .transparent(see_through)
        .always_on_top(true)
        .focused(true)
        .visible(true)
        .build();
        match built {
            Ok(window) => {
                let _ = window.set_focus();
                window.on_window_event(move |e| {
                    if let WindowEvent::Destroyed = e {
                        forget(key);
                    }
                });
            }
            Err(e) => {
                let req = request(key);
                forget(key);
                tracing::warn!("could not open the download window: {e}");
                if let Some(mgr) = app.try_state::<std::sync::Arc<crate::manager::Manager>>() {
                    if let Some(req) = req {
                        mgr.push_pending(crate::manager::Pending::Add {
                            request: Box::new(req),
                        });
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_requests_until_the_window_is_gone() {
        store(
            900,
            AddRequest {
                url: "https://example.com/a.zip".into(),
                ..Default::default()
            },
        );
        assert_eq!(key_of("ask-900"), Some(900));
        assert_eq!(key_of("main"), None);
        assert_eq!(request(900).unwrap().url, "https://example.com/a.zip");
        forget(900);
        assert!(request(900).is_none());
    }
}
