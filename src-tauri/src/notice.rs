use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::window::Color;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Theme, WebviewUrl};

pub const LABEL: &str = "notice";
pub const EVT_CHANGED: &str = "notice://changed";
const WIDTH: f64 = 360.0;
const MARGIN: f64 = 14.0;
const MAX_SHOWN: usize = 4;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub id: u64,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub download: Option<String>,
    pub tone: &'static str,
}

static NEXT: AtomicU64 = AtomicU64::new(1);
static LIST: Mutex<Vec<Notice>> = Mutex::new(Vec::new());

pub fn tone_of(kind: &str, ok: bool) -> &'static str {
    match kind {
        "started" => "accent",
        "done" | "hash_ok" => "ok",
        "failed" | "threat" | "hash_bad" => "bad",
        "scan_cloud" if ok => "ok",
        "scan_cloud" => "warn",
        _ => "accent",
    }
}

fn dark(app: &AppHandle) -> bool {
    let theme = app
        .try_state::<std::sync::Arc<crate::manager::Manager>>()
        .map(|m| m.settings().theme)
        .unwrap_or_default();
    match theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => app
            .get_webview_window("main")
            .and_then(|w| w.theme().ok())
            .is_none_or(|t| t == Theme::Dark),
    }
}

#[cfg(windows)]
fn round_corners(window: &tauri::WebviewWindow) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };
    if let Ok(hwnd) = window.hwnd() {
        let preference = DWMWCP_ROUND;
        unsafe {
            DwmSetWindowAttribute(
                hwnd.0 as _,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                &preference as *const _ as *const _,
                std::mem::size_of_val(&preference) as u32,
            );
        }
    }
}

fn add(list: &mut Vec<Notice>, notice: Notice) {
    if let Some(download) = &notice.download {
        list.retain(|n| !(n.download.as_ref() == Some(download) && n.kind == notice.kind));
    }
    list.push(notice);
    while list.len() > MAX_SHOWN {
        list.remove(0);
    }
}

pub fn push(
    app: &AppHandle,
    kind: &str,
    title: String,
    body: &str,
    download: Option<&str>,
    ok: bool,
) {
    let notice = Notice {
        id: NEXT.fetch_add(1, Ordering::Relaxed),
        kind: kind.to_string(),
        title,
        body: body.to_string(),
        download: download.map(str::to_string),
        tone: tone_of(kind, ok),
    };
    add(&mut LIST.lock(), notice);
    if app.get_webview_window(LABEL).is_some() {
        let _ = app.emit_to(LABEL, EVT_CHANGED, ());
    } else {
        let app = app.clone();
        let background = if dark(&app) {
            Color(0x23, 0x23, 0x25, 0xff)
        } else {
            Color(0xff, 0xff, 0xff, 0xff)
        };
        std::thread::spawn(move || {
            let built = tauri::WebviewWindowBuilder::new(
                &app,
                LABEL,
                WebviewUrl::App("index.html#notice".into()),
            )
            .title("Multron Download Manager")
            .inner_size(WIDTH, 120.0)
            .decorations(false)
            .background_color(background)
            .shadow(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false)
            .visible(false)
            .build();
            match built {
                #[cfg(windows)]
                Ok(window) => round_corners(&window),
                #[cfg(not(windows))]
                Ok(_) => {}
                Err(e) => tracing::warn!("could not open the notice window: {e}"),
            }
        });
    }
}

pub fn list() -> Vec<Notice> {
    LIST.lock().clone()
}

pub fn dismiss(app: &AppHandle, id: u64) {
    LIST.lock().retain(|n| n.id != id);
    let _ = app.emit_to(LABEL, EVT_CHANGED, ());
}

pub fn layout(app: &AppHandle, height: f64) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(LABEL) else {
        return Ok(());
    };
    if height <= 0.0 || LIST.lock().is_empty() {
        return window.hide();
    }
    let monitor = app
        .get_webview_window("main")
        .and_then(|w| w.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return window.show();
    };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let w = (WIDTH * scale).round() as u32;
    let h = (height * scale).round() as u32;
    let margin = (MARGIN * scale).round() as i32;
    let x = area.position.x + area.size.width as i32 - w as i32 - margin;
    let y = area.position.y + area.size.height as i32 - h as i32 - margin;
    window.set_size(PhysicalSize::new(w, h))?;
    window.set_position(PhysicalPosition::new(x, y))?;
    window.set_always_on_top(true)?;
    window.show()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(id: u64, kind: &str, download: Option<&str>) -> Notice {
        Notice {
            id,
            kind: kind.into(),
            title: String::new(),
            body: String::new(),
            download: download.map(str::to_string),
            tone: "ok",
        }
    }

    #[test]
    fn keeps_a_short_list_and_replaces_older_cards_of_the_same_download() {
        let mut list = Vec::new();
        add(&mut list, n(1, "started", Some("a")));
        add(&mut list, n(2, "done", Some("a")));
        add(&mut list, n(3, "done", Some("a")));
        assert_eq!(list.iter().map(|x| x.id).collect::<Vec<_>>(), vec![1, 3]);
        for i in 4..10 {
            add(&mut list, n(i, "done", Some(&format!("d{i}"))));
        }
        assert_eq!(list.len(), MAX_SHOWN);
        assert_eq!(list.last().unwrap().id, 9);
    }

    #[test]
    fn picks_colors_by_kind() {
        assert_eq!(tone_of("done", true), "ok");
        assert_eq!(tone_of("threat", false), "bad");
        assert_eq!(tone_of("scan_cloud", true), "ok");
        assert_eq!(tone_of("scan_cloud", false), "warn");
        assert_eq!(tone_of("started", true), "accent");
    }
}
