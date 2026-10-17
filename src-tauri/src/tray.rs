use std::sync::Arc;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager as _, Wry};

use crate::i18n::t;
use crate::manager::{GlobalStats, Manager, OverallProgress};

const TRAY_ID: &str = "main";

fn build_menu(app: &AppHandle, lang: &str) -> tauri::Result<Menu<Wry>> {
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "show", t(lang, "show"), true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "pause_all", t(lang, "pause_all"), true, None::<&str>)?,
            &MenuItem::with_id(app, "resume_all", t(lang, "resume_all"), true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", t(lang, "quit"), true, None::<&str>)?,
        ],
    )
}

pub fn create(app: &AppHandle, lang: &str) -> tauri::Result<()> {
    let menu = build_menu(app, lang)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Multron Download Manager")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let mgr = app.state::<Arc<Manager>>().inner().clone();
            match event.id.as_ref() {
                "show" => crate::show_main_window(app),
                "pause_all" => {
                    tauri::async_runtime::spawn(async move { mgr.pause_all().await });
                }
                "resume_all" => {
                    tauri::async_runtime::spawn(async move { mgr.resume_all().await });
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn refresh(app: &AppHandle, lang: &str) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(app, lang)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn human_speed(bps: u64) -> String {
    let b = bps as f64;
    if b >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", b / 1024.0 / 1024.0)
    } else {
        format!("{:.0} KB/s", b / 1024.0)
    }
}

static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct Frames {
    accent: String,
    overlay: Vec<Vec<u8>>,
    tray: Vec<Vec<u8>>,
}

const TRAY_SIZE: u32 = 32;
const TRAY_BADGE: u32 = 18;

#[cfg(windows)]
mod overlay {
    use tauri::image::Image;
    use tauri::{AppHandle, Manager as _};

    const SIZE: u32 = 32;

    pub fn frames(color: [u8; 3]) -> Vec<Vec<u8>> {
        (0..crate::spinner::FRAMES)
            .map(|f| crate::spinner::badge(SIZE, f, color))
            .collect()
    }

    pub fn show(app: &AppHandle, rgba: &[u8]) {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_overlay_icon(Some(Image::new(rgba, SIZE, SIZE)));
        }
    }

    pub fn clear(app: &AppHandle) {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_overlay_icon(None);
        }
    }
}

#[cfg(not(windows))]
mod overlay {
    use tauri::AppHandle;

    pub fn frames(_color: [u8; 3]) -> Vec<Vec<u8>> {
        Vec::new()
    }

    pub fn show(_app: &AppHandle, _rgba: &[u8]) {}

    pub fn clear(_app: &AppHandle) {}
}

fn build_frames(app: &AppHandle, accent: &str) -> Frames {
    let color = crate::spinner::accent(accent);
    let base = app
        .default_window_icon()
        .map(|i| crate::spinner::downscale(i.rgba(), i.width(), i.height(), TRAY_SIZE))
        .unwrap_or_else(|| vec![0u8; (TRAY_SIZE * TRAY_SIZE * 4) as usize]);
    let tray = (0..crate::spinner::FRAMES)
        .map(|f| {
            let badge = crate::spinner::badge(TRAY_BADGE, f, color);
            crate::spinner::overlay_bottom_right(&base, TRAY_SIZE, &badge, TRAY_BADGE)
        })
        .collect();
    Frames {
        accent: accent.to_string(),
        overlay: overlay::frames(color),
        tray,
    }
}

pub fn start_animation(app: &AppHandle) {
    use std::sync::atomic::Ordering;
    use tauri::image::Image;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut frames: Option<Frames> = None;
        let mut frame = 0usize;
        let mut shown = false;
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(90));
        loop {
            tick.tick().await;
            let tray = app.tray_by_id(TRAY_ID);
            if !BUSY.load(Ordering::Relaxed) {
                if shown {
                    shown = false;
                    overlay::clear(&app);
                    if let (Some(t), Some(icon)) = (&tray, app.default_window_icon()) {
                        let _ = t.set_icon(Some(icon.clone()));
                    }
                }
                continue;
            }
            let accent = app
                .try_state::<Arc<Manager>>()
                .map(|m| m.settings().accent_color)
                .unwrap_or_default();
            if frames.as_ref().map(|f| f.accent != accent).unwrap_or(true) {
                frames = Some(build_frames(&app, &accent));
            }
            let Some(set) = &frames else { continue };
            frame = (frame + 1) % crate::spinner::FRAMES;
            if let Some(rgba) = set.overlay.get(frame) {
                overlay::show(&app, rgba);
            }
            if let Some(t) = &tray {
                let img = Image::new(&set.tray[frame], TRAY_SIZE, TRAY_SIZE);
                let _ = t.set_icon(Some(img));
            }
            shown = true;
        }
    });
}

pub fn set_progress(app: &AppHandle, progress: OverallProgress) {
    BUSY.store(
        !matches!(progress, OverallProgress::Idle),
        std::sync::atomic::Ordering::Relaxed,
    );
    use std::sync::Mutex;
    use tauri::window::{ProgressBarState, ProgressBarStatus};
    static LAST: Mutex<Option<(u8, u64)>> = Mutex::new(None);
    let key = match progress {
        OverallProgress::Idle => (0, 0),
        OverallProgress::Unknown => (1, 0),
        OverallProgress::Fraction(_) => (2, progress.percent().unwrap_or(0)),
    };
    {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if *last == Some(key) {
            return;
        }
        *last = Some(key);
    }
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let state = match progress {
        OverallProgress::Idle => ProgressBarState {
            status: Some(ProgressBarStatus::None),
            progress: None,
        },
        OverallProgress::Unknown => ProgressBarState {
            status: Some(ProgressBarStatus::Indeterminate),
            progress: None,
        },
        OverallProgress::Fraction(_) => ProgressBarState {
            status: Some(ProgressBarStatus::Normal),
            progress: Some(progress.percent().unwrap_or(0).max(1)),
        },
    };
    let _ = window.set_progress_bar(state);
}

pub fn set_tooltip(app: &AppHandle, stats: &GlobalStats) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let text = if stats.active == 0 && stats.download_speed == 0 && stats.upload_speed == 0 {
            "Multron Download Manager".to_string()
        } else {
            let pct = stats
                .progress
                .percent()
                .map(|p| format!("{p}% · "))
                .unwrap_or_default();
            format!(
                "Multron Download Manager · {pct}↓ {} · ↑ {} · {} active",
                human_speed(stats.download_speed),
                human_speed(stats.upload_speed),
                stats.active
            )
        };
        let _ = tray.set_tooltip(Some(text));
    }
}
