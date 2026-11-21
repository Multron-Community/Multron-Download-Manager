mod appearance;
mod archive;
mod askwin;
mod bridge;
mod browsers;
mod captcha;
mod clicknload;
mod clipboard;
mod cloudscan;
mod commands;
mod crawl;
mod dragout;
mod engine;
mod extras;
mod feeds;
mod flatpak;
mod i18n;
mod ipfilter;
mod manager;
mod media;
mod model;
pub mod nativehost;
mod notice;
mod portable;
mod protection;
mod resolver;
mod rss;
mod sandbox;
mod search;
mod secret;
mod security;
mod selfupdate;
mod settings;
mod spinner;
mod tor;
mod torrent;
mod trackers;
mod tray;
mod unpack;
mod updates;
mod util;
mod virustotal;
mod wallpaper;

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager as _, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

use manager::{Manager, Pending};
use model::AddRequest;

pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        set_webview_active(app, true);
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        report_visibility(app, true);
    }
}

pub fn report_visibility(app: &AppHandle, shown: bool) {
    let _ = app.emit("main-visibility", shown);
    set_webview_active(app, shown);
}

static WEBVIEW_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

fn set_webview_active(app: &AppHandle, active: bool) {
    if WEBVIEW_ACTIVE.swap(active, std::sync::atomic::Ordering::SeqCst) == active {
        return;
    }
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    #[cfg(windows)]
    let _ = window.with_webview(move |w| {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
        };
        use windows_core::Interface;
        let controller = w.controller();
        unsafe {
            let _ = controller.SetIsVisible(active);
            if let Ok(core) = controller
                .CoreWebView2()
                .and_then(|c| c.cast::<ICoreWebView2_19>())
            {
                let _ = core.SetMemoryUsageTargetLevel(if active {
                    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
                } else {
                    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
                });
            }
        }
    });
    #[cfg(not(windows))]
    let _ = window;
}

fn handle_args(app: &AppHandle, args: impl IntoIterator<Item = String>) {
    let Some(mgr) = app.try_state::<Arc<Manager>>() else {
        return;
    };
    for arg in args {
        let lower = arg.to_ascii_lowercase();
        if lower.starts_with("magnet:") {
            mgr.push_pending(Pending::Add {
                request: Box::new(AddRequest {
                    url: arg,
                    kind: Some("torrent".into()),
                    ..Default::default()
                }),
            });
        } else if lower.ends_with(".torrent") && std::path::Path::new(&arg).is_file() {
            mgr.push_pending(Pending::OpenTorrent { path: arg });
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,librqbit=warn,librqbit_dht=warn".into()),
        )
        .try_init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            show_main_window(app);
            handle_args(app, argv.into_iter().skip(1));
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            let handle = app.handle().clone();
            if let Ok(dir) = handle.path().resource_dir() {
                i18n::set_dir(dir.join("lang"));
            }
            let mgr = Manager::new(handle.clone())?;
            app.manage(mgr.clone());
            selfupdate::init(&mgr.paths.data);
            let settings = mgr.settings();
            tray::create(&handle, &settings.language)?;
            tray::start_animation(&handle);
            let refresh = handle.clone();
            std::thread::spawn(move || browsers::refresh_copies(&refresh));
            if let Some(dir) = portable::dir() {
                std::thread::spawn(move || portable::cleanup(&dir));
            }
            if let Some(w) = handle.get_webview_window("main") {
                appearance::apply_settings(&w, &settings);
            }

            let minimized =
                std::env::args().any(|a| a == "--minimized") || settings.start_minimized;
            if minimized {
                set_webview_active(&handle, false);
            } else {
                show_main_window(&handle);
            }
            bridge::restart(mgr.clone());
            clicknload::start(mgr.clone());
            tauri::async_runtime::spawn(mgr.clone().background());
            handle_args(&handle, std::env::args().skip(1));

            {
                let mgr2 = mgr.clone();
                tauri::async_runtime::spawn(async move {
                    if !cfg!(debug_assertions) || updates::test_mode() {
                        selfupdate::mandatory_update(&mgr2.app, &mgr2.api_client).await;
                    }
                    selfupdate::startup_check_done();
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
                        let info = updates::check_app(&mgr2.api_client).await;
                        if info.has_update {
                            let _ = mgr2.app.emit("app://update", info);
                            if !cfg!(debug_assertions) {
                                if let Err(e) =
                                    selfupdate::fetch_latest(&mgr2.app, &mgr2.api_client).await
                                {
                                    tracing::warn!("automatic update failed: {e:#}");
                                }
                            }
                        }
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    let mgr = window.state::<Arc<Manager>>();
                    if mgr.settings().close_to_tray {
                        api.prevent_close();
                        let _ = window.hide();
                        report_visibility(window.app_handle(), false);
                    }
                }
                WindowEvent::Resized(_) | WindowEvent::Focused(_) => {
                    let shown = window.is_visible().unwrap_or(true)
                        && !window.is_minimized().unwrap_or(false);
                    report_visibility(window.app_handle(), shown);
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap,
            commands::get_downloads,
            commands::get_global_stats,
            commands::probe_url,
            commands::check_link,
            commands::scan_page,
            commands::rss_resolve,
            commands::reset_settings,
            commands::virustotal_open,
            commands::site_rule_test,
            commands::auto_backups,
            commands::auto_backup_now,
            commands::extract_with_password,
            commands::podcast_search,
            commands::discover_feeds,
            commands::media_probe,
            commands::torrent_inspect,
            commands::add_download,
            commands::pause_download,
            commands::resume_download,
            commands::remove_download,
            commands::remove_downloads,
            commands::pause_all,
            commands::resume_all,
            commands::clear_finished,
            commands::set_threads,
            commands::set_speed_limit,
            commands::set_expected_hash,
            commands::open_file,
            commands::show_in_folder,
            commands::drag_out,
            commands::drag_out_supported,
            commands::open_path,
            commands::language_list,
            commands::language_strings,
            commands::language_reload,
            commands::language_folder,
            commands::open_url,
            commands::torrent_details,
            commands::torrent_set_files,
            commands::torrent_session_status,
            commands::start_torrent_engine,
            commands::public_trackers,
            commands::get_settings,
            commands::save_settings,
            commands::apply_category_language,
            commands::tools_status,
            commands::update_ytdlp,
            commands::install_deno,
            commands::install_ffmpeg,
            commands::check_updates,
            commands::disk_space,
            commands::reload_ip_filter,
            commands::ip_filter_status,
            commands::bridge_status,
            commands::take_pending,
            commands::approve_extension,
            commands::remove_extension,
            commands::remove_extension_everywhere,
            commands::clear_cookies,
            commands::find_sha256,
            commands::extension_dir,
            commands::browser_list,
            commands::setup_extension,
            commands::appearance_support,
            commands::apply_appearance,
            commands::move_in_queue,
            commands::smartscreen_status,
            commands::torrent_recheck,
            commands::cloud_scan_download,
            commands::cloud_scan_report,
            commands::cloud_scan_all,
            commands::find_duplicate,
            commands::notice_list,
            commands::ask_request,
            commands::extract_download,
            commands::captcha_list,
            commands::cloud_history,
            commands::set_download_meta,
            commands::set_download_mirrors,
            commands::download_history,
            commands::download_history_remove,
            commands::download_history_clear,
            commands::backup_export,
            commands::backup_import,
            commands::background_store,
            commands::background_data,
            commands::stop_all,
            commands::set_download_login,
            commands::tor_detect,
            commands::tor_check,
            commands::virustotal_check,
            commands::cloud_history_clear,
            commands::quarantine_list,
            commands::quarantine_restore,
            commands::quarantine_delete,
            commands::captcha_open,
            commands::captcha_dismiss,
            commands::extract_tool_available,
            commands::ask_started,
            commands::ask_progress,
            commands::notice_dismiss,
            commands::notice_layout,
            commands::notice_show_download,
            commands::cloud_scan_stop,
            commands::reveal_downloaded_file,
            commands::cloud_scan_with_password,
            commands::torrent_export,
            commands::create_torrent,
            commands::torrent_stream_url,
            commands::rss_feeds,
            commands::rss_refresh,
            commands::rss_download,
            commands::search_sites,
            commands::torrent_search,
            commands::download_app_update,
            commands::app_update_ready,
            commands::mandatory_update_status,
            commands::retry_mandatory_update,
            commands::install_app_update,
            commands::quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Multron Download Manager");

    app.run(|app, event| match event {
        #[cfg(target_os = "macos")]
        RunEvent::Opened { urls } => {
            let args = urls.into_iter().map(|u| {
                u.to_file_path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| u.to_string())
            });
            handle_args(app, args);
        }
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => show_main_window(app),
        RunEvent::ExitRequested { api, code, .. } => {
            if code.is_none() {
                if let Some(mgr) = app.try_state::<Arc<Manager>>() {
                    if mgr.settings().close_to_tray {
                        api.prevent_exit();
                    }
                }
            }
        }
        RunEvent::Exit => {
            if let Some(mgr) = app.try_state::<Arc<Manager>>() {
                let mgr = mgr.inner().clone();
                tauri::async_runtime::block_on(async move { mgr.shutdown().await });
                if !cfg!(debug_assertions) {
                    if let Err(e) = selfupdate::install_ready(false, false) {
                        tracing::warn!("could not install the update: {e:#}");
                    }
                }
            }
        }
        _ => {}
    });
}
