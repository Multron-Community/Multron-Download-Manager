mod appearance;
mod archive;
mod bridge;
mod browsers;
mod cloudscan;
mod commands;
mod engine;
mod flatpak;
mod i18n;
mod ipfilter;
mod manager;
mod media;
mod model;
pub mod nativehost;
mod notice;
mod portable;
mod resolver;
mod rss;
mod security;
mod selfupdate;
mod settings;
mod spinner;
mod torrent;
mod trackers;
mod tray;
mod updates;
mod util;

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager as _, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

use manager::{Manager, Pending};
use model::AddRequest;

pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
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
            if !minimized {
                show_main_window(&handle);
            }
            bridge::restart(mgr.clone());
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
            if let WindowEvent::CloseRequested { api, .. } = event {
                let mgr = window.state::<Arc<Manager>>();
                if mgr.settings().close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap,
            commands::get_downloads,
            commands::get_global_stats,
            commands::probe_url,
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
            commands::open_path,
            commands::open_url,
            commands::torrent_details,
            commands::torrent_set_files,
            commands::torrent_session_status,
            commands::start_torrent_engine,
            commands::public_trackers,
            commands::get_settings,
            commands::save_settings,
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
