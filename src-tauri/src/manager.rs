use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use parking_lot::{Mutex, RwLock};
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, REFERER, USER_AGENT};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager as _};
use tokio_util::sync::CancellationToken;

use crate::engine::{self, limiter::Limiter, Progress};
use crate::ipfilter::{self, FilterStatus};
use crate::media::{self, MediaEvent, MediaJob, ToolProgress, Tools};
use crate::model::*;
use crate::resolver;
use crate::settings::{Settings, MAX_THREADS};
use crate::torrent::{EngineConfig, TorrentEngine, TorrentSource};
use crate::util::{self, now_ms};

pub mod automation;
pub use automation::FeedView;

pub const EVT_DOWNLOADS: &str = "downloads://changed";
pub const EVT_STATS: &str = "stats://global";
pub const EVT_PENDING: &str = "app://pending";
pub const EVT_TOOLS: &str = "tools://progress";
pub const EVT_TOAST: &str = "app://toast";
pub const EVT_CLOUD: &str = "cloud://report";

pub struct Paths {
    pub data: PathBuf,
    pub tools: PathBuf,
    pub db: PathBuf,
    pub settings: PathBuf,
    pub torrents: PathBuf,
    pub torrent_state: PathBuf,
    pub cookies: PathBuf,
    pub ip_filter: PathBuf,
}

struct Running {
    cancel: CancellationToken,
    progress: Option<Arc<Progress>>,
    limiter: Arc<Limiter>,
    last_received: u64,
    last_tick: Instant,
    ema: f64,
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
    media_attempt: Option<(CancellationToken, u32)>,
    progress_base: u64,
    progress_total: Option<u64>,
    peak: u64,
}

async fn remove_scratch(files: &[PathBuf]) {
    for f in files {
        let _ = tokio::fs::remove_file(f).await;
    }
}

struct Entry {
    rec: DownloadRecord,
    live: Live,
    run: Option<Running>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Pending {
    Add {
        request: Box<AddRequest>,
    },
    Duplicate {
        request: Box<AddRequest>,
        existing: crate::model::Duplicate,
    },
    Batch {
        batch: Box<crate::model::PageBatch>,
    },
    Pair {
        origin: String,
    },
    OpenTorrent {
        path: String,
    },
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStatus {
    pub running: bool,
    pub port: u16,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum OverallProgress {
    #[default]
    Idle,
    Unknown,
    Fraction(f64),
}

impl OverallProgress {
    pub fn percent(self) -> Option<u64> {
        match self {
            OverallProgress::Fraction(f) => Some((f * 100.0).floor().clamp(0.0, 100.0) as u64),
            _ => None,
        }
    }
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStats {
    pub download_speed: u64,
    pub upload_speed: u64,
    pub active: usize,
    pub queued: usize,
    #[serde(skip)]
    pub progress: OverallProgress,
    pub torrent: Option<crate::torrent::SessionStatus>,
    pub alt_speed: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProbeInfo {
    pub kind: String,
    pub url: String,
    pub filename: Option<String>,
    pub total: Option<u64>,
    pub resumable: bool,
    pub service: Option<String>,
    pub content_type: Option<String>,
    pub sha256: Option<String>,
    pub sha256_source: Option<String>,
    pub error: Option<String>,
}

pub struct Manager {
    pub app: AppHandle,
    pub paths: Paths,
    settings: RwLock<Settings>,
    entries: Mutex<Vec<Entry>>,
    http: RwLock<reqwest::Client>,
    pub api_client: reqwest::Client,
    pub global_limit: Arc<Limiter>,
    pub tools: Tools,
    torrent: tokio::sync::Mutex<Option<Arc<TorrentEngine>>>,
    torrent_ready: RwLock<Option<Arc<TorrentEngine>>>,
    torrent_error: Mutex<Option<String>>,
    pending: Mutex<Vec<Pending>>,
    pub uninstall_requests: Mutex<std::collections::HashSet<String>>,
    pub pair_asked: Mutex<std::collections::HashSet<String>>,
    pub cloud_reports: Mutex<std::collections::HashMap<String, automation::CloudReport>>,
    pub cloud_stops: Mutex<std::collections::HashMap<String, Arc<AtomicBool>>>,
    pub filter_status: Mutex<FilterStatus>,
    pub bridge: Mutex<BridgeStatus>,
    pub bridge_cancel: Mutex<CancellationToken>,
    dirty: AtomicBool,
    save_needed: AtomicBool,
    last_global: Mutex<GlobalStats>,
    auto: Mutex<automation::AutoState>,
}

fn redirect_policy(block_insecure: bool) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 15 {
            return attempt.error("too many redirects");
        }
        let downgrade = attempt.url().scheme() == "http"
            && attempt
                .previous()
                .last()
                .is_some_and(|p| p.scheme() == "https");
        if block_insecure && downgrade {
            let host = attempt.url().host_str().unwrap_or("?").to_string();
            return attempt.error(format!(
                "refused a redirect from HTTPS to unencrypted HTTP ({host})"
            ));
        }
        attempt.follow()
    })
}

fn build_engine_client(accept_invalid: bool, block_insecure: bool) -> reqwest::Client {
    reqwest::Client::builder()
        .http1_only()
        .user_agent(crate::settings::DEFAULT_USER_AGENT)
        .tcp_nodelay(true)
        .tcp_keepalive(Duration::from_secs(30))
        .pool_max_idle_per_host(64)
        .pool_idle_timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(30))
        .redirect(redirect_policy(block_insecure))
        .min_tls_version(reqwest::tls::Version::TLS_1_2)
        .danger_accept_invalid_certs(accept_invalid)
        .build()
        .expect("http client")
}

fn build_api_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!(
            "Multron-Download-Manager/",
            env!("CARGO_PKG_VERSION")
        ))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .redirect(redirect_policy(true))
        .min_tls_version(reqwest::tls::Version::TLS_1_2)
        .build()
        .expect("api client")
}

fn category_dir(c: Category) -> &'static str {
    match c {
        Category::Video => "Video",
        Category::Music => "Music",
        Category::Documents => "Documents",
        Category::Archives => "Archives",
        Category::Programs => "Programs",
        Category::Images => "Images",
        Category::Other => "Other",
    }
}

fn normalize_sha(s: Option<&str>) -> Result<Option<String>> {
    let Some(s) = s.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let token = s
        .split_whitespace()
        .find(|t| util::is_sha256_hex(t))
        .ok_or_else(|| anyhow!("SHA-256 must be 64 hexadecimal characters"))?;
    Ok(Some(token.to_ascii_lowercase()))
}

pub fn sha256_file(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

impl Manager {
    pub fn new(app: AppHandle) -> Result<Arc<Self>> {
        let portable = crate::portable::data_dir();
        let data = match &portable {
            Some(dir) => dir.clone(),
            None => app.path().app_data_dir().context("no app data dir")?,
        };
        let config = match portable {
            Some(dir) => dir,
            None => app.path().app_config_dir().unwrap_or_else(|_| data.clone()),
        };
        std::fs::create_dir_all(&data)?;
        restrict_to_user(&data);
        restrict_to_user(&config);
        let paths = Paths {
            tools: data.join("tools"),
            db: data.join("downloads.json"),
            settings: config.join("settings.json"),
            torrents: data.join("torrents"),
            torrent_state: data.join("torrent-session"),
            cookies: data.join("cookies"),
            ip_filter: data.join("ipfilter.p2p"),
            data,
        };
        let mut settings = Settings::load(&paths.settings);
        if settings.download_dir.trim().is_empty() {
            settings.download_dir = app
                .path()
                .download_dir()
                .or_else(|_| app.path().home_dir().map(|h| h.join("Downloads")))
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        let _ = settings.save(&paths.settings);
        {
            let data = paths.data.clone();
            let trusted = settings.trusted_extensions.clone();
            std::thread::spawn(move || crate::nativehost::sync(&data, &trusted));
        }

        let mut entries: Vec<Entry> = std::fs::read(&paths.db)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<DownloadRecord>>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|mut rec| {
                if rec.status == Status::Active && rec.kind != Kind::Torrent {
                    rec.status = if settings.resume_on_startup {
                        Status::Queued
                    } else {
                        Status::Paused
                    };
                }
                if rec.hash_state == HashState::Verifying {
                    rec.hash_state = HashState::Pending;
                }
                if rec.cloud_state == "scanning" {
                    crate::manager::automation::end_cloud(&mut rec, None);
                }
                if rec.queue_order == 0 {
                    rec.queue_order = rec.created_at;
                }
                Entry {
                    rec,
                    live: Live::default(),
                    run: None,
                }
            })
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.rec.created_at));

        let api_client = build_api_client();
        let tools = Tools::new(paths.tools.clone(), api_client.clone());
        Ok(Arc::new(Self {
            app,
            global_limit: Arc::new(Limiter::new(settings.speed_limit * 1024)),
            http: RwLock::new(build_engine_client(
                settings.tls_accept_invalid_certs,
                settings.block_insecure_redirects,
            )),
            api_client,
            tools,
            paths,
            settings: RwLock::new(settings),
            entries: Mutex::new(entries),
            torrent: tokio::sync::Mutex::new(None),
            torrent_ready: RwLock::new(None),
            torrent_error: Mutex::new(None),
            pending: Mutex::new(Vec::new()),
            uninstall_requests: Mutex::new(std::collections::HashSet::new()),
            pair_asked: Mutex::new(std::collections::HashSet::new()),
            cloud_reports: Mutex::new(std::collections::HashMap::new()),
            cloud_stops: Mutex::new(std::collections::HashMap::new()),
            filter_status: Mutex::new(FilterStatus::default()),
            bridge: Mutex::new(BridgeStatus::default()),
            bridge_cancel: Mutex::new(CancellationToken::new()),
            dirty: AtomicBool::new(true),
            save_needed: AtomicBool::new(false),
            last_global: Mutex::new(GlobalStats::default()),
            auto: Mutex::new(automation::AutoState::default()),
        }))
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().clone()
    }

    pub async fn save_settings(self: &Arc<Self>, new: Settings) -> Result<Settings> {
        let new = new.normalized();
        let old = self.settings();
        new.save(&self.paths.settings)?;
        *self.settings.write() = new.clone();
        self.reset_applied_limits();

        if old.speed_limit != new.speed_limit {
            self.global_limit.set_rate(new.speed_limit * 1024);
        }
        if old.tls_accept_invalid_certs != new.tls_accept_invalid_certs
            || old.block_insecure_redirects != new.block_insecure_redirects
        {
            *self.http.write() =
                build_engine_client(new.tls_accept_invalid_certs, new.block_insecure_redirects);
        }
        if old.launch_at_startup != new.launch_at_startup {
            use tauri_plugin_autostart::ManagerExt;
            let auto = self.app.autolaunch();
            let _ = if new.launch_at_startup {
                auto.enable()
            } else {
                auto.disable()
            };
        }
        if old.trusted_extensions != new.trusted_extensions {
            let data = self.paths.data.clone();
            let trusted = new.trusted_extensions.clone();
            std::thread::spawn(move || crate::nativehost::sync(&data, &trusted));
        }
        if old.language != new.language {
            crate::tray::refresh(&self.app, &new.language);
        }
        if old.bridge_enabled != new.bridge_enabled || old.bridge_port != new.bridge_port {
            crate::bridge::restart(self.clone());
        }
        let torrent_changed = old.torrent_port != new.torrent_port
            || old.torrent_dht != new.torrent_dht
            || old.torrent_upnp != new.torrent_upnp
            || old.torrent_utp != new.torrent_utp
            || old.torrent_lsd != new.torrent_lsd
            || old.torrent_ipv4_only != new.torrent_ipv4_only
            || old.torrent_peer_limit != new.torrent_peer_limit
            || old.torrent_proxy != new.torrent_proxy
            || old.torrent_bind_interface != new.torrent_bind_interface
            || old.torrent_disable_trackers != new.torrent_disable_trackers
            || old.torrent_hide_client != new.torrent_hide_client
            || old.extra_trackers != new.extra_trackers
            || old.ip_filter_enabled != new.ip_filter_enabled
            || old.ip_filter_allow_mode != new.ip_filter_allow_mode
            || old.torrent_scrape != new.torrent_scrape
            || old.torrent_public_trackers != new.torrent_public_trackers;
        let filter_changed = old.ip_filter_source != new.ip_filter_source
            || old.ip_filter_source_v6 != new.ip_filter_source_v6
            || old.ip_filter_manual != new.ip_filter_manual;
        if filter_changed && new.ip_filter_enabled {
            let _ = self.reload_ip_filter(false).await;
        }
        if torrent_changed || (filter_changed && new.ip_filter_enabled) {
            self.restart_torrent_engine().await;
        } else if old.torrent_download_limit != new.torrent_download_limit
            || old.torrent_upload_limit != new.torrent_upload_limit
        {
            if let Some(engine) = self.torrent_if_running() {
                engine.apply_limits(new.torrent_download_limit, new.torrent_upload_limit);
            }
        }
        if !new.torrent_enabled && old.torrent_enabled {
            self.stop_torrent_engine().await;
        }
        self.schedule();
        Ok(new)
    }

    pub fn views(&self) -> Vec<DownloadView> {
        let entries = self.entries.lock();
        let mut waiting: Vec<&Entry> = entries.iter().filter(|e| is_waiting(e)).collect();
        waiting.sort_by_key(|e| e.rec.queue_order);
        let pos = |id: &str| {
            waiting
                .iter()
                .position(|w| w.rec.id == id)
                .map(|i| i as u32 + 1)
        };
        entries
            .iter()
            .map(|e| DownloadView {
                record: e.rec.clone(),
                live: e.live.clone(),
                queue_pos: pos(&e.rec.id),
            })
            .collect()
    }

    pub fn move_in_queue(self: &Arc<Self>, id: &str, to: &str) -> Result<()> {
        {
            let mut entries = self.entries.lock();
            let mut order: Vec<(String, i64)> = entries
                .iter()
                .filter(|e| is_waiting(e))
                .map(|e| (e.rec.id.clone(), e.rec.queue_order))
                .collect();
            order.sort_by_key(|(_, o)| *o);
            let from = order
                .iter()
                .position(|(i, _)| i == id)
                .ok_or_else(|| anyhow!("only queued downloads can be reordered"))?;
            let item = order.remove(from);
            let dest = match to {
                "up" => from.saturating_sub(1),
                "down" => (from + 1).min(order.len()),
                "top" => 0,
                "bottom" => order.len(),
                other => return Err(anyhow!("unknown queue move: {other}")),
            };
            order.insert(dest, item);
            let base = order.iter().map(|(_, o)| *o).min().unwrap_or_else(now_ms);
            for (i, (oid, _)) in order.iter().enumerate() {
                if let Some(e) = entries.iter_mut().find(|e| &e.rec.id == oid) {
                    e.rec.queue_order = base + i as i64;
                }
            }
        }
        self.mark_dirty();
        Ok(())
    }

    pub fn global_stats(&self) -> GlobalStats {
        self.last_global.lock().clone()
    }

    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
        self.save_needed.store(true, Ordering::Relaxed);
    }

    fn emit_now(&self) {
        let _ = self.app.emit(EVT_DOWNLOADS, self.views());
    }

    pub fn toast(&self, kind: &str, message: impl Into<String>) {
        let _ = self.app.emit(
            EVT_TOAST,
            serde_json::json!({ "kind": kind, "message": message.into() }),
        );
    }

    pub(crate) fn word(&self, key: &str) -> String {
        crate::i18n::t(&self.settings().language, key)
    }

    fn in_background(&self) -> bool {
        self.app.get_webview_window("main").is_none_or(|w| {
            !w.is_visible().unwrap_or(false)
                || w.is_minimized().unwrap_or(false)
                || !w.is_focused().unwrap_or(false)
        })
    }

    fn notify(&self, download: Option<&str>, title_key: &str, body: &str) {
        let s = self.settings();
        let finished = matches!(title_key, "done" | "failed");
        if (title_key == "started" || finished)
            && !s.download_notices
            && !(finished && self.in_background())
        {
            return;
        }
        if title_key == "scan_cloud" && !s.scan_notices {
            return;
        }
        if matches!(title_key, "done" | "failed" | "threat" | "hash_bad") {
            if let Some(w) = self.app.get_webview_window("main") {
                if !w.is_focused().unwrap_or(false) {
                    let kind = if title_key == "done" {
                        tauri::UserAttentionType::Informational
                    } else {
                        tauri::UserAttentionType::Critical
                    };
                    let _ = w.request_user_attention(Some(kind));
                }
            }
        }
        if !s.notifications {
            return;
        }
        let card_in_window = matches!(title_key, "started" | "done" | "failed" | "scan_cloud");
        if card_in_window && s.in_app_notices && !self.in_background() {
            return;
        }
        let ok = body.contains(&self.word("no_threat"));
        crate::notice::push(
            &self.app,
            title_key,
            crate::i18n::t(&s.language, title_key),
            body,
            download,
            ok,
        );
    }

    pub fn persist(&self) {
        let records: Vec<DownloadRecord> =
            self.entries.lock().iter().map(|e| e.rec.clone()).collect();
        let tmp = self.paths.db.with_extension("json.tmp");
        let res = serde_json::to_vec(&records)
            .map_err(anyhow::Error::from)
            .and_then(|b| std::fs::write(&tmp, b).map_err(Into::into))
            .and_then(|_| std::fs::rename(&tmp, &self.paths.db).map_err(Into::into));
        if let Err(e) = res {
            tracing::error!("could not save downloads: {e:#}");
        }
    }

    fn with_entry<R>(&self, id: &str, f: impl FnOnce(&mut Entry) -> R) -> Option<R> {
        let mut entries = self.entries.lock();
        entries.iter_mut().find(|e| e.rec.id == id).map(f)
    }

    fn record(&self, id: &str) -> Result<DownloadRecord> {
        self.with_entry(id, |e| e.rec.clone())
            .ok_or_else(|| anyhow!("download not found"))
    }

    pub fn push_pending(&self, p: Pending) {
        {
            let mut pending = self.pending.lock();
            if let Pending::Pair { origin } = &p {
                if pending
                    .iter()
                    .any(|x| matches!(x, Pending::Pair { origin: o } if o == origin))
                {
                    return;
                }
            }
            pending.push(p);
        }
        crate::show_main_window(&self.app);
        let _ = self.app.emit(EVT_PENDING, ());
    }

    pub fn take_pending(&self) -> Vec<Pending> {
        std::mem::take(&mut *self.pending.lock())
    }

    fn request_headers(&self, rec: &DownloadRecord) -> HeaderMap {
        let ua = rec
            .user_agent
            .clone()
            .unwrap_or_else(|| self.settings.read().user_agent.clone());
        let mut h = HeaderMap::new();
        if let Ok(v) = HeaderValue::from_str(&ua) {
            h.insert(USER_AGENT, v);
        }
        if let Some(r) = rec
            .referrer
            .as_deref()
            .and_then(|r| HeaderValue::from_str(r).ok())
        {
            h.insert(REFERER, r);
        }
        if let Some(c) = rec
            .cookie_header
            .as_deref()
            .and_then(|c| HeaderValue::from_str(c).ok())
        {
            h.insert(COOKIE, c);
        }
        h
    }

    pub async fn probe_url(
        &self,
        url: &str,
        referrer: Option<String>,
        cookies: &[BrowserCookie],
    ) -> ProbeInfo {
        let url = util::reddit_video_url(url.trim());
        let kind = detect_kind(&url);
        let mut info = ProbeInfo {
            kind: format!("{kind:?}").to_lowercase(),
            url: url.clone(),
            service: resolver::service_name(&url).map(str::to_string),
            ..Default::default()
        };
        if kind != Kind::Http {
            return info;
        }
        let rec = DownloadRecord {
            referrer,
            cookie_header: cookie_header(cookies, &url),
            ..blank_record(&url)
        };
        let mut headers = self.request_headers(&rec);
        let client = self.http.read().clone();
        let result: Result<()> = async {
            let resolved = resolver::resolve(&client, &url).await?;
            let probe = engine::probe_with_fallback(&client, &resolved.url, &mut headers).await?;
            info.filename = resolved.filename.or(probe.filename);
            info.total = resolved.size.or(probe.total);
            info.resumable = probe.resumable;
            info.content_type = probe.content_type.clone();
            if resolved.service.is_none()
                && probe
                    .content_type
                    .as_deref()
                    .map(|c| c.starts_with("text/html"))
                    .unwrap_or(false)
            {
                info.kind = "media".into();
            }
            Ok(())
        }
        .await;
        if let Err(e) = result {
            info.error = Some(format!("{e:#}"));
            info.filename = util::filename_from_url(&url);
        }
        if info.kind == "http" && self.settings.read().auto_detect_sha256 {
            if let Some((hash, source)) =
                find_sha256(&self.api_client, &url, info.filename.as_deref()).await
            {
                info.sha256 = Some(hash);
                info.sha256_source = Some(source);
            }
        }
        info
    }

    pub fn find_duplicate(&self, req: &AddRequest) -> Option<crate::model::Duplicate> {
        if req.allow_duplicate {
            return None;
        }
        let url = util::reddit_video_url(req.url.trim());
        let hash = req
            .info_hash
            .clone()
            .or_else(|| crate::trackers::magnet_info_hash(&url))
            .map(|h| h.to_ascii_lowercase());
        let entries = self.entries.lock();
        entries
            .iter()
            .find(|e| {
                let same_hash = hash.as_deref().is_some_and(|h| {
                    e.rec
                        .torrent
                        .as_ref()
                        .and_then(|t| t.info_hash.as_deref())
                        .is_some_and(|x| x.eq_ignore_ascii_case(h))
                });
                same_hash || (!url.is_empty() && crate::model::same_link(&e.rec.url, &url))
            })
            .map(|e| crate::model::Duplicate {
                id: e.rec.id.clone(),
                name: e.rec.name.clone(),
                status: e.rec.status,
            })
    }

    pub async fn add(self: &Arc<Self>, req: AddRequest) -> Result<String> {
        let settings = self.settings();
        let url = util::reddit_video_url(req.url.trim());
        if url.is_empty() && req.torrent_file.is_none() {
            bail!("enter a link to download");
        }
        let mut kind = match req.kind.as_deref() {
            Some("http") => Kind::Http,
            Some("media") => Kind::Media,
            Some("torrent") => Kind::Torrent,
            _ if req.torrent_file.is_some() => Kind::Torrent,
            _ => detect_kind(&url),
        };
        if kind == Kind::Media && !settings.media_enabled {
            bail!("media downloads are disabled in Settings → Components");
        }
        if kind == Kind::Torrent && !settings.torrent_enabled {
            bail!("the torrent engine is disabled in Settings → Components");
        }
        let sha = normalize_sha(req.sha256.as_deref())?;
        let threads = req
            .threads
            .unwrap_or(settings.threads)
            .clamp(1, MAX_THREADS);
        let base_dir = req
            .dir
            .clone()
            .filter(|d| !d.trim().is_empty())
            .unwrap_or_else(|| settings.download_dir.clone());
        let id = uuid::Uuid::new_v4().to_string();

        let mut rec = DownloadRecord {
            id: id.clone(),
            kind,
            url: url.clone(),
            threads,
            referrer: req.referrer.clone(),
            user_agent: req.user_agent.clone(),
            speed_limit: req.speed_limit.unwrap_or(0),
            sha256_expected: sha.clone(),
            hash_state: if sha.is_some() {
                HashState::Pending
            } else {
                HashState::None
            },
            status: if req.start_paused {
                Status::Paused
            } else {
                Status::Queued
            },
            ..blank_record(&url)
        };

        if kind == Kind::Http {
            rec.cookie_header = cookie_header(&req.cookies, &url);
            let mut headers = self.request_headers(&rec);
            let client = self.http.read().clone();
            let probed = async {
                let resolved = resolver::resolve(&client, &url).await?;
                let probe =
                    engine::probe_with_fallback(&client, &resolved.url, &mut headers).await?;
                anyhow::Ok((resolved, probe))
            }
            .await;
            if headers.get(USER_AGENT).and_then(|v| v.to_str().ok()) == Some(engine::APP_USER_AGENT)
            {
                rec.user_agent = Some(engine::APP_USER_AGENT.to_string());
            }
            let mut name = req.filename.clone().filter(|n| !n.trim().is_empty());
            let mut content_type: Option<String> = None;
            match probed {
                Ok((resolved, probe)) => {
                    content_type = probe.content_type.clone();
                    let is_page = resolved.service.is_none()
                        && probe
                            .content_type
                            .as_deref()
                            .map(|c| c.starts_with("text/html"))
                            .unwrap_or(false)
                        && req.kind.as_deref() != Some("http");
                    if is_page && settings.media_enabled {
                        kind = Kind::Media;
                        rec.kind = Kind::Media;
                    } else {
                        name = name.or(resolved.filename).or(probe.filename);
                        rec.total = resolved.size.or(probe.total);
                        rec.resumable = probe.resumable;
                    }
                }
                Err(e) => {
                    tracing::warn!("probe failed for {url}: {e:#}");
                    name = name.or_else(|| util::filename_from_url(&url));
                }
            }
            if kind == Kind::Http {
                let mut name = util::sanitize_filename(&name.unwrap_or_else(|| "download".into()));
                if util::extension(&name).is_empty() {
                    if let Some(ext) = content_type
                        .as_deref()
                        .and_then(|ct| {
                            mime_guess::get_mime_extensions_str(
                                ct.split(';').next().unwrap_or(ct).trim(),
                            )
                        })
                        .and_then(|e| e.first())
                    {
                        name.push('.');
                        name.push_str(ext);
                    }
                }
                rec.category = util::category_for(&name);
                let dir = self.target_dir(&base_dir, rec.category, settings.organize_by_category);
                std::fs::create_dir_all(&dir).ok();
                let path = util::unique_path(&dir, &name);
                rec.name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or(name);
                rec.dir = dir.to_string_lossy().into_owned();
            }
        }

        if kind == Kind::Media {
            let quality = req
                .quality
                .clone()
                .unwrap_or_else(|| settings.media_quality.clone());
            rec.category = if quality.starts_with("audio") {
                Category::Music
            } else {
                Category::Video
            };
            let dir = self.target_dir(&base_dir, rec.category, settings.organize_by_category);
            std::fs::create_dir_all(&dir).ok();
            rec.dir = dir.to_string_lossy().into_owned();
            rec.name =
                util::media_title(req.page_title.as_deref(), &url).unwrap_or_else(|| url.clone());
            rec.sha256_expected = None;
            rec.hash_state = HashState::None;
            let cookies_file = if req.cookies.is_empty() {
                None
            } else {
                let path = self.paths.cookies.join(format!("{id}.txt"));
                media::write_cookie_file(&path, &req.cookies)?;
                Some(path.to_string_lossy().into_owned())
            };
            rec.media = Some(MediaOptions {
                quality,
                playlist: req.playlist,
                cookies_file,
                thumbnail: req.thumbnail.clone(),
                files: Vec::new(),
                filename: req.filename.clone().filter(|f| !f.trim().is_empty()),
                partials: Vec::new(),
                audio_url: req.audio_url.clone().filter(|a| a.starts_with("https://")),
            });
        }

        if kind == Kind::Torrent {
            rec.category = Category::Other;
            rec.dir = base_dir.clone();
            rec.name = req
                .page_title
                .clone()
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| "Fetching metadata…".into());
            rec.total = req.total;
            rec.torrent = Some(TorrentOptions {
                info_hash: req.info_hash.clone(),
                source: url.clone(),
                torrent_file: req.torrent_file.clone(),
                only_files: req.only_files.clone(),
                trackers: req.trackers.clone(),
                user_paused: req.start_paused,
                ..Default::default()
            });
            rec.status = if req.start_paused {
                Status::Paused
            } else {
                Status::Active
            };
            rec.threads = 0;
        }

        self.entries.lock().insert(
            0,
            Entry {
                rec,
                live: Live::default(),
                run: None,
            },
        );
        self.mark_dirty();
        if kind == Kind::Torrent {
            let this = self.clone();
            let id2 = id.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = this.add_torrent_task(&id2).await {
                    this.fail(&id2, format!("{e:#}"));
                }
            });
        } else {
            self.schedule();
        }
        self.emit_now();
        if let Ok(rec) = self.record(&id) {
            let name = if rec.name.is_empty() {
                rec.url.clone()
            } else {
                rec.name.clone()
            };
            self.notify(Some(&id), "started", &name);
        }
        Ok(id)
    }

    fn target_dir(&self, base: &str, cat: Category, organize: bool) -> PathBuf {
        let base = PathBuf::from(base);
        if organize {
            base.join(category_dir(cat))
        } else {
            base
        }
    }

    pub fn schedule(self: &Arc<Self>) {
        let max = {
            let s = self.settings.read();
            if s.sequential {
                1
            } else {
                s.max_concurrent as usize
            }
        };
        let to_start: Vec<String> = {
            let entries = self.entries.lock();
            let running = entries
                .iter()
                .filter(|e| e.run.is_some() && e.rec.kind != Kind::Torrent)
                .count();
            let mut queued: Vec<&Entry> = entries
                .iter()
                .filter(|e| e.rec.status == Status::Queued && e.run.is_none())
                .collect();
            queued.sort_by_key(|e| e.rec.queue_order);
            queued
                .into_iter()
                .take(max.saturating_sub(running))
                .map(|e| e.rec.id.clone())
                .collect()
        };
        for id in to_start {
            self.start(&id);
        }
    }

    fn start(self: &Arc<Self>, id: &str) {
        let cancel = CancellationToken::new();
        let Some((kind, limit)) = self.with_entry(id, |e| {
            e.rec.status = Status::Active;
            e.rec.error = None;
            e.live = Live::default();
            let limiter = Arc::new(Limiter::new(e.rec.speed_limit * 1024));
            e.run = Some(Running {
                cancel: cancel.clone(),
                progress: None,
                limiter: limiter.clone(),
                last_received: 0,
                last_tick: Instant::now(),
                ema: 0.0,
                handle: None,
                media_attempt: None,
                progress_base: 0,
                progress_total: None,
                peak: 0,
            });
            (e.rec.kind, limiter)
        }) else {
            return;
        };
        let this = self.clone();
        let id2 = id.to_string();
        let handle = tauri::async_runtime::spawn(async move {
            let result = match kind {
                Kind::Http | Kind::Media => this.clone().run_resilient(&id2, cancel, limit).await,
                Kind::Torrent => Ok(false),
            };
            this.on_finished(&id2, result).await;
        });
        self.with_entry(id, |e| {
            if let Some(r) = e.run.as_mut() {
                r.handle = Some(handle);
            }
        });
        self.mark_dirty();
    }

    async fn run_resilient(
        self: Arc<Self>,
        id: &str,
        cancel: CancellationToken,
        limit: Arc<Limiter>,
    ) -> Result<bool> {
        const MAX_RECOVERIES: u32 = 8;
        let mut recoveries = 0u32;
        loop {
            let rec = self.record(id)?;
            let result = match rec.kind {
                Kind::Media => self.clone().run_media(id, cancel.clone()).await,
                _ => {
                    self.clone()
                        .run_http(id, cancel.clone(), limit.clone())
                        .await
                }
            };
            let err = match result {
                Err(e) if !cancel.is_cancelled() => e,
                other => return other,
            };
            if crate::security::antivirus_error(&err).is_some() || recoveries >= MAX_RECOVERIES {
                return Err(err);
            }
            let failure = media::classify_failure(&format!("{err:#}"));
            if failure == media::Failure::Other
                || (failure == media::Failure::Corrupt && rec.kind != Kind::Media)
            {
                return Err(err);
            }
            recoveries += 1;
            tracing::warn!(
                "{} failed ({failure:?}), recovering ({recoveries}/{MAX_RECOVERIES}): {err:#}",
                rec.url
            );
            if failure == media::Failure::Corrupt {
                self.discard_media_pieces(id).await;
            }
            if !self.wait_for_network(id, &rec.url, &cancel).await {
                return Ok(false);
            }
            self.with_entry(id, |e| {
                e.live.phase = Some("retrying".into());
                e.live.speed = 0;
                e.live.eta = None;
            });
            self.mark_dirty();
            let pause = Duration::from_secs((2 * recoveries as u64).min(20));
            tokio::select! {
                _ = cancel.cancelled() => return Ok(false),
                _ = tokio::time::sleep(pause) => {}
            }
        }
    }

    async fn wait_for_network(&self, id: &str, url: &str, cancel: &CancellationToken) -> bool {
        let origin = url::Url::parse(url)
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
            .map(|u| u.origin().ascii_serialization());
        let Some(origin) = origin else {
            return !cancel.is_cancelled();
        };
        let client = self.http.read().clone();
        let mut announced = false;
        loop {
            let reachable = tokio::select! {
                _ = cancel.cancelled() => return false,
                r = client.get(&origin).timeout(Duration::from_secs(8)).send() => r.is_ok(),
            };
            if reachable {
                return true;
            }
            if !announced {
                announced = true;
                self.with_entry(id, |e| {
                    e.live.phase = Some("waiting for network".into());
                    e.live.speed = 0;
                    e.live.eta = None;
                });
                self.mark_dirty();
            }
            tokio::select! {
                _ = cancel.cancelled() => return false,
                _ = tokio::time::sleep(Duration::from_secs(3)) => {}
            }
        }
    }

    async fn discard_media_pieces(&self, id: &str) {
        let Some((partials, files)) = self
            .with_entry(id, |e| {
                let m = e.rec.media.as_mut()?;
                Some((std::mem::take(&mut m.partials), m.files.clone()))
            })
            .flatten()
        else {
            return;
        };
        let mut targets = Vec::new();
        for p in partials.iter().filter(|p| !files.contains(p)) {
            targets.extend(media_leftovers(Path::new(p)));
        }
        let left = delete_paths(targets).await;
        if !left.is_empty() {
            tracing::warn!("could not discard {} broken media piece(s)", left.len());
        }
    }

    async fn on_finished(self: &Arc<Self>, id: &str, result: Result<bool>) {
        let mut notify: Option<(&str, String)> = None;
        self.with_entry(id, |e| {
            e.run = None;
            e.live.speed = 0;
            e.live.eta = None;
            e.live.connections = 0;
            match &result {
                Ok(true) => {
                    if e.rec.status != Status::Failed {
                        e.rec.status = Status::Completed;
                        e.rec.completed_at = Some(now_ms());
                        e.rec.error = None;
                        if let Some(t) = e.rec.total {
                            e.rec.downloaded = t;
                        }
                        e.rec.segments.clear();
                        e.live.phase = None;
                        notify = Some(("done", e.rec.name.clone()));
                    }
                }
                Ok(false) => {
                    if e.rec.status == Status::Active {
                        e.rec.status = Status::Paused;
                    }
                }
                Err(err) => {
                    if e.rec.status == Status::Active {
                        e.rec.status = Status::Failed;
                        if let Some(why) = crate::security::antivirus_error(err) {
                            e.rec.error = Some(why.to_string());
                            e.rec.scan_state = "threat".into();
                            e.rec.scan_detail = Some(why.to_string());
                            notify = Some(("threat", format!("{}: {why}", e.rec.name)));
                        } else if crate::security::access_denied(err) {
                            let why = "access denied: the file or folder may be open in another program or protected, or your antivirus may have blocked it";
                            e.rec.error = Some(format!("{why} ({err:#})"));
                            notify = Some(("failed", format!("{}: {why}", e.rec.name)));
                        } else {
                            e.rec.error = Some(format!("{err:#}"));
                            notify = Some(("failed", format!("{}: {err}", e.rec.name)));
                        }
                    }
                }
            }
        });
        if let Some(rec) = self.with_entry(id, |e| e.rec.clone()) {
            if rec.status == Status::Completed {
                if let Some(c) = rec.media.as_ref().and_then(|m| m.cookies_file.clone()) {
                    let _ = std::fs::remove_file(c);
                }
                if notify.is_some() {
                    self.run_on_complete(&rec);
                    self.after_download(rec.clone());
                }
            }
        }
        if let Some((key, body)) = notify {
            self.notify(Some(id), key, &body);
        }
        self.mark_dirty();
        self.persist();
        self.schedule();
        self.emit_now();
    }

    fn fail(&self, id: &str, msg: String) {
        self.with_entry(id, |e| {
            e.rec.status = Status::Failed;
            e.rec.error = Some(msg);
            e.run = None;
        });
        self.mark_dirty();
        self.emit_now();
    }

    async fn run_http(
        self: Arc<Self>,
        id: &str,
        cancel: CancellationToken,
        limiter: Arc<Limiter>,
    ) -> Result<bool> {
        let rec = self.record(id)?;
        let settings = self.settings();
        let mut headers = self.request_headers(&rec);
        let client = self.http.read().clone();
        self.with_entry(id, |e| e.live.phase = Some("connecting".into()));

        let resolved = resolver::resolve(&client, &rec.url).await?;
        let mut probe = engine::probe_with_fallback(&client, &resolved.url, &mut headers).await?;
        if headers.get(USER_AGENT).and_then(|v| v.to_str().ok()) == Some(engine::APP_USER_AGENT) {
            self.with_entry(id, |e| {
                e.rec.user_agent = Some(engine::APP_USER_AGENT.to_string())
            });
        }
        if probe.total.is_none() {
            probe.total = resolved.size;
        }
        if probe
            .content_type
            .as_deref()
            .map(|c| c.starts_with("text/html"))
            .unwrap_or(false)
            && resolved.service.is_none()
            && !matches!(util::extension(&rec.name).as_str(), "html" | "htm")
        {
            bail!("the link opens a web page instead of a file (it may have expired or need a login). Try adding it from the browser extension");
        }
        let final_path = PathBuf::from(&rec.dir).join(&rec.name);
        let part = util::part_path(&final_path);
        let part_ok = tokio::fs::metadata(&part)
            .await
            .map(|m| Some(m.len()) == probe.total)
            .unwrap_or(false);
        let unchanged = match (&rec.validator, &probe.validator) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        };
        let same_file = part_ok
            && unchanged
            && probe.total.is_some()
            && probe.total == rec.total
            && probe.resumable;
        let segments = if same_file {
            rec.segments.clone()
        } else {
            Vec::new()
        };
        let progress = Progress::new(probe.total, &segments, rec.threads);
        self.with_entry(id, |e| {
            e.rec.total = probe.total;
            e.rec.resumable = probe.resumable;
            e.rec.validator = probe.validator.clone();
            if !same_file {
                e.rec.segments.clear();
                e.rec.downloaded = 0;
            }
            e.live.phase = Some("downloading".into());
            if let Some(r) = e.run.as_mut() {
                r.progress = Some(progress.clone());
                r.peak = 0;
                r.last_received = 0;
                r.last_tick = Instant::now();
            }
        });

        if !same_file && part.exists() {
            let _ = tokio::fs::remove_file(&part).await;
        }
        let job = Arc::new(engine::Job {
            client,
            url: probe.final_url.clone(),
            headers,
            part: part.clone(),
            resumable: probe.resumable,
            progress: progress.clone(),
            cancel,
            limiters: vec![self.global_limit.clone(), limiter],
            retries: settings.retries,
            transform: resolved.transform,
            if_range: probe.validator.clone(),
        });
        let done = engine::run(job).await;
        self.with_entry(id, |e| {
            e.rec.segments = progress.segments();
            e.rec.downloaded = progress.downloaded();
            if e.rec.total.is_none() {
                e.rec.total = progress.total();
            }
        });
        if !done? {
            return Ok(false);
        }

        let target = if final_path.exists() {
            util::unique_path(final_path.parent().unwrap_or(Path::new(".")), &rec.name)
        } else {
            final_path
        };
        if !part.exists() {
            bail!(
                "the file was removed right after it was downloaded, most likely by your antivirus"
            );
        }
        tokio::fs::rename(&part, &target)
            .await
            .with_context(|| format!("could not move the file into {}", target.display()))?;
        if let Some(n) = target.file_name() {
            let n = n.to_string_lossy().into_owned();
            self.with_entry(id, |e| e.rec.name = n);
        }
        self.verify_hash(id, &target).await;
        Ok(true)
    }

    pub async fn verify_hash(&self, id: &str, path: &Path) {
        let (expected, ask) = match self.with_entry(id, |e| e.rec.sha256_expected.clone()) {
            Some(exp) => (exp, self.settings.read().ask_sha256),
            None => return,
        };
        if expected.is_none() && !ask {
            return;
        }
        self.with_entry(id, |e| {
            e.rec.hash_state = HashState::Verifying;
            e.live.phase = Some("verifying".into());
        });
        self.emit_now();
        let p = path.to_path_buf();
        let actual = tokio::task::spawn_blocking(move || sha256_file(&p)).await;
        let actual = match actual {
            Ok(Ok(h)) => h,
            Ok(Err(e)) => {
                self.with_entry(id, |en| {
                    en.rec.hash_state = HashState::None;
                    en.rec.error = Some(format!("could not hash the file: {e:#}"));
                });
                return;
            }
            Err(_) => return,
        };
        let mut message = None;
        self.with_entry(id, |e| {
            e.live.phase = None;
            e.rec.sha256_actual = Some(actual.clone());
            match &expected {
                Some(exp) if exp.eq_ignore_ascii_case(&actual) => {
                    e.rec.hash_state = HashState::Ok;
                    message = Some(("hash_ok", e.rec.name.clone()));
                }
                Some(exp) => {
                    e.rec.hash_state = HashState::Mismatch;
                    e.rec.status = Status::Failed;
                    e.rec.error = Some(format!(
                        "SHA-256 mismatch: expected {exp}, got {actual}. The file may be corrupted or tampered with."
                    ));
                    message = Some(("hash_bad", e.rec.name.clone()));
                }
                None => e.rec.hash_state = HashState::Computed,
            }
        });
        if let Some((k, body)) = message {
            self.notify(Some(id), k, &body);
        }
        self.mark_dirty();
    }

    pub async fn set_expected_hash(self: &Arc<Self>, id: &str, hash: Option<String>) -> Result<()> {
        let sha = normalize_sha(hash.as_deref())?;
        let rec = self.record(id)?;
        self.with_entry(id, |e| {
            e.rec.sha256_expected = sha.clone();
            e.rec.hash_state = if sha.is_some() {
                HashState::Pending
            } else {
                HashState::None
            };
        });
        if rec.kind == Kind::Http && matches!(rec.status, Status::Completed | Status::Failed) {
            let path = PathBuf::from(&rec.dir).join(&rec.name);
            if path.is_file() {
                if rec.status == Status::Failed && rec.hash_state == HashState::Mismatch {
                    self.with_entry(id, |e| {
                        e.rec.status = Status::Completed;
                        e.rec.error = None;
                    });
                }
                self.verify_hash(id, &path).await;
            }
        }
        self.mark_dirty();
        self.emit_now();
        Ok(())
    }

    async fn run_media(self: Arc<Self>, id: &str, cancel: CancellationToken) -> Result<bool> {
        let rec = self.record(id)?;
        let settings = self.settings();
        let media = rec.media.clone().unwrap_or_default();
        if media::instagram_profile(&rec.url) {
            bail!("this is an Instagram profile, which cannot be downloaded as a whole. Open the post, reel or story you want and download that, or use the browser extension's list of the videos on the page")
        }
        self.with_entry(id, |e| e.live.phase = Some("preparing".into()));
        let app = self.app.clone();
        let exe = self
            .tools
            .ensure_ytdlp(move |p: ToolProgress| {
                let _ = app.emit(EVT_TOOLS, p);
            })
            .await
            .context("could not install yt-dlp")?;

        if rec.name == rec.url && !media.playlist {
            let this = self.clone();
            let id2 = id.to_string();
            let url = rec.url.clone();
            let youtube = media::is_youtube(&rec.url);
            let cookies = media.cookies_file.clone().filter(|_| !youtube);
            let browser = if youtube {
                String::new()
            } else {
                settings.cookies_from_browser.clone()
            };
            tokio::spawn(async move {
                let title = this
                    .tools
                    .probe(&url, cookies.as_deref().map(Path::new), &browser)
                    .await
                    .ok()
                    .map(|i| i.title.trim().to_string())
                    .filter(|t| !t.is_empty());
                if let Some(title) = title {
                    this.with_entry(&id2, |e| {
                        if e.rec.name == url {
                            e.rec.name = title;
                        }
                    });
                    this.dirty.store(true, Ordering::Relaxed);
                }
            });
        }

        let mut ffmpeg = self.tools.ffmpeg_location();
        if ffmpeg.is_none() && crate::media::ffmpeg_auto_install() {
            self.with_entry(id, |e| e.live.phase = Some("installing FFmpeg".into()));
            self.dirty.store(true, Ordering::Relaxed);
            let installed = self
                .tools
                .install_ffmpeg(self.tool_progress(id, "FFmpeg"), &cancel)
                .await;
            self.tool_finished(id);
            if cancel.is_cancelled() {
                return Ok(false);
            }
            match installed {
                Ok(()) => ffmpeg = self.tools.ffmpeg_location(),
                Err(e) => {
                    tracing::warn!("FFmpeg install failed: {e:#}");
                    self.toast(
                        "warning",
                        format!("FFmpeg could not be installed, so the video is saved in a lower quality: {e:#}"),
                    );
                }
            }
        }
        let mut js_runtime = self.tools.deno_location();
        if js_runtime.is_none() && media::is_youtube(&rec.url) && media::deno_auto_install() {
            self.with_entry(id, |e| e.live.phase = Some("installing Deno".into()));
            self.dirty.store(true, Ordering::Relaxed);
            let installed = self
                .tools
                .install_deno(self.tool_progress(id, "Deno"), &cancel)
                .await;
            self.tool_finished(id);
            if cancel.is_cancelled() {
                return Ok(false);
            }
            match installed {
                Ok(v) => {
                    tracing::info!("Deno {v} installed for YouTube");
                    js_runtime = self.tools.deno_location();
                }
                Err(e) => {
                    tracing::warn!("Deno install failed: {e:#}");
                    self.toast(
                        "warning",
                        format!(
                            "Deno could not be installed, so YouTube may download slowly: {e:#}"
                        ),
                    );
                }
            }
        }
        let has_ffmpeg = ffmpeg.is_some();

        if let Some(audio) = media.audio_url.clone() {
            let Some(ffmpeg) = ffmpeg else {
                bail!("joining the picture and sound of this video needs FFmpeg. Install it in Settings → Media")
            };
            let dest = util::unique_path(&PathBuf::from(&rec.dir), &rec.name);
            let this = self.clone();
            let id2 = id.to_string();
            let mut meter = media::SpeedMeter::default();
            return media::merge(
                &ffmpeg,
                &rec.url,
                &audio,
                rec.referrer.as_deref(),
                rec.user_agent.as_deref(),
                &dest,
                cancel,
                move |ev| {
                    this.with_entry(&id2, |e| match ev {
                        MediaEvent::Progress { downloaded, .. } => {
                            e.rec.downloaded = downloaded;
                            e.live.speed = meter.sample(downloaded);
                            e.live.phase = Some("downloading".into());
                        }
                        MediaEvent::File(path) => {
                            if let Some(m) = e.rec.media.as_mut() {
                                m.files.push(path.to_string_lossy().into_owned());
                            }
                            if let Some(n) = path.file_name() {
                                e.rec.name = n.to_string_lossy().into_owned();
                                e.rec.category = util::category_for(&e.rec.name);
                            }
                            if let Ok(meta) = std::fs::metadata(&path) {
                                e.rec.total = Some(meta.len());
                                e.rec.downloaded = meta.len();
                            }
                        }
                        _ => {}
                    });
                    this.dirty.store(true, Ordering::Relaxed);
                },
            )
            .await;
        }

        let base = MediaJob {
            exe,
            url: rec.url.clone(),
            dir: PathBuf::from(&rec.dir),
            quality: media.quality.clone(),
            container: settings.media_container.clone(),
            playlist: media.playlist,
            fragments: rec.threads,
            rate_limit: if rec.speed_limit > 0 {
                rec.speed_limit
            } else {
                settings.speed_limit
            },
            ffmpeg,
            cookies_file: media.cookies_file.as_ref().map(PathBuf::from),
            cookies_browser: settings.cookies_from_browser.clone(),
            embed_metadata: settings.embed_metadata,
            embed_thumbnail: settings.embed_thumbnail,
            filename: media.filename.clone(),
            js_runtime,
            local_info: None,
        };
        let mut base = base;
        let has_cookies = base.cookies_file.is_some() || !base.cookies_browser.trim().is_empty();
        let youtube = media::is_youtube(&rec.url);
        let mut attempts: Vec<bool> = match (has_cookies, youtube) {
            (false, _) => vec![false],
            (true, true) => vec![false, true],
            (true, false) => vec![true, false],
        };
        let mut scratch: Vec<PathBuf> = Vec::new();
        if !media.playlist && !youtube {
            let mut first = base.clone();
            if !attempts[0] {
                first.cookies_file = None;
                first.cookies_browser = String::new();
            }
            match self
                .clone()
                .fetch_media_direct(id, &first, &cancel, &mut scratch)
                .await
            {
                Ok(Some(info)) => {
                    base.local_info = Some(info);
                    attempts = vec![attempts[0]];
                }
                Ok(None) => {}
                Err(_) if cancel.is_cancelled() => {
                    remove_scratch(&scratch).await;
                    return Ok(false);
                }
                Err(e) => {
                    tracing::info!("downloading {} with yt-dlp instead: {e:#}", rec.url);
                    remove_scratch(&scratch).await;
                    scratch.clear();
                }
            }
            self.with_entry(id, |e| {
                if let Some(r) = e.run.as_mut() {
                    r.progress = None;
                    r.progress_base = 0;
                    r.progress_total = None;
                }
                e.rec.segments.clear();
                e.live.phase = Some(
                    if base.local_info.is_some() {
                        "merging"
                    } else {
                        "preparing"
                    }
                    .into(),
                );
            });
        }
        let mut result = Ok(false);
        for (n, with_cookies) in attempts.iter().copied().enumerate() {
            let mut job = base.clone();
            if !with_cookies {
                job.cookies_file = None;
                job.cookies_browser = String::new();
            }
            loop {
                let attempt = cancel.child_token();
                job.fragments = self.record(id).map(|r| r.threads).unwrap_or(job.fragments);
                let fragments = job.fragments;
                self.with_entry(id, |e| {
                    if let Some(r) = e.run.as_mut() {
                        r.media_attempt = Some((attempt.clone(), fragments));
                    }
                });
                let this = self.clone();
                let id2 = id.to_string();
                let media = media.clone();
                let mut last_index: Option<u32> = None;
                let mut totals = media::MediaTotals::default();
                let mut meter = media::SpeedMeter::default();
                let local = job.local_info.is_some();
                result = media::run(job.clone(), attempt.clone(), move |ev| {
                    this.with_entry(&id2, |e| match ev {
                        MediaEvent::Progress {
                            downloaded,
                            total,
                            estimate,
                            speed,
                            eta,
                            index,
                            count,
                        } => {
                            if index != last_index {
                                totals = media::MediaTotals::default();
                                meter = media::SpeedMeter::default();
                                last_index = index;
                            }
                            if local {
                                e.live.phase = Some("merging".into());
                                return;
                            }
                            let (shown, size) = totals.update(downloaded, total, estimate);
                            e.rec.downloaded = shown;
                            e.rec.total = size;
                            let measured = meter.sample(shown);
                            e.live.speed = if measured > 0 {
                                measured
                            } else {
                                speed.unwrap_or(0)
                            };
                            e.live.eta = match size {
                                Some(t) if e.live.speed > 0 => {
                                    Some(t.saturating_sub(shown) / e.live.speed)
                                }
                                _ => eta,
                            };
                            e.live.item_index = index;
                            e.live.item_count = count;
                            e.live.phase = Some("downloading".into());
                            e.live.connections = e.rec.threads;
                        }
                        MediaEvent::Size(size) => {
                            if !media.playlist {
                                totals.set_exact(size);
                            }
                        }
                        MediaEvent::Phase(p) => {
                            e.live.phase = Some(p);
                            e.live.speed = 0;
                            e.live.eta = None;
                        }
                        MediaEvent::Title(t) => {
                            if !media.playlist && !t.is_empty() {
                                e.rec.name = t;
                            }
                        }
                        MediaEvent::Destination(path) => {
                            if let Some(m) = e.rec.media.as_mut() {
                                let p = path.to_string_lossy().into_owned();
                                if !m.partials.contains(&p) {
                                    m.partials.push(p);
                                }
                            }
                        }
                        MediaEvent::File(path) => {
                            if let Some(m) = e.rec.media.as_mut() {
                                m.files.push(path.to_string_lossy().into_owned());
                            }
                            if !media.playlist {
                                if let Some(n) = path.file_name() {
                                    e.rec.name = n.to_string_lossy().into_owned();
                                    e.rec.category = util::category_for(&e.rec.name);
                                }
                                if let Some(parent) = path.parent() {
                                    e.rec.dir = parent.to_string_lossy().into_owned();
                                }
                                match e.rec.total {
                                    Some(t) if e.rec.downloaded > 0 => e.rec.downloaded = t,
                                    _ => {
                                        if let Ok(meta) = std::fs::metadata(&path) {
                                            e.rec.total = Some(meta.len());
                                            e.rec.downloaded = meta.len();
                                        }
                                    }
                                }
                            }
                        }
                    });
                    this.dirty.store(true, Ordering::Relaxed);
                })
                .await;
                self.with_entry(id, |e| {
                    if let Some(r) = e.run.as_mut() {
                        r.media_attempt = None;
                    }
                });
                if matches!(result, Ok(false)) && attempt.is_cancelled() && !cancel.is_cancelled() {
                    tracing::info!(
                        "restarting {} with {} connections",
                        rec.url,
                        self.record(id).map(|r| r.threads).unwrap_or(0)
                    );
                    continue;
                }
                break;
            }
            let retry = match &result {
                Err(e) if n + 1 < attempts.len() => {
                    let text = format!("{e:#}");
                    if with_cookies {
                        media::cookies_rejected(&text)
                    } else {
                        media::needs_sign_in(&text)
                    }
                }
                _ => false,
            };
            if !retry {
                break;
            }
            tracing::info!(
                "retrying {} {} cookies",
                rec.url,
                if with_cookies { "without" } else { "with" }
            );
            self.with_entry(id, |e| {
                e.rec.downloaded = 0;
                e.rec.total = None;
                e.live.speed = 0;
                e.live.phase = Some("preparing".into());
            });
        }
        remove_scratch(&scratch).await;
        match result {
            Err(e) if !has_cookies && media::needs_sign_in(&format!("{e:#}")) => {
                bail!(
                    "{} only shows this to signed-in users. Sign in to it in your browser and download with the Multron Download Manager browser extension, or choose your browser in Settings → Media → Use cookies from browser ({e})",
                    media::site_name(&rec.url)
                )
            }
            Err(e)
                if !has_ffmpeg && e.to_string().contains("Requested format is not available") =>
            {
                bail!("this site only offers separate video and audio streams, which need FFmpeg. Install it in Settings → Media (Linux: sudo apt install ffmpeg / sudo dnf install ffmpeg-free)")
            }
            other => other,
        }
    }

    fn torrent_if_running(&self) -> Option<Arc<TorrentEngine>> {
        self.torrent_ready.read().clone()
    }

    fn engine_config(&self) -> EngineConfig {
        let s = self.settings();
        let filter = (s.ip_filter_enabled && self.paths.ip_filter.is_file())
            .then(|| self.paths.ip_filter.clone());
        EngineConfig {
            default_dir: PathBuf::from(&s.download_dir),
            state_dir: self.paths.torrent_state.clone(),
            port: s.torrent_port,
            dht: s.torrent_dht,
            upnp: s.torrent_upnp,
            utp: s.torrent_utp,
            lsd: s.torrent_lsd,
            ipv4_only: s.torrent_ipv4_only,
            peer_limit: s.torrent_peer_limit,
            download_kbps: s.torrent_download_limit,
            upload_kbps: s.torrent_upload_limit,
            trackers: s.extra_trackers.clone(),
            proxy: s.torrent_proxy.clone(),
            bind_interface: s.torrent_bind_interface.clone(),
            blocklist: if s.ip_filter_allow_mode {
                None
            } else {
                filter.clone()
            },
            allowlist: if s.ip_filter_allow_mode { filter } else { None },
            disable_trackers: s.torrent_disable_trackers,
            hide_client: s.torrent_hide_client,
            scrape: s.torrent_scrape,
            public_trackers: s.torrent_public_trackers,
        }
    }

    pub async fn torrent_engine(self: &Arc<Self>) -> Result<Arc<TorrentEngine>> {
        if !self.settings.read().torrent_enabled {
            bail!("the torrent engine is disabled in Settings → Components");
        }
        let mut guard = self.torrent.lock().await;
        if let Some(e) = guard.as_ref() {
            return Ok(e.clone());
        }
        let engine = TorrentEngine::start(self.engine_config(), self.api_client.clone())
            .await
            .map_err(|e| {
                *self.torrent_error.lock() = Some(format!("{e:#}"));
                e
            })?;
        let engine = Arc::new(engine);
        if let Some(w) = &engine.warning {
            self.toast("warning", w.clone());
        }
        *self.torrent_error.lock() = None;
        *guard = Some(engine.clone());
        *self.torrent_ready.write() = Some(engine.clone());
        drop(guard);
        self.restore_torrents(&engine).await;
        Ok(engine)
    }

    pub fn torrent_error(&self) -> Option<String> {
        self.torrent_error.lock().clone()
    }

    async fn stop_torrent_engine(&self) {
        let mut guard = self.torrent.lock().await;
        if let Some(e) = guard.take() {
            *self.torrent_ready.write() = None;
            e.stop().await;
        }
    }

    pub async fn restart_torrent_engine(self: &Arc<Self>) {
        let was_running = self.torrent.lock().await.is_some();
        self.stop_torrent_engine().await;
        if was_running {
            if let Err(e) = self.torrent_engine().await {
                self.toast("error", format!("Torrent engine: {e:#}"));
            }
        }
    }

    async fn restore_torrents(&self, engine: &Arc<TorrentEngine>) {
        let recs: Vec<DownloadRecord> = self
            .entries
            .lock()
            .iter()
            .filter(|e| e.rec.kind == Kind::Torrent)
            .map(|e| e.rec.clone())
            .collect();
        for rec in recs {
            let Some(t) = rec.torrent.as_ref() else {
                continue;
            };
            let Some(hash) = t.info_hash.as_deref() else {
                continue;
            };
            if engine.handle(hash).is_some() {
                continue;
            }
            let file = self.paths.torrents.join(format!("{hash}.torrent"));
            let paused = t.user_paused || rec.status == Status::Paused;
            if let Err(e) = engine
                .restore(
                    Some(&file),
                    &t.source,
                    Path::new(&rec.dir),
                    t.only_files.clone(),
                    paused,
                )
                .await
            {
                tracing::warn!("could not restore torrent {hash}: {e:#}");
            }
        }
    }

    async fn add_torrent_task(self: &Arc<Self>, id: &str) -> Result<()> {
        let engine = self.torrent_engine().await?;
        let rec = self.record(id)?;
        let opts = rec.torrent.clone().unwrap_or_default();
        let src = match &opts.torrent_file {
            Some(f) => TorrentSource::File(PathBuf::from(f)),
            None => TorrentSource::Url(opts.source.clone()),
        };
        self.with_entry(id, |e| e.live.phase = Some("metadata".into()));
        self.emit_now();
        let added = engine
            .add(
                &src,
                opts.info_hash.as_deref(),
                Path::new(&rec.dir),
                opts.only_files.clone(),
                opts.user_paused,
                opts.trackers.clone(),
            )
            .await?;
        let torrent_file = match &added.torrent_bytes {
            Some(bytes) => {
                std::fs::create_dir_all(&self.paths.torrents).ok();
                let p = self
                    .paths
                    .torrents
                    .join(format!("{}.torrent", added.info_hash));
                std::fs::write(&p, bytes).ok();
                Some(p.to_string_lossy().into_owned())
            }
            None => opts.torrent_file.clone(),
        };
        self.with_entry(id, |e| {
            e.rec.name = added.name.clone();
            e.rec.total = Some(added.total);
            e.rec.dir = added.output_folder.to_string_lossy().into_owned();
            e.live.phase = None;
            if let Some(t) = e.rec.torrent.as_mut() {
                t.info_hash = Some(added.info_hash.clone());
                t.files = added.files.clone();
                t.torrent_file = torrent_file;
            }
        });
        self.mark_dirty();
        self.persist();
        self.emit_now();
        Ok(())
    }

    pub async fn torrent_details(
        self: &Arc<Self>,
        id: &str,
    ) -> Result<crate::torrent::TorrentDetails> {
        let rec = self.record(id)?;
        let t = rec.torrent.ok_or_else(|| anyhow!("not a torrent"))?;
        let hash = t
            .info_hash
            .ok_or_else(|| anyhow!("metadata is still being fetched"))?;
        let engine = self.torrent_engine().await?;
        engine.details(&hash, &t.trackers)
    }

    pub async fn torrent_set_files(self: &Arc<Self>, id: &str, files: Vec<usize>) -> Result<()> {
        let rec = self.record(id)?;
        let hash = rec
            .torrent
            .as_ref()
            .and_then(|t| t.info_hash.clone())
            .ok_or_else(|| anyhow!("metadata is still being fetched"))?;
        let engine = self.torrent_engine().await?;
        engine.set_only_files(&hash, &files).await?;
        let list = engine.files(&hash);
        self.with_entry(id, |e| {
            if let Some(t) = e.rec.torrent.as_mut() {
                t.only_files = Some(files.clone());
                t.files = list;
            }
            if e.rec.status == Status::Completed {
                e.rec.status = Status::Active;
                e.rec.completed_at = None;
            }
        });
        self.mark_dirty();
        Ok(())
    }

    fn torrent_tick(self: &Arc<Self>, engine: &Arc<TorrentEngine>) {
        let settings = self.settings();
        let items: Vec<(String, String)> = self
            .entries
            .lock()
            .iter()
            .filter(|e| e.rec.kind == Kind::Torrent)
            .filter_map(|e| {
                e.rec
                    .torrent
                    .as_ref()
                    .and_then(|t| t.info_hash.clone())
                    .map(|h| (e.rec.id.clone(), h))
            })
            .collect();
        let mut to_pause = Vec::new();
        let mut finished_now = Vec::new();
        for (id, hash) in items {
            let Some(snap) = engine.snapshot(&hash) else {
                continue;
            };
            self.with_entry(&id, |e| {
                let user_paused = e
                    .rec
                    .torrent
                    .as_ref()
                    .map(|t| t.user_paused)
                    .unwrap_or(false);
                e.rec.total = Some(snap.total);
                e.rec.downloaded = snap.progress;
                e.live.speed = snap.download_speed;
                e.live.upload_speed = snap.upload_speed;
                e.live.peers = snap.peers;
                e.live.connections = snap.peers;
                e.live.eta = (snap.download_speed > 0 && !snap.finished)
                    .then(|| snap.total.saturating_sub(snap.progress) / snap.download_speed.max(1));
                if let Some(t) = e.rec.torrent.as_mut() {
                    t.uploaded = snap.uploaded;
                }
                let was_complete = e.rec.completed_at.is_some();
                match snap.state.as_str() {
                    "error" => {
                        e.rec.status = Status::Failed;
                        e.rec.error = snap.error.clone();
                        e.live.seeding = false;
                    }
                    "paused" => {
                        e.live.seeding = false;
                        e.live.phase = None;
                        let queued = e.rec.torrent.as_ref().map(|t| t.queued).unwrap_or(false);
                        e.rec.status = if snap.finished {
                            Status::Completed
                        } else if queued {
                            Status::Queued
                        } else {
                            Status::Paused
                        };
                    }
                    "live" => {
                        e.live.phase = None;
                        e.rec.error = None;
                        if snap.finished {
                            e.rec.status = Status::Completed;
                            e.live.seeding = true;
                            if !settings.seed_after_complete && !was_complete {
                                to_pause.push(hash.clone());
                                if let Some(t) = e.rec.torrent.as_mut() {
                                    t.seed_done = true;
                                }
                            }
                        } else {
                            e.rec.status = Status::Active;
                            e.live.seeding = false;
                        }
                        if user_paused {
                            to_pause.push(hash.clone());
                        }
                    }
                    _ => {
                        e.live.phase = Some("checking".into());
                        if e.rec.status != Status::Paused {
                            e.rec.status = Status::Active;
                        }
                    }
                }
                if snap.finished && !was_complete {
                    e.rec.completed_at = Some(now_ms());
                    finished_now.push(e.rec.clone());
                }
            });
        }
        for rec in finished_now {
            self.notify(Some(&rec.id), "done", &rec.name);
            self.run_on_complete(&rec);
            self.after_download(rec);
            self.save_needed.store(true, Ordering::Relaxed);
        }
        for hash in to_pause {
            let engine = engine.clone();
            tauri::async_runtime::spawn(async move {
                let _ = engine.pause(&hash).await;
            });
        }
    }

    pub async fn pause(self: &Arc<Self>, id: &str) -> Result<()> {
        let rec = self.record(id)?;
        if rec.kind == Kind::Torrent {
            self.with_entry(id, |e| {
                if let Some(t) = e.rec.torrent.as_mut() {
                    t.user_paused = true;
                }
                if e.rec.status != Status::Completed {
                    e.rec.status = Status::Paused;
                }
            });
            if let Some(hash) = rec.torrent.and_then(|t| t.info_hash) {
                if let Some(engine) = self.torrent_if_running() {
                    engine.pause(&hash).await?;
                }
            }
        } else {
            self.with_entry(id, |e| {
                if matches!(e.rec.status, Status::Active | Status::Queued) {
                    e.rec.status = Status::Paused;
                }
                if let Some(r) = &e.run {
                    r.cancel.cancel();
                }
            });
        }
        self.mark_dirty();
        self.emit_now();
        Ok(())
    }

    pub async fn resume(self: &Arc<Self>, id: &str) -> Result<()> {
        let rec = self.record(id)?;
        if rec.kind == Kind::Torrent {
            self.with_entry(id, |e| {
                let finished = e.rec.completed_at.is_some();
                if let Some(t) = e.rec.torrent.as_mut() {
                    t.user_paused = false;
                    t.queued = false;
                    if finished && t.seed_done {
                        t.seed_override = true;
                    }
                    t.seed_done = false;
                }
                if e.rec.status != Status::Completed {
                    e.rec.status = Status::Active;
                }
                e.rec.error = None;
            });
            match rec.torrent.as_ref().and_then(|t| t.info_hash.clone()) {
                Some(hash) => {
                    let engine = self.torrent_engine().await?;
                    if engine.handle(&hash).is_none() {
                        self.restore_torrents(&engine).await;
                    }
                    engine.resume(&hash).await?;
                }
                None => {
                    let this = self.clone();
                    let id2 = id.to_string();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = this.add_torrent_task(&id2).await {
                            this.fail(&id2, format!("{e:#}"));
                        }
                    });
                }
            }
        } else {
            self.with_entry(id, |e| {
                if matches!(e.rec.status, Status::Paused | Status::Failed) && e.run.is_none() {
                    e.rec.status = Status::Queued;
                    e.rec.error = None;
                    if e.rec.hash_state == HashState::Mismatch {
                        e.rec.hash_state = HashState::Pending;
                        e.rec.segments.clear();
                        e.rec.downloaded = 0;
                    }
                }
            });
            self.schedule();
        }
        self.mark_dirty();
        self.emit_now();
        Ok(())
    }

    pub async fn remove(self: &Arc<Self>, id: &str, delete_files: bool) -> Result<()> {
        let entry = {
            let mut entries = self.entries.lock();
            let idx = entries
                .iter()
                .position(|e| e.rec.id == id)
                .ok_or_else(|| anyhow!("download not found"))?;
            entries.remove(idx)
        };
        if let Some(run) = &entry.run {
            run.cancel.cancel();
        }
        let rec = entry.rec.clone();
        if rec.kind == Kind::Torrent {
            let mut engine_deleted = false;
            if let Some(hash) = rec.torrent.as_ref().and_then(|t| t.info_hash.clone()) {
                if let Ok(engine) = self.torrent_engine().await {
                    engine_deleted = engine.remove(&hash, delete_files).await.is_ok();
                }
                let _ = std::fs::remove_file(self.paths.torrents.join(format!("{hash}.torrent")));
            }
            if delete_files && !engine_deleted && !rec.name.trim().is_empty() {
                let this = self.clone();
                let path = PathBuf::from(&rec.dir).join(&rec.name);
                tauri::async_runtime::spawn(async move {
                    let failed = delete_paths(vec![path]).await;
                    this.report_undeleted(&failed);
                });
            }
        } else {
            let handle = entry.run.and_then(|r| r.handle);
            let cookies = rec.media.as_ref().and_then(|m| m.cookies_file.clone());
            let this = self.clone();
            tauri::async_runtime::spawn(async move {
                if let Some(h) = handle {
                    let _ = h.await;
                }
                if let Some(c) = cookies {
                    let _ = tokio::fs::remove_file(c).await;
                }
                let path = PathBuf::from(&rec.dir).join(&rec.name);
                let mut targets = vec![util::part_path(&path)];
                if let Some(m) = rec.media.as_ref() {
                    for p in &m.partials {
                        targets.extend(media_leftovers(Path::new(p)));
                    }
                    if delete_files {
                        targets.extend(m.files.iter().map(PathBuf::from));
                    }
                }
                if delete_files && (rec.status == Status::Completed || rec.status == Status::Failed)
                {
                    targets.push(path);
                }
                targets.sort();
                targets.dedup();
                let failed = delete_paths(targets).await;
                this.report_undeleted(&failed);
            });
        }
        self.mark_dirty();
        self.persist();
        self.schedule();
        self.emit_now();
        Ok(())
    }

    fn report_undeleted(&self, failed: &[PathBuf]) {
        if failed.is_empty() {
            return;
        }
        let names: Vec<String> = failed
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .take(3)
            .collect();
        self.toast(
            "warning",
            format!(
                "Could not delete {} file(s) from disk: {} — close any program using it and delete it by hand.",
                failed.len(),
                names.join(", ")
            ),
        );
    }

    pub async fn pause_all(self: &Arc<Self>) {
        let ids: Vec<String> = self
            .entries
            .lock()
            .iter()
            .filter(|e| matches!(e.rec.status, Status::Active | Status::Queued))
            .map(|e| e.rec.id.clone())
            .collect();
        for id in ids {
            let _ = self.pause(&id).await;
        }
    }

    pub async fn resume_all(self: &Arc<Self>) {
        let ids: Vec<String> = self
            .entries
            .lock()
            .iter()
            .filter(|e| e.rec.status == Status::Paused)
            .map(|e| e.rec.id.clone())
            .collect();
        for id in ids {
            let _ = self.resume(&id).await;
        }
    }

    pub fn clear_finished(&self) {
        self.entries.lock().retain(|e| {
            !(e.rec.status == Status::Completed && !(e.rec.kind == Kind::Torrent && e.live.seeding))
        });
        self.mark_dirty();
        self.persist();
        self.emit_now();
    }

    pub fn set_threads(self: &Arc<Self>, id: &str, threads: u32) -> Result<()> {
        let threads = threads.clamp(1, MAX_THREADS);
        let relaunch = self
            .with_entry(id, |e| {
                e.rec.threads = threads;
                if let Some(p) = e.run.as_ref().and_then(|r| r.progress.as_ref()) {
                    p.set_target(threads);
                }
                e.run
                    .as_ref()
                    .and_then(|r| r.media_attempt.as_ref())
                    .is_some_and(|(_, n)| *n != threads)
            })
            .ok_or_else(|| anyhow!("download not found"))?;
        if relaunch {
            let this = self.clone();
            let id = id.to_string();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(1500)).await;
                this.with_entry(&id, |e| {
                    let wanted = e.rec.threads;
                    if let Some((attempt, n)) =
                        e.run.as_ref().and_then(|r| r.media_attempt.as_ref())
                    {
                        let downloading = e.live.phase.as_deref() == Some("downloading");
                        if *n != wanted && wanted == threads && downloading {
                            attempt.cancel();
                        }
                    }
                });
            });
        }
        self.mark_dirty();
        Ok(())
    }

    pub fn set_speed_limit(&self, id: &str, kib: u64) -> Result<()> {
        self.with_entry(id, |e| {
            e.rec.speed_limit = kib;
            if let Some(r) = &e.run {
                r.limiter.set_rate(kib * 1024);
            }
        })
        .ok_or_else(|| anyhow!("download not found"))?;
        self.mark_dirty();
        Ok(())
    }

    pub fn file_path(&self, id: &str) -> Result<PathBuf> {
        let rec = self.record(id)?;
        if rec.kind == Kind::Media {
            if let Some(f) = rec.media.as_ref().and_then(|m| m.files.last()) {
                let f = PathBuf::from(f);
                if f.exists() {
                    return Ok(f);
                }
                if let Some(found) = automation::find_by_media_id(&f) {
                    return Ok(found);
                }
            }
            if let Some(found) =
                automation::find_by_media_id(&PathBuf::from(&rec.dir).join(&rec.name))
            {
                return Ok(found);
            }
        }
        let p = PathBuf::from(&rec.dir).join(&rec.name);
        if p.exists() {
            Ok(p)
        } else if util::part_path(&p).exists() {
            Ok(util::part_path(&p))
        } else {
            Ok(PathBuf::from(&rec.dir))
        }
    }

    pub async fn reload_ip_filter(self: &Arc<Self>, restart: bool) -> FilterStatus {
        let s = self.settings();
        let sources = vec![s.ip_filter_source.clone(), s.ip_filter_source_v6.clone()];
        let result = ipfilter::build(
            &self.api_client,
            &sources,
            &s.ip_filter_manual,
            &self.paths.ip_filter,
        )
        .await;
        let status = match result {
            Ok(rules) => FilterStatus {
                rules,
                loaded_at: Some(now_ms()),
                error: None,
            },
            Err(e) => FilterStatus {
                rules: self.filter_status.lock().rules,
                loaded_at: self.filter_status.lock().loaded_at,
                error: Some(format!("{e:#}")),
            },
        };
        *self.filter_status.lock() = status.clone();
        if restart && status.error.is_none() && s.ip_filter_enabled {
            self.restart_torrent_engine().await;
        }
        status
    }

    pub async fn background(self: Arc<Self>) {
        let s = self.settings();
        if s.ip_filter_enabled {
            let stale = std::fs::metadata(&self.paths.ip_filter)
                .and_then(|m| m.modified())
                .map(|t| {
                    t.elapsed().unwrap_or_default()
                        > Duration::from_secs(s.ip_filter_reload_days.max(1) as u64 * 86_400)
                })
                .unwrap_or(true);
            if stale {
                self.reload_ip_filter(false).await;
            } else if let Ok(text) = std::fs::read_to_string(&self.paths.ip_filter) {
                *self.filter_status.lock() = FilterStatus {
                    rules: text.lines().count(),
                    loaded_at: None,
                    error: None,
                };
            }
        }
        let has_torrents = self
            .entries
            .lock()
            .iter()
            .any(|e| e.rec.kind == Kind::Torrent);
        if has_torrents && s.torrent_enabled {
            if let Err(e) = self.torrent_engine().await {
                tracing::error!("torrent engine failed to start: {e:#}");
            }
        }
        if s.media_enabled {
            let this = self.clone();
            tauri::async_runtime::spawn(async move { this.keep_tools_updated().await });
        }
        self.schedule();

        let mut interval = tokio::time::interval(Duration::from_millis(500));
        let mut ticks: u64 = 0;
        loop {
            interval.tick().await;
            ticks += 1;
            self.tick();
            if let Some(engine) = self.torrent_if_running() {
                self.torrent_tick(&engine);
            }
            self.automation_tick(ticks);
            let stats = self.compute_global();
            let any_active = stats.active > 0 || stats.download_speed > 0 || stats.upload_speed > 0;
            if self.dirty.swap(false, Ordering::Relaxed) || any_active {
                self.emit_now();
            }
            let _ = self.app.emit(EVT_STATS, &stats);
            crate::tray::set_progress(&self.app, stats.progress);
            if ticks % 4 == 0 {
                crate::tray::set_tooltip(&self.app, &stats);
            }
            if ticks % 10 == 0 && self.save_needed.swap(false, Ordering::Relaxed) {
                self.persist();
            }
            if any_active && ticks % 10 == 0 {
                self.save_needed.store(true, Ordering::Relaxed);
            }
        }
    }

    fn tick(&self) {
        let mut entries = self.entries.lock();
        for e in entries.iter_mut() {
            let Some(run) = e.run.as_mut() else { continue };
            let Some(p) = run.progress.clone() else {
                continue;
            };
            let now = Instant::now();
            let dt = now.duration_since(run.last_tick).as_secs_f64().max(0.001);
            let received = p.received.load(Ordering::Relaxed);
            let inst = received.saturating_sub(run.last_received) as f64 / dt;
            run.ema = if run.ema == 0.0 {
                inst
            } else {
                run.ema * 0.6 + inst * 0.4
            };
            run.last_received = received;
            run.last_tick = now;
            run.peak = run.peak.max(run.progress_base + p.downloaded());
            e.rec.downloaded = run.peak;
            if let Some(t) = run.progress_total.or(p.total()) {
                e.rec.total = Some(t);
            }
            e.rec.segments = p.segments();
            e.live.speed = run.ema as u64;
            e.live.connections = p.connections.load(Ordering::Relaxed);
            e.live.eta = match (e.rec.total, e.live.speed) {
                (Some(t), s) if s > 0 => Some(t.saturating_sub(e.rec.downloaded) / s),
                _ => None,
            };
        }
    }

    fn compute_global(&self) -> GlobalStats {
        let torrent = self.torrent_if_running().map(|t| t.status());
        let (mut down, mut up, mut active, mut queued) = (0u64, 0u64, 0usize, 0usize);
        let (mut done_bytes, mut total_bytes, mut unknown) = (0u64, 0u64, false);
        for e in self.entries.lock().iter() {
            match e.rec.status {
                Status::Active => active += 1,
                Status::Queued => queued += 1,
                _ => {}
            }
            let downloading = e.rec.status == Status::Active && !e.live.seeding;
            if downloading {
                let (got, size) = if e.live.tool_name.is_some() {
                    (e.live.tool_downloaded, e.live.tool_total)
                } else {
                    (e.rec.downloaded, e.rec.total)
                };
                match size {
                    Some(t) if t > 0 => {
                        done_bytes += got.min(t);
                        total_bytes += t;
                    }
                    _ => unknown = true,
                }
            }
            if e.rec.kind != Kind::Torrent {
                down += e.live.speed;
            }
        }
        if let Some(t) = &torrent {
            down += t.download_speed;
            up += t.upload_speed;
        }
        let progress = if total_bytes > 0 {
            OverallProgress::Fraction(done_bytes as f64 / total_bytes as f64)
        } else if unknown {
            OverallProgress::Unknown
        } else {
            OverallProgress::Idle
        };
        let stats = GlobalStats {
            download_speed: down,
            upload_speed: up,
            active,
            queued,
            progress,
            torrent,
            alt_speed: self.alt_active(),
        };
        *self.last_global.lock() = stats.clone();
        stats
    }

    fn media_busy(&self) -> bool {
        self.entries
            .lock()
            .iter()
            .any(|e| e.rec.kind == Kind::Media && e.run.is_some())
    }

    async fn keep_tools_updated(self: Arc<Self>) {
        crate::selfupdate::wait_for_startup_check().await;
        loop {
            let mut retry_soon = false;
            if self.settings().media_enabled {
                if self.media_busy() {
                    retry_soon = true;
                } else {
                    self.clone().auto_update_ytdlp().await;
                    if self.media_busy() {
                        retry_soon = true;
                    } else {
                        let app = self.app.clone();
                        match self
                            .tools
                            .update_ffmpeg(move |p| {
                                let _ = app.emit(EVT_TOOLS, p);
                            })
                            .await
                        {
                            Ok(true) => tracing::info!("FFmpeg updated"),
                            Ok(false) => {}
                            Err(e) => {
                                tracing::info!("FFmpeg update skipped: {e:#}");
                                retry_soon = true;
                            }
                        }
                        let app = self.app.clone();
                        if !self.media_busy() {
                            match self
                                .tools
                                .update_deno(move |p| {
                                    let _ = app.emit(EVT_TOOLS, p);
                                })
                                .await
                            {
                                Ok(true) => tracing::info!("Deno updated"),
                                Ok(false) => {}
                                Err(e) => {
                                    tracing::info!("Deno update skipped: {e:#}");
                                    retry_soon = true;
                                }
                            }
                        }
                    }
                }
            }
            let wait = if retry_soon { 30 * 60 } else { 6 * 3600 };
            tokio::time::sleep(Duration::from_secs(wait)).await;
        }
    }

    async fn fetch_media_direct(
        self: Arc<Self>,
        id: &str,
        job: &MediaJob,
        cancel: &CancellationToken,
        scratch: &mut Vec<PathBuf>,
    ) -> Result<Option<PathBuf>> {
        self.with_entry(id, |e| e.live.phase = Some("metadata".into()));
        self.dirty.store(true, Ordering::Relaxed);
        let mut info = tokio::select! {
            _ = cancel.cancelled() => bail!("cancelled"),
            i = media::extract(job) => i?,
        };
        let Some(formats) = media::direct_formats(&info) else {
            return Ok(None);
        };
        let settings = self.settings();
        let client = self.http.read().clone();
        let limiter = self
            .with_entry(id, |e| e.run.as_ref().map(|r| r.limiter.clone()))
            .flatten()
            .ok_or_else(|| anyhow!("download not found"))?;
        let mut probed = Vec::new();
        for format in &formats {
            let url = format["url"].as_str().unwrap_or_default().to_string();
            let headers = media::format_headers(format);
            let probe = engine::probe(&client, &url, &headers).await?;
            probed.push((format, headers, probe));
        }
        let sizes: Option<Vec<u64>> = probed.iter().map(|(_, _, p)| p.total).collect();
        let overall = sizes.as_ref().map(|s| s.iter().sum::<u64>());
        let mut base = 0u64;
        let mut local = Vec::new();
        for (format, headers, probe) in probed {
            let format_id = format["format_id"].as_str().unwrap_or("0").to_string();
            let safe: String = format_id
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            let part = job.dir.join(format!(".{id}.f{safe}.mdmpart"));
            scratch.push(part.clone());
            if part.exists() {
                let _ = tokio::fs::remove_file(&part).await;
            }
            if let Some(total) = probe.total {
                let p = part.clone();
                tokio::task::spawn_blocking(move || engine::ensure_space(&p, total)).await??;
            }
            let threads = self.record(id).map(|r| r.threads).unwrap_or(1);
            let progress = Progress::new(probe.total, &[], threads);
            self.with_entry(id, |e| {
                e.rec.segments.clear();
                e.rec.downloaded = base;
                e.rec.total = overall.or(probe.total);
                e.live.phase = Some("downloading".into());
                if let Some(r) = e.run.as_mut() {
                    r.progress = Some(progress.clone());
                    r.progress_base = base;
                    r.progress_total = overall;
                    r.peak = base;
                    r.last_received = 0;
                    r.last_tick = Instant::now();
                }
            });
            let done = engine::run(Arc::new(engine::Job {
                client: client.clone(),
                url: probe.final_url.clone(),
                headers,
                part: part.clone(),
                resumable: probe.resumable,
                progress,
                cancel: cancel.clone(),
                limiters: vec![self.global_limit.clone(), limiter.clone()],
                retries: settings.retries,
                transform: None,
                if_range: probe.validator.clone(),
            }))
            .await?;
            if !done {
                bail!("cancelled");
            }
            base += probe.total.unwrap_or(0);
            local.push((format_id, part));
        }
        media::use_local_files(&mut info, &local);
        let info_path = job.dir.join(format!(".{id}.mdminfo.json"));
        scratch.push(info_path.clone());
        tokio::fs::write(&info_path, serde_json::to_vec(&info)?).await?;
        Ok(Some(info_path))
    }

    fn tool_progress(self: &Arc<Self>, id: &str, name: &'static str) -> impl Fn(ToolProgress) {
        let app = self.app.clone();
        let this = self.clone();
        let id = id.to_string();
        let last = Mutex::new((0u64, Instant::now()));
        move |p: ToolProgress| {
            let speed = {
                let mut l = last.lock();
                let secs = l.1.elapsed().as_secs_f64();
                if secs >= 0.2 {
                    let s = (p.downloaded.saturating_sub(l.0) as f64 / secs) as u64;
                    *l = (p.downloaded, Instant::now());
                    Some(s)
                } else {
                    None
                }
            };
            this.with_entry(&id, |e| {
                e.live.tool_name = Some(name.into());
                e.live.tool_downloaded = p.downloaded;
                e.live.tool_total = p.total;
                let step = if p.done { "extracting" } else { "installing" };
                e.live.phase = Some(format!("{step} {name}"));
                if p.done {
                    e.live.speed = 0;
                    e.live.eta = None;
                } else if let Some(s) = speed {
                    e.live.speed = s;
                    e.live.eta = p
                        .total
                        .filter(|_| s > 0)
                        .map(|t| t.saturating_sub(p.downloaded) / s);
                }
            });
            this.dirty.store(true, Ordering::Relaxed);
            let _ = app.emit(EVT_TOOLS, p);
        }
    }

    fn tool_finished(&self, id: &str) {
        self.with_entry(id, |e| {
            e.live.tool_name = None;
            e.live.tool_downloaded = 0;
            e.live.tool_total = None;
            e.live.speed = 0;
            e.live.eta = None;
            e.live.phase = Some("preparing".into());
        });
    }

    async fn auto_update_ytdlp(self: Arc<Self>) {
        let installed = self.tools.ytdlp_version().await;
        let latest = match self.tools.latest_ytdlp_version().await {
            Ok(v) => v,
            Err(e) => {
                tracing::info!("yt-dlp update check failed: {e:#}");
                return;
            }
        };
        if installed.as_deref() == Some(latest.as_str()) {
            return;
        }
        if self.media_busy() && installed.is_some() {
            return;
        }
        let app = self.app.clone();
        match self
            .tools
            .install_ytdlp(move |p| {
                let _ = app.emit(EVT_TOOLS, p);
            })
            .await
        {
            Ok(v) => tracing::info!("yt-dlp updated to {v}"),
            Err(e) => tracing::warn!("yt-dlp update failed: {e:#}"),
        }
    }

    pub async fn shutdown(&self) {
        let runs: Vec<CancellationToken> = self
            .entries
            .lock()
            .iter()
            .filter_map(|e| e.run.as_ref().map(|r| r.cancel.clone()))
            .collect();
        for c in &runs {
            c.cancel();
        }
        if !runs.is_empty() {
            tokio::time::sleep(Duration::from_millis(600)).await;
        }
        self.tick();
        self.persist();
        if let Some(engine) = self.torrent_if_running() {
            engine.stop().await;
        }
    }
}

fn is_waiting(e: &Entry) -> bool {
    e.rec.kind != Kind::Torrent && e.rec.status == Status::Queued && e.run.is_none()
}

fn blank_record(url: &str) -> DownloadRecord {
    DownloadRecord {
        id: String::new(),
        kind: Kind::Http,
        url: url.to_string(),
        name: String::new(),
        dir: String::new(),
        category: Category::Other,
        status: Status::Queued,
        total: None,
        downloaded: 0,
        threads: crate::settings::DEFAULT_THREADS,
        resumable: false,
        segments: Vec::new(),
        created_at: now_ms(),
        completed_at: None,
        error: None,
        referrer: None,
        user_agent: None,
        cookie_header: None,
        speed_limit: 0,
        scan_state: String::new(),
        scan_detail: None,
        cloud_state: String::new(),
        cloud_before: None,
        cloud_detail: None,
        queue_order: now_ms(),
        media: None,
        torrent: None,
        sha256_expected: None,
        sha256_actual: None,
        hash_state: HashState::None,
        validator: None,
    }
}

fn media_leftovers(dest: &Path) -> Vec<PathBuf> {
    let mut out = vec![dest.to_path_buf()];
    let name = match dest.file_name() {
        Some(n) => n.to_string_lossy().into_owned(),
        None => return out,
    };
    for suffix in [".part", ".ytdl", ".temp"] {
        let mut s = dest.as_os_str().to_owned();
        s.push(suffix);
        out.push(PathBuf::from(s));
    }
    if let Some(dir) = dest.parent() {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if n.starts_with(&format!("{name}.part-Frag"))
                    || n.starts_with(&format!("{name}.temp"))
                {
                    out.push(e.path());
                }
            }
        }
    }
    out
}

async fn delete_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut failed = Vec::new();
    for path in paths {
        let mut attempt = 0;
        loop {
            let result = match tokio::fs::symlink_metadata(&path).await {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
                Ok(m) if m.is_dir() => tokio::fs::remove_dir_all(&path).await,
                Ok(_) => tokio::fs::remove_file(&path).await,
            };
            match result {
                Ok(()) => break,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                Err(e) if attempt >= 12 => {
                    tracing::warn!("could not delete {}: {e}", path.display());
                    failed.push(path);
                    break;
                }
                Err(_) => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }
    failed
}

fn restrict_to_user(dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if dir.is_dir() {
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}

fn cookie_header(cookies: &[BrowserCookie], url: &str) -> Option<String> {
    let host = url::Url::parse(url).ok()?.host_str()?.to_ascii_lowercase();
    let parts: Vec<String> = cookies
        .iter()
        .filter(|c| {
            let d = c.domain.trim_start_matches('.').to_ascii_lowercase();
            host == d || host.ends_with(&format!(".{d}"))
        })
        .map(|c| format!("{}={}", c.name, c.value))
        .collect();
    (!parts.is_empty()).then(|| parts.join("; "))
}

pub async fn find_sha256(
    client: &reqwest::Client,
    url: &str,
    filename: Option<&str>,
) -> Option<(String, String)> {
    let parsed = url::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let mut base = parsed.clone();
    base.set_query(None);
    base.set_fragment(None);
    let file_url = base.to_string();
    let name = filename
        .map(str::to_string)
        .or_else(|| util::filename_from_url(url))?;
    let dir = base.join("./").ok()?;
    let mut candidates = vec![
        format!("{file_url}.sha256"),
        format!("{file_url}.sha256sum"),
        format!("{file_url}.sha256.txt"),
    ];
    for n in [
        "SHA256SUMS",
        "SHA256SUMS.txt",
        "sha256sums.txt",
        "sha256sum.txt",
        "checksums.txt",
    ] {
        if let Ok(u) = dir.join(n) {
            candidates.push(u.to_string());
        }
    }
    for c in candidates {
        let resp = match client.get(&c).timeout(Duration::from_secs(6)).send().await {
            Ok(r) if r.status().is_success() => r,
            _ => continue,
        };
        if resp.content_length().unwrap_or(0) > 2 * 1024 * 1024 {
            continue;
        }
        let Ok(text) = resp.text().await else {
            continue;
        };
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        let hashes: Vec<(&str, &str)> = lines
            .iter()
            .filter_map(|l| {
                let token = l.split_whitespace().find(|t| util::is_sha256_hex(t))?;
                Some((token, *l))
            })
            .collect();
        if let Some((h, _)) = hashes.iter().find(|(_, l)| {
            l.split_whitespace()
                .any(|t| t.trim_start_matches('*').trim_start_matches("./") == name)
        }) {
            return Some((h.to_ascii_lowercase(), c));
        }
        if hashes.len() == 1
            && (c.ends_with(".sha256") || c.ends_with(".sha256sum") || c.ends_with(".sha256.txt"))
        {
            return Some((hashes[0].0.to_ascii_lowercase(), c));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha_normalization() {
        let h = "A".repeat(64);
        assert_eq!(normalize_sha(Some(&h)).unwrap(), Some("a".repeat(64)));
        assert_eq!(
            normalize_sha(Some(&format!("{h}  file.iso"))).unwrap(),
            Some("a".repeat(64))
        );
        assert!(normalize_sha(Some("abc")).is_err());
        assert_eq!(normalize_sha(Some("  ")).unwrap(), None);
    }

    #[test]
    fn cookies_for_host() {
        let c = vec![
            BrowserCookie {
                name: "a".into(),
                value: "1".into(),
                domain: ".example.com".into(),
                path: "/".into(),
                secure: false,
                http_only: false,
                expiration_date: None,
            },
            BrowserCookie {
                name: "b".into(),
                value: "2".into(),
                domain: "other.org".into(),
                path: "/".into(),
                secure: false,
                http_only: false,
                expiration_date: None,
            },
        ];
        assert_eq!(
            cookie_header(&c, "https://dl.example.com/x").as_deref(),
            Some("a=1")
        );
    }

    #[test]
    fn overall_progress_percent() {
        assert_eq!(OverallProgress::Fraction(0.456).percent(), Some(45));
        assert_eq!(OverallProgress::Fraction(1.2).percent(), Some(100));
        assert_eq!(OverallProgress::Unknown.percent(), None);
        assert_eq!(OverallProgress::Idle.percent(), None);
    }

    #[tokio::test]
    async fn deletes_unfinished_media_leftovers() {
        let dir = std::env::temp_dir().join(format!("mdm-del-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("Clip [abc].f137.mp4");
        for name in [
            "Clip [abc].f137.mp4.part",
            "Clip [abc].f137.mp4.ytdl",
            "Clip [abc].f137.mp4.part-Frag12",
            "Clip [abc].f137.mp4.part-Frag13.part",
            "keep.mp4",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let failed = delete_paths(media_leftovers(&dest)).await;
        assert!(failed.is_empty());
        let left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec!["keep.mp4".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
