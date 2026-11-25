use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use crate::ipfilter::FilterStatus;
use crate::manager::{BridgeStatus, GlobalStats, Manager, Pending, ProbeInfo, EVT_TOOLS};
use crate::media::{MediaInfo, ToolStatus};
use crate::model::{AddRequest, BrowserCookie, DownloadView};
use crate::settings::Settings;
use crate::torrent::{TorrentDetails, TorrentPreview, TorrentSource};
use crate::updates::{self, AppUpdate};

type Mgr<'a> = State<'a, Arc<Manager>>;
type R<T> = Result<T, String>;

pub const CONFIRM_START: &str = "confirm_start";

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn anyhow_err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    settings: Settings,
    downloads: Vec<DownloadView>,
    version: String,
    platform: String,
    arch: String,
    data_dir: String,
    tools_dir: String,
}

#[tauri::command]
pub fn get_bootstrap(mgr: Mgr<'_>) -> Bootstrap {
    Bootstrap {
        settings: mgr.settings(),
        downloads: mgr.views(),
        version: env!("CARGO_PKG_VERSION").into(),
        platform: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        data_dir: mgr.paths.data.to_string_lossy().into_owned(),
        tools_dir: mgr.paths.tools.to_string_lossy().into_owned(),
    }
}

#[tauri::command]
pub fn get_downloads(mgr: Mgr<'_>) -> Vec<DownloadView> {
    mgr.views()
}

#[tauri::command]
pub fn get_global_stats(mgr: Mgr<'_>) -> GlobalStats {
    mgr.global_stats()
}

#[tauri::command]
pub async fn probe_url(
    mgr: Mgr<'_>,
    url: String,
    referrer: Option<String>,
    cookies: Option<Vec<BrowserCookie>>,
) -> R<ProbeInfo> {
    Ok(mgr
        .probe_url(&url, referrer, cookies.as_deref().unwrap_or_default())
        .await)
}

#[tauri::command]
pub async fn check_link(
    mgr: Mgr<'_>,
    url: String,
    referrer: Option<String>,
    cookies: Option<Vec<BrowserCookie>>,
) -> R<ProbeInfo> {
    Ok(mgr
        .check_link(&url, referrer, cookies.as_deref().unwrap_or_default())
        .await)
}

#[tauri::command]
pub async fn scan_page(
    mgr: Mgr<'_>,
    url: String,
    referrer: Option<String>,
    cookies: Option<Vec<BrowserCookie>>,
    timeout: Option<u64>,
) -> R<crate::crawl::PageScan> {
    mgr.scan_page(
        &url,
        referrer,
        cookies.as_deref().unwrap_or_default(),
        timeout.unwrap_or(25),
    )
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn media_probe(
    mgr: Mgr<'_>,
    url: String,
    cookies: Option<Vec<BrowserCookie>>,
    referrer: Option<String>,
    user_agent: Option<String>,
) -> R<MediaInfo> {
    let settings = mgr.settings();
    let cookie_file = match cookies.filter(|c| !c.is_empty()) {
        Some(c) => {
            let p = mgr
                .paths
                .cookies
                .join(format!("probe-{}.txt", uuid::Uuid::new_v4()));
            crate::media::write_cookie_file(&p, &c).map_err(anyhow_err)?;
            Some(p)
        }
        None => None,
    };
    let res = mgr
        .tools
        .probe_stream(
            &url,
            cookie_file.as_deref(),
            &settings.cookies_from_browser,
            referrer.as_deref(),
            user_agent.as_deref(),
        )
        .await
        .map_err(anyhow_err);
    if let Some(p) = cookie_file {
        let _ = std::fs::remove_file(p);
    }
    res
}

#[tauri::command]
pub async fn archive_list(
    mgr: Mgr<'_>,
    url: String,
    referrer: Option<String>,
    cookies: Option<Vec<BrowserCookie>>,
) -> R<crate::remote_archive::Listing> {
    mgr.list_archive(&url, referrer, cookies.as_deref().unwrap_or_default())
        .await
        .map_err(anyhow_err)
}

#[tauri::command]
pub async fn torrent_inspect(
    mgr: Mgr<'_>,
    source: String,
    file: Option<String>,
) -> R<TorrentPreview> {
    let engine = mgr.torrent_engine().await.map_err(anyhow_err)?;
    let src = match file.filter(|f| !f.is_empty()) {
        Some(f) => TorrentSource::File(f.into()),
        None => TorrentSource::Url(source.trim().to_string()),
    };
    engine.inspect(&src).await.map_err(anyhow_err)
}

#[tauri::command]
pub async fn add_download(mgr: Mgr<'_>, request: AddRequest) -> R<String> {
    mgr.add(request).await.map_err(anyhow_err)
}

#[tauri::command]
pub async fn pause_download(mgr: Mgr<'_>, id: String) -> R<()> {
    mgr.pause(&id).await.map_err(anyhow_err)
}

#[tauri::command]
pub async fn resume_download(mgr: Mgr<'_>, id: String) -> R<()> {
    mgr.resume(&id).await.map_err(anyhow_err)
}

#[tauri::command]
pub async fn remove_download(mgr: Mgr<'_>, id: String, delete_files: bool) -> R<()> {
    mgr.remove(&id, delete_files).await.map_err(anyhow_err)
}

#[tauri::command]
pub async fn remove_downloads(mgr: Mgr<'_>, ids: Vec<String>, delete_files: bool) -> R<usize> {
    let mut removed = 0;
    for id in ids {
        if mgr.remove(&id, delete_files).await.is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

#[tauri::command]
pub async fn pause_all(mgr: Mgr<'_>) -> R<()> {
    mgr.pause_all().await;
    Ok(())
}

#[tauri::command]
pub async fn resume_all(mgr: Mgr<'_>) -> R<()> {
    mgr.resume_all().await;
    Ok(())
}

#[tauri::command]
pub fn clear_finished(mgr: Mgr<'_>) {
    mgr.clear_finished();
}

#[tauri::command]
pub fn set_threads(mgr: Mgr<'_>, id: String, threads: u32) -> R<()> {
    mgr.set_threads(&id, threads).map_err(anyhow_err)
}

#[tauri::command]
pub fn set_speed_limit(mgr: Mgr<'_>, id: String, kib: u64) -> R<()> {
    mgr.set_speed_limit(&id, kib).map_err(anyhow_err)
}

#[tauri::command]
pub async fn set_expected_hash(mgr: Mgr<'_>, id: String, hash: Option<String>) -> R<()> {
    mgr.set_expected_hash(&id, hash).await.map_err(anyhow_err)
}

#[tauri::command]
pub fn open_file(
    app: AppHandle,
    mgr: Mgr<'_>,
    id: String,
    confirmed: Option<bool>,
    skip_scan: Option<bool>,
) -> R<()> {
    mgr.open_check(
        &id,
        confirmed == Some(true),
        skip_scan == Some(true) || confirmed == Some(true),
    )
    .map_err(|e| e.to_string())?;
    let path = mgr.file_path(&id).map_err(anyhow_err)?;
    let s = mgr.settings();
    if (s.ask_before_opening_files
        || (crate::security::is_program(&path) && s.ask_before_starting_programs))
        && confirmed != Some(true)
    {
        return Err(CONFIRM_START.into());
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(err)
}

#[tauri::command]
pub fn show_in_folder(app: AppHandle, mgr: Mgr<'_>, id: String) -> R<()> {
    let path = mgr.file_path(&id).map_err(anyhow_err)?;
    if path.is_dir() {
        app.opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(err)
    } else {
        app.opener().reveal_item_in_dir(path).map_err(err)
    }
}

#[tauri::command]
pub fn drag_out(mgr: Mgr<'_>, window: tauri::WebviewWindow, ids: Vec<String>) -> R<usize> {
    let files: Vec<std::path::PathBuf> = ids
        .iter()
        .filter(|id| {
            mgr.record(id)
                .is_ok_and(|r| r.status == crate::model::Status::Completed)
        })
        .filter(|id| mgr.open_check(id, false, true).is_ok())
        .filter_map(|id| mgr.file_path(id).ok())
        .filter(|p| p.exists())
        .collect();
    let count = files.len();
    if count > 0 {
        crate::dragout::start(&window, files).map_err(anyhow_err)?;
    }
    Ok(count)
}

#[tauri::command]
pub fn drag_out_supported() -> bool {
    crate::dragout::supported()
}

#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> R<()> {
    app.opener().open_path(path, None::<&str>).map_err(err)
}

#[tauri::command]
pub fn language_list() -> Vec<crate::i18n::LanguageInfo> {
    crate::i18n::list()
}

#[tauri::command]
pub fn language_strings(code: String) -> std::collections::HashMap<String, String> {
    crate::i18n::strings(&code)
}

#[tauri::command]
pub fn language_reload() -> Vec<crate::i18n::LanguageInfo> {
    crate::i18n::reload();
    crate::i18n::list()
}

#[tauri::command]
pub fn language_folder(app: AppHandle) -> R<String> {
    let dir = crate::i18n::dir().ok_or("The language folder was not found")?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(err)?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> R<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("only web links can be opened".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(err)
}

#[tauri::command]
pub async fn torrent_details(mgr: Mgr<'_>, id: String) -> R<TorrentDetails> {
    mgr.torrent_details(&id).await.map_err(anyhow_err)
}

#[tauri::command]
pub async fn smartscreen_status() -> String {
    tauri::async_runtime::spawn_blocking(crate::security::smartscreen_status)
        .await
        .unwrap_or("unsupported")
        .to_string()
}

#[tauri::command]
pub async fn cloud_scan_with_password(mgr: Mgr<'_>, id: String, password: String) -> R<()> {
    mgr.inner()
        .clone()
        .scan_locked_in_cloud(&id, password)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn virustotal_open(app: tauri::AppHandle, mgr: Mgr<'_>, id: String) -> R<String> {
    use tauri_plugin_opener::OpenerExt;
    let path = mgr.file_path(&id).map_err(anyhow_err)?;
    if !path.is_file() {
        return Err("this download is a folder; VirusTotal checks one file at a time".into());
    }
    let sha = tauri::async_runtime::spawn_blocking(move || crate::virustotal::sha256_file(&path))
        .await
        .map_err(err)?
        .map_err(anyhow_err)?;
    app.opener()
        .open_url(
            format!("https://www.virustotal.com/gui/file/{sha}"),
            None::<&str>,
        )
        .map_err(err)?;
    Ok(sha)
}

#[tauri::command]
pub async fn virustotal_check(
    mgr: Mgr<'_>,
    id: String,
    upload: bool,
) -> R<crate::virustotal::VtResult> {
    let key = mgr.settings().virustotal_key;
    if key.is_empty() {
        return Err("add your VirusTotal API key in Settings → VirusTotal first".into());
    }
    let path = mgr.file_path(&id).map_err(anyhow_err)?;
    if !path.is_file() {
        return Err("this download is a folder; VirusTotal checks one file at a time".into());
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let hashed = path.clone();
    let sha = tauri::async_runtime::spawn_blocking(move || crate::virustotal::sha256_file(&hashed))
        .await
        .map_err(err)?
        .map_err(anyhow_err)?;
    let client = mgr.api_client.clone();
    let result = if upload {
        crate::virustotal::upload(&client, &key, &path, &sha, &name).await
    } else {
        crate::virustotal::lookup(&client, &key, &sha, &name).await
    };
    result.map_err(anyhow_err)
}

#[tauri::command]
pub async fn tor_detect() -> Option<String> {
    crate::tor::detect().await
}

#[tauri::command]
pub async fn tor_check(proxy: String) -> R<crate::tor::TorCheck> {
    crate::tor::check(&proxy).await.map_err(anyhow_err)
}

#[tauri::command]
pub async fn stop_all(mgr: Mgr<'_>) -> R<()> {
    mgr.inner().clone().stop_all().await;
    Ok(())
}

#[tauri::command]
pub async fn set_download_login(
    mgr: Mgr<'_>,
    id: String,
    user: String,
    password: String,
    remember: bool,
) -> R<()> {
    mgr.inner()
        .clone()
        .set_download_login(&id, &user, &password, remember)
        .await
        .map_err(err)
}

#[tauri::command]
pub fn background_store(mgr: Mgr<'_>, path: String) -> R<String> {
    crate::wallpaper::store(&mgr.paths.data, std::path::Path::new(&path))
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(err)
}

#[tauri::command]
pub fn background_data(mgr: Mgr<'_>, path: String) -> Option<String> {
    crate::wallpaper::data_url(&mgr.paths.data, std::path::Path::new(&path))
}

#[tauri::command]
pub fn set_download_meta(mgr: Mgr<'_>, id: String, tags: Vec<String>, note: String) -> R<()> {
    mgr.set_download_meta(&id, tags, note).map_err(anyhow_err)
}

#[tauri::command]
pub fn set_download_mirrors(mgr: Mgr<'_>, id: String, mirrors: Vec<String>) -> R<()> {
    mgr.set_download_mirrors(&id, mirrors).map_err(anyhow_err)
}

#[tauri::command]
pub fn download_history(mgr: Mgr<'_>) -> Vec<crate::extras::HistoryItem> {
    mgr.history.list()
}

#[tauri::command]
pub fn download_history_remove(mgr: Mgr<'_>, id: String) {
    mgr.history.remove(&id);
}

#[tauri::command]
pub fn download_history_clear(mgr: Mgr<'_>) {
    mgr.history.clear();
}

#[tauri::command]
pub fn backup_export(mgr: Mgr<'_>, path: String, secrets: bool) -> R<()> {
    mgr.export_backup(std::path::Path::new(&path), secrets)
        .map_err(err)
}

#[tauri::command]
pub fn auto_backups(mgr: Mgr<'_>) -> (String, Vec<crate::extras::BackupFile>) {
    let dir = mgr.auto_backup_dir();
    let list = crate::extras::auto_backups(&dir);
    (dir.to_string_lossy().into_owned(), list)
}

#[tauri::command]
pub fn auto_backup_now(mgr: Mgr<'_>) -> R<crate::extras::BackupFile> {
    mgr.auto_backup_now().map_err(err)
}

#[tauri::command]
pub fn backup_import(mgr: Mgr<'_>, path: String) -> R<(usize, Settings)> {
    mgr.inner()
        .clone()
        .import_backup(std::path::Path::new(&path))
        .map_err(err)
}

#[tauri::command]
pub fn cloud_history(mgr: Mgr<'_>) -> Vec<crate::protection::HistoryEntry> {
    mgr.protection.prune(mgr.settings().cloud_history_days);
    mgr.protection.history()
}

#[tauri::command]
pub fn cloud_history_clear(mgr: Mgr<'_>) {
    mgr.protection.clear_history();
}

#[tauri::command]
pub fn quarantine_list(mgr: Mgr<'_>) -> Vec<crate::protection::QuarantineItem> {
    mgr.protection.quarantine()
}

#[tauri::command]
pub fn quarantine_restore(mgr: Mgr<'_>, id: u64) -> R<()> {
    mgr.quarantine_restore(id).map_err(err)
}

#[tauri::command]
pub fn quarantine_delete(mgr: Mgr<'_>, id: u64) -> R<()> {
    mgr.quarantine_delete(id).map_err(err)
}

#[tauri::command]
pub fn captcha_list(mgr: Mgr<'_>) -> Vec<crate::captcha::Request> {
    mgr.captcha_list()
}

#[tauri::command]
pub fn captcha_open(mgr: Mgr<'_>, id: u64) -> R<()> {
    mgr.captcha_open(id).map_err(err)
}

#[tauri::command]
pub fn captcha_dismiss(mgr: Mgr<'_>, id: u64) {
    mgr.captcha_dismiss(id);
}

#[tauri::command]
pub fn extract_download(mgr: Mgr<'_>, id: String) -> R<()> {
    mgr.inner().clone().extract_download(&id).map_err(err)
}

#[tauri::command]
pub fn extract_tool_available() -> bool {
    crate::unpack::tool_available()
}

#[tauri::command]
pub fn ask_request(mgr: Mgr<'_>, window: tauri::WebviewWindow) -> Option<(AddRequest, Settings)> {
    let key = crate::askwin::key_of(window.label())?;
    crate::askwin::request(key).map(|r| (r, mgr.settings()))
}

#[tauri::command]
pub fn ask_progress(mgr: Mgr<'_>, window: tauri::WebviewWindow) -> Option<(String, Settings)> {
    let key = crate::askwin::key_of(window.label())?;
    crate::askwin::progress(key).map(|id| (id, mgr.settings()))
}

#[tauri::command]
pub fn ask_confirm(window: tauri::WebviewWindow) -> Option<String> {
    crate::askwin::confirm(crate::askwin::key_of(window.label())?)
}

#[tauri::command]
pub fn open_confirm_window(app: AppHandle, mgr: Mgr<'_>, id: String) -> R<()> {
    mgr.record(&id).map_err(anyhow_err)?;
    crate::askwin::open_confirm(&app, &id);
    Ok(())
}

#[tauri::command]
pub fn ask_started(window: tauri::WebviewWindow) {
    if let Some(key) = crate::askwin::key_of(window.label()) {
        crate::askwin::forget(key);
    }
    let _ = window.set_always_on_top(false);
}

#[tauri::command]
pub fn notice_list() -> Vec<crate::notice::Notice> {
    crate::notice::list()
}

#[tauri::command]
pub fn notice_dismiss(app: AppHandle, id: u64) {
    crate::notice::dismiss(&app, id);
}

#[tauri::command]
pub fn notice_layout(app: AppHandle, height: f64) -> R<()> {
    crate::notice::layout(&app, height).map_err(err)
}

#[tauri::command]
pub fn notice_show_download(app: AppHandle, id: Option<String>, start: bool, report: Option<bool>) {
    crate::show_main_window(&app);
    let _ = app.emit_to(
        "main",
        "app://select",
        serde_json::json!({ "id": id, "start": start, "report": report.unwrap_or(false) }),
    );
}

#[tauri::command]
pub fn find_duplicate(mgr: Mgr<'_>, request: AddRequest) -> Option<crate::model::Duplicate> {
    mgr.find_duplicate(&request)
}

#[tauri::command]
pub fn cloud_scan_all(mgr: Mgr<'_>) -> R<usize> {
    mgr.scan_all_in_cloud().map_err(anyhow_err)
}

#[tauri::command]
pub fn cloud_scan_report(
    mgr: Mgr<'_>,
    id: String,
) -> Option<crate::manager::automation::CloudReport> {
    mgr.cloud_report(&id)
}

#[tauri::command]
pub fn cloud_scan_stop(mgr: Mgr<'_>, id: String) {
    mgr.stop_cloud_scan(&id);
}

#[tauri::command]
pub fn reveal_downloaded_file(app: AppHandle, mgr: Mgr<'_>, id: String, path: String) -> R<()> {
    let target = mgr.reveal_path_of(&id, &path).map_err(anyhow_err)?;
    app.opener().reveal_item_in_dir(target).map_err(err)
}

#[tauri::command]
pub fn cloud_scan_download(mgr: Mgr<'_>, id: String) -> R<()> {
    mgr.scan_in_cloud(&id).map_err(anyhow_err)
}

#[tauri::command]
pub async fn torrent_recheck(mgr: Mgr<'_>, id: String) -> R<()> {
    mgr.torrent_recheck(&id).await.map_err(anyhow_err)
}

#[tauri::command]
pub fn torrent_export(mgr: Mgr<'_>, id: String, dest: String) -> R<()> {
    let file = mgr.torrent_file_path(&id).map_err(anyhow_err)?;
    std::fs::copy(file, dest).map(|_| ()).map_err(err)
}

#[tauri::command]
pub async fn create_torrent(
    mgr: Mgr<'_>,
    source: String,
    trackers: Vec<String>,
    piece_kib: Option<u32>,
    save_to: String,
    seed: bool,
) -> R<String> {
    let trackers = trackers
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    mgr.create_torrent(
        std::path::PathBuf::from(source),
        trackers,
        piece_kib,
        std::path::PathBuf::from(save_to),
        seed,
    )
    .await
    .map_err(err)
}

#[tauri::command]
pub fn torrent_stream_url(mgr: Mgr<'_>, id: String, file: usize) -> R<String> {
    let bridge = mgr.bridge.lock().clone();
    if !bridge.running {
        return Err("turn on the browser bridge (Settings → Browser Integration) to play files while they download".into());
    }
    let hash = mgr.torrent_hash(&id).map_err(anyhow_err)?;
    Ok(format!(
        "http://127.0.0.1:{}/stream/{}/{hash}/{file}",
        bridge.port,
        mgr.stream_token()
    ))
}

#[tauri::command]
pub fn rss_feeds(mgr: Mgr<'_>) -> Vec<crate::manager::FeedView> {
    mgr.rss_views()
}

#[tauri::command]
pub async fn rss_refresh(mgr: Mgr<'_>) -> R<usize> {
    mgr.inner().clone().refresh_rss().await.map_err(anyhow_err)
}

#[tauri::command]
pub fn extract_with_password(mgr: Mgr<'_>, id: String, password: Option<String>) -> R<()> {
    mgr.inner()
        .clone()
        .extract_with_password(&id, password)
        .map_err(anyhow_err)
}

#[tauri::command]
pub async fn site_rule_test(
    mgr: Mgr<'_>,
    rule: crate::settings::SiteRule,
    url: String,
) -> R<crate::crawl::PageScan> {
    mgr.test_site_rule(rule, &url).await.map_err(anyhow_err)
}

#[tauri::command]
pub async fn rss_resolve(mgr: Mgr<'_>, url: String) -> R<crate::feeds::ResolvedFeed> {
    crate::feeds::resolve(&mgr.api_client, &url)
        .await
        .map_err(anyhow_err)
}

#[tauri::command]
pub async fn discover_feeds(mgr: Mgr<'_>, url: String) -> R<Vec<crate::feeds::FoundFeed>> {
    crate::feeds::discover(&mgr.api_client, &url)
        .await
        .map_err(anyhow_err)
}

#[tauri::command]
pub async fn podcast_search(mgr: Mgr<'_>, term: String) -> R<Vec<crate::feeds::Podcast>> {
    crate::feeds::search_podcasts(&mgr.api_client, &term)
        .await
        .map_err(anyhow_err)
}

#[tauri::command]
pub async fn rss_download(mgr: Mgr<'_>, feed: String, item: String) -> R<String> {
    mgr.rss_download(&feed, &item).await.map_err(anyhow_err)
}

#[tauri::command]
pub fn rss_mark(mgr: Mgr<'_>, feed: String, item: String) -> R<()> {
    mgr.rss_mark(&feed, &item).map_err(anyhow_err)
}

#[tauri::command]
pub fn search_sites(mgr: Mgr<'_>) -> Vec<crate::search::SiteView> {
    crate::search::sites(&mgr.settings())
}

#[tauri::command]
pub async fn torrent_search(
    mgr: Mgr<'_>,
    query: String,
    sites: Option<Vec<String>>,
) -> R<crate::search::SearchOutcome> {
    let s = mgr.settings();
    Ok(crate::search::search(&mgr.api_client, &s, &query, &sites.unwrap_or_default()).await)
}

#[tauri::command]
pub async fn torrent_set_files(mgr: Mgr<'_>, id: String, files: Vec<usize>) -> R<()> {
    mgr.torrent_set_files(&id, files).await.map_err(anyhow_err)
}

#[tauri::command]
pub async fn torrent_session_status(mgr: Mgr<'_>) -> R<Value> {
    let running = mgr.global_stats().torrent;
    Ok(json!({
        "status": running.unwrap_or_default(),
        "error": mgr.torrent_error(),
    }))
}

#[tauri::command]
pub async fn start_torrent_engine(mgr: Mgr<'_>) -> R<()> {
    mgr.torrent_engine().await.map(|_| ()).map_err(anyhow_err)
}

#[tauri::command]
pub async fn public_trackers(mgr: Mgr<'_>) -> R<Vec<String>> {
    crate::trackers::fetch_public_trackers(&mgr.api_client)
        .await
        .map_err(err)
}

#[tauri::command]
pub fn get_settings(mgr: Mgr<'_>) -> Settings {
    mgr.settings()
}

#[tauri::command]
pub async fn save_settings(mgr: Mgr<'_>, settings: Settings) -> R<Settings> {
    mgr.save_settings(settings).await.map_err(anyhow_err)
}

#[tauri::command]
pub async fn reset_settings(mgr: Mgr<'_>) -> R<Settings> {
    let current = mgr.settings();
    let fresh = Settings {
        trusted_extensions: current.trusted_extensions.clone(),
        bridge_port: current.bridge_port,
        ..Settings::default()
    };
    mgr.save_settings(fresh).await.map_err(anyhow_err)
}

#[tauri::command]
pub fn apply_category_language(mgr: Mgr<'_>, rename: bool) -> R<usize> {
    mgr.apply_category_language(rename).map_err(anyhow_err)
}

#[tauri::command]
pub async fn tools_status(mgr: Mgr<'_>) -> R<ToolStatus> {
    Ok(mgr.tools.status().await)
}

#[tauri::command]
pub async fn update_ytdlp(app: AppHandle, mgr: Mgr<'_>) -> R<String> {
    mgr.tools
        .install_ytdlp(move |p| {
            let _ = app.emit(EVT_TOOLS, p);
        })
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn install_ffmpeg(app: AppHandle, mgr: Mgr<'_>) -> R<()> {
    mgr.tools
        .install_ffmpeg(
            move |p| {
                let _ = app.emit(EVT_TOOLS, p);
            },
            &tokio_util::sync::CancellationToken::new(),
        )
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn install_deno(app: AppHandle, mgr: Mgr<'_>) -> R<String> {
    mgr.tools
        .install_deno(
            move |p| {
                let _ = app.emit(EVT_TOOLS, p);
            },
            &tokio_util::sync::CancellationToken::new(),
        )
        .await
        .map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReport {
    app: AppUpdate,
    ytdlp_installed: Option<String>,
    ytdlp_latest: Option<String>,
    ytdlp_error: Option<String>,
}

#[tauri::command]
pub async fn check_updates(mgr: Mgr<'_>) -> R<UpdateReport> {
    let (app, installed, latest) = tokio::join!(
        updates::check_app(&mgr.api_client),
        mgr.tools.ytdlp_version(),
        mgr.tools.latest_ytdlp_version()
    );
    let (ytdlp_latest, ytdlp_error) = match latest {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(format!("{e:#}"))),
    };
    Ok(UpdateReport {
        app,
        ytdlp_installed: installed,
        ytdlp_latest,
        ytdlp_error,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskSpace {
    free: u64,
    total: u64,
}

#[tauri::command]
pub fn disk_space(mgr: Mgr<'_>, path: Option<String>) -> R<DiskSpace> {
    let mut p = std::path::PathBuf::from(path.unwrap_or_else(|| mgr.settings().download_dir));
    while !p.exists() {
        if !p.pop() {
            break;
        }
    }
    Ok(DiskSpace {
        free: fs4::available_space(&p).map_err(err)?,
        total: fs4::total_space(&p).map_err(err)?,
    })
}

#[tauri::command]
pub async fn reload_ip_filter(mgr: Mgr<'_>) -> R<FilterStatus> {
    Ok(mgr.reload_ip_filter(true).await)
}

#[tauri::command]
pub fn ip_filter_status(mgr: Mgr<'_>) -> FilterStatus {
    mgr.filter_status.lock().clone()
}

#[tauri::command]
pub fn bridge_status(mgr: Mgr<'_>) -> BridgeStatus {
    mgr.bridge.lock().clone()
}

#[tauri::command]
pub fn take_pending(mgr: Mgr<'_>) -> Vec<Pending> {
    mgr.take_pending()
}

#[tauri::command]
pub async fn approve_extension(mgr: Mgr<'_>, origin: String, allow: bool) -> R<Settings> {
    let mut s = mgr.settings();
    s.trusted_extensions.retain(|o| o != &origin);
    if allow {
        s.trusted_extensions.push(origin);
    }
    mgr.save_settings(s).await.map_err(anyhow_err)
}

#[tauri::command]
pub async fn remove_extension(mgr: Mgr<'_>, origin: String) -> R<Settings> {
    mgr.uninstall_requests.lock().insert(origin.clone());
    let mut s = mgr.settings();
    s.trusted_extensions.retain(|o| o != &origin);
    mgr.save_settings(s).await.map_err(anyhow_err)
}

#[tauri::command]
pub async fn remove_extension_everywhere(mgr: Mgr<'_>) -> R<Settings> {
    let mut s = mgr.settings();
    {
        let mut pending = mgr.uninstall_requests.lock();
        for o in &s.trusted_extensions {
            pending.insert(o.clone());
        }
        pending.insert(format!(
            "chrome-extension://{}",
            crate::nativehost::CHROMIUM_ID
        ));
    }
    s.trusted_extensions.clear();
    s.extension_setup_done = false;
    let saved = mgr.save_settings(s).await.map_err(anyhow_err)?;
    let data = mgr.paths.data.clone();
    let _ = tokio::task::spawn_blocking(move || crate::nativehost::unregister(&data)).await;
    Ok(saved)
}

#[tauri::command]
pub fn clear_cookies(mgr: Mgr<'_>) -> R<usize> {
    let mut removed = 0;
    if let Ok(dir) = std::fs::read_dir(&mgr.paths.cookies) {
        for entry in dir.flatten() {
            if std::fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

#[tauri::command]
pub async fn find_sha256(mgr: Mgr<'_>, url: String, filename: Option<String>) -> R<Option<Value>> {
    Ok(
        crate::manager::find_sha256(&mgr.api_client, &url, filename.as_deref())
            .await
            .map(|(hash, source)| json!({ "hash": hash, "source": source })),
    )
}

#[tauri::command]
pub fn extension_dir(app: AppHandle, firefox: Option<bool>) -> R<String> {
    crate::browsers::extension_folder(&app, firefox.unwrap_or(false))
        .map(|d| d.to_string_lossy().into_owned())
        .map_err(err)
}

#[tauri::command]
pub fn appearance_support() -> crate::appearance::Support {
    crate::appearance::support()
}

#[tauri::command]
pub fn apply_appearance(
    window: tauri::WebviewWindow,
    theme: String,
    transparency: bool,
    material: String,
    tint: Option<[u8; 4]>,
) -> bool {
    crate::appearance::apply(&window, &theme, transparency, &material, tint)
}

#[tauri::command]
pub async fn browser_list() -> Vec<crate::browsers::Browser> {
    tauri::async_runtime::spawn_blocking(crate::browsers::detect)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn setup_extension(
    app: AppHandle,
    mgr: Mgr<'_>,
    browser: String,
    store: Option<bool>,
) -> R<String> {
    mgr.pair_asked.lock().clear();
    tauri::async_runtime::spawn_blocking(move || {
        let b = crate::browsers::detect()
            .into_iter()
            .find(|b| b.id == browser)
            .ok_or_else(|| anyhow::anyhow!("{browser} is not installed"))?;
        let folder = crate::browsers::extension_folder_for(&app, &b)?;
        if store.unwrap_or(false) {
            crate::browsers::open_store(&b)?;
        } else {
            crate::browsers::open_page(&b)?;
        }
        Ok::<_, anyhow::Error>(folder.to_string_lossy().into_owned())
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
pub async fn install_firefox_xpi(app: AppHandle, mgr: Mgr<'_>, browser: String) -> R<String> {
    mgr.pair_asked.lock().clear();
    let b = tauri::async_runtime::spawn_blocking(move || {
        crate::browsers::detect()
            .into_iter()
            .find(|b| b.id == browser)
            .ok_or_else(|| anyhow::anyhow!("{browser} is not installed"))
    })
    .await
    .map_err(err)?
    .map_err(err)?;
    let dir = crate::browsers::xpi_dir(&app, &b).map_err(err)?;
    let file = crate::browsers::download_signed_xpi(&mgr.api_client, &dir)
        .await
        .map_err(err)?;
    crate::browsers::open_xpi(&b, &file).map_err(err)?;
    Ok(file.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn move_in_queue(mgr: Mgr<'_>, id: String, to: String) -> R<()> {
    mgr.move_in_queue(&id, &to).map_err(anyhow_err)
}

#[tauri::command]
pub async fn download_app_update(
    app: AppHandle,
    mgr: Mgr<'_>,
) -> R<Option<crate::selfupdate::ReadyUpdate>> {
    crate::selfupdate::fetch_latest(&app, &mgr.api_client)
        .await
        .map_err(err)
}

#[tauri::command]
pub fn mandatory_update_status() -> Option<crate::selfupdate::MandatoryStatus> {
    crate::selfupdate::mandatory_status()
}

#[tauri::command]
pub async fn retry_mandatory_update(app: AppHandle, mgr: Mgr<'_>) -> R<()> {
    crate::selfupdate::mandatory_update(&app, &mgr.api_client).await;
    Ok(())
}

#[tauri::command]
pub fn app_update_ready() -> Option<crate::selfupdate::ReadyUpdate> {
    crate::selfupdate::ready()
}

#[tauri::command]
pub async fn install_app_update(app: AppHandle, mgr: Mgr<'_>) -> R<()> {
    mgr.shutdown().await;
    if !crate::selfupdate::install_ready(true, true).map_err(anyhow_err)? {
        return Err("no update has been downloaded".into());
    }
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
