use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Local, Timelike};
use serde::Deserialize;

use super::*;
use crate::rss::{self, RssItem};
use crate::settings::parse_hhmm;

const RSS_KEEP_ITEMS: usize = 200;
const RSS_KEEP_DOWNLOADED: usize = 5000;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FeedState {
    pub title: Option<String>,
    pub items: Vec<RssItem>,
    pub error: Option<String>,
    pub updated: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
struct RssStore {
    feeds: HashMap<String, FeedState>,
    downloaded: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FeedView {
    pub url: String,
    pub name: String,
    pub enabled: bool,
    pub state: FeedState,
    pub downloaded: Vec<String>,
}

pub struct AutoState {
    applied: Option<(u64, u64, u64)>,
    when_done_armed: bool,
    rss_last: Option<Instant>,
    rss_busy: bool,
    rss: Option<RssStore>,
    pub stream_token: String,
}

impl Default for AutoState {
    fn default() -> Self {
        Self {
            applied: None,
            when_done_armed: false,
            rss_last: None,
            rss_busy: false,
            rss: None,
            stream_token: uuid::Uuid::new_v4().simple().to_string(),
        }
    }
}

pub fn in_schedule(s: &Settings, weekday: u8, minute: u32) -> bool {
    let (Some(from), Some(to)) = (
        parse_hhmm(&s.alt_schedule_from),
        parse_hhmm(&s.alt_schedule_to),
    ) else {
        return false;
    };
    let day_ok = |d: u8| s.alt_schedule_days.contains(&d);
    if from == to {
        return day_ok(weekday);
    }
    if from < to {
        day_ok(weekday) && minute >= from && minute < to
    } else if minute >= from {
        day_ok(weekday)
    } else {
        minute < to && day_ok((weekday + 6) % 7)
    }
}

fn shell(command: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").raw_arg(command);
        c.creation_flags(0x0800_0000);
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = std::process::Command::new("/bin/sh");
        c.arg("-c").arg(command);
        c
    }
}

fn system_action(action: &str) -> Result<()> {
    let mut cmd = match action {
        "sleep" if cfg!(windows) => shell("rundll32.exe powrprof.dll,SetSuspendState 0,1,0"),
        "sleep" if cfg!(target_os = "macos") => shell("pmset sleepnow"),
        "sleep" => shell("systemctl suspend"),
        "shutdown" if cfg!(windows) => {
            shell("shutdown /s /t 60 /c \"Multron Download Manager: downloads finished\"")
        }
        "shutdown" if cfg!(target_os = "macos") => {
            shell("osascript -e 'tell app \"System Events\" to shut down'")
        }
        "shutdown" => shell("systemctl poweroff"),
        other => bail!("unknown action {other}"),
    };
    cmd.spawn()?;
    Ok(())
}

impl Manager {
    pub fn alt_active(&self) -> bool {
        let s = self.settings.read();
        if s.alt_speed_enabled {
            return true;
        }
        if !s.alt_schedule_enabled {
            return false;
        }
        let now = Local::now();
        in_schedule(
            &s,
            now.weekday().num_days_from_monday() as u8,
            now.hour() * 60 + now.minute(),
        )
    }

    pub fn stream_token(&self) -> String {
        self.auto.lock().stream_token.clone()
    }

    pub(super) fn reset_applied_limits(&self) {
        self.auto.lock().applied = None;
    }

    pub(super) fn automation_tick(self: &Arc<Self>, ticks: u64) {
        if ticks % 2 == 0 {
            self.apply_effective_limits();
            if let Some(engine) = self.torrent_if_running() {
                self.torrent_queue(&engine);
                self.seeding_limits(&engine);
            }
            self.when_done_check();
        }
        if ticks % 20 == 0 {
            self.watch_folder_scan();
        }
        if ticks % 20 == 10 {
            let s = self.settings();
            let due = {
                let st = self.auto.lock();
                !st.rss_busy
                    && st
                        .rss_last
                        .map(|t| t.elapsed() >= Duration::from_secs(s.rss_interval as u64 * 60))
                        .unwrap_or(true)
            };
            if s.rss_enabled && due && s.rss_feeds.iter().any(|f| f.enabled) {
                let this = self.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = this.refresh_rss().await;
                });
            }
        }
    }

    fn apply_effective_limits(&self) {
        let s = self.settings();
        let alt = self.alt_active();
        let limits = if alt {
            (
                s.alt_download_limit,
                s.alt_upload_limit,
                s.alt_download_limit,
            )
        } else {
            (
                s.torrent_download_limit,
                s.torrent_upload_limit,
                s.speed_limit,
            )
        };
        let changed = {
            let mut st = self.auto.lock();
            let changed = st.applied != Some(limits);
            st.applied = Some(limits);
            changed
        };
        if !changed {
            return;
        }
        self.global_limit.set_rate(limits.2 * 1024);
        if let Some(engine) = self.torrent_if_running() {
            engine.apply_limits(limits.0, limits.1);
        }
        let _ = self.app.emit(EVT_STATS, &self.compute_global());
    }

    fn torrent_queue(self: &Arc<Self>, engine: &Arc<TorrentEngine>) {
        let s = self.settings();
        let mut to_pause = Vec::new();
        let mut to_resume = Vec::new();
        {
            let mut entries = self.entries.lock();
            let mut downloading: Vec<usize> = Vec::new();
            let mut seeding: Vec<usize> = Vec::new();
            for (i, e) in entries.iter().enumerate() {
                let Some(t) = e.rec.torrent.as_ref() else {
                    continue;
                };
                if e.rec.kind != Kind::Torrent
                    || t.info_hash.is_none()
                    || t.user_paused
                    || t.seed_done
                {
                    continue;
                }
                if e.rec.status == Status::Failed {
                    continue;
                }
                if e.rec.completed_at.is_none() {
                    downloading.push(i);
                } else if s.seed_after_complete {
                    seeding.push(i);
                }
            }
            for (set, max) in [
                (&mut downloading, s.torrent_max_active_downloads),
                (&mut seeding, s.torrent_max_active_seeds),
            ] {
                set.sort_by_key(|i| entries[*i].rec.queue_order);
                for (pos, idx) in set.iter().enumerate() {
                    let allowed = max == 0 || pos < max as usize;
                    let e = &mut entries[*idx];
                    let finished = e.rec.completed_at.is_some();
                    let Some(t) = e.rec.torrent.as_mut() else {
                        continue;
                    };
                    let hash = t.info_hash.clone().unwrap_or_default();
                    if allowed && t.queued {
                        t.queued = false;
                        if !finished {
                            e.rec.status = Status::Active;
                        }
                        to_resume.push(hash);
                    } else if !allowed && !t.queued {
                        t.queued = true;
                        if !finished {
                            e.rec.status = Status::Queued;
                        }
                        to_pause.push(hash);
                    }
                }
            }
        }
        if to_pause.is_empty() && to_resume.is_empty() {
            return;
        }
        self.mark_dirty();
        let engine = engine.clone();
        tauri::async_runtime::spawn(async move {
            for h in to_pause {
                let _ = engine.pause(&h).await;
            }
            for h in to_resume {
                let _ = engine.resume(&h).await;
            }
        });
    }

    fn seeding_limits(self: &Arc<Self>, engine: &Arc<TorrentEngine>) {
        let s = self.settings();
        if s.seed_ratio <= 0.0 && s.seed_time_limit == 0 {
            return;
        }
        let now = now_ms();
        let mut hits: Vec<(String, String, String)> = Vec::new();
        {
            let mut entries = self.entries.lock();
            for e in entries.iter_mut() {
                let (Some(done_at), Some(total)) = (e.rec.completed_at, e.rec.total) else {
                    continue;
                };
                let name = e.rec.name.clone();
                let id = e.rec.id.clone();
                let Some(t) = e.rec.torrent.as_mut() else {
                    continue;
                };
                if t.seed_done || t.user_paused || t.seed_override || t.info_hash.is_none() {
                    continue;
                }
                let ratio_hit = s.seed_ratio > 0.0
                    && total > 0
                    && t.uploaded as f64 >= total as f64 * s.seed_ratio;
                let time_hit =
                    s.seed_time_limit > 0 && now - done_at >= s.seed_time_limit as i64 * 60_000;
                if ratio_hit || time_hit {
                    t.seed_done = true;
                    hits.push((id, t.info_hash.clone().unwrap_or_default(), name));
                }
            }
        }
        if hits.is_empty() {
            return;
        }
        self.mark_dirty();
        let this = self.clone();
        let engine = engine.clone();
        let action = s.seed_limit_action.clone();
        tauri::async_runtime::spawn(async move {
            for (id, hash, name) in hits {
                match action.as_str() {
                    "remove" => {
                        let _ = this.remove(&id, false).await;
                    }
                    "delete" => {
                        let _ = this.remove(&id, true).await;
                    }
                    _ => {
                        let _ = engine.pause(&hash).await;
                    }
                }
                tracing::info!("seeding limit reached for {name}");
            }
        });
    }

    fn watch_folder_scan(self: &Arc<Self>) {
        let s = self.settings();
        if !s.watch_folder_enabled || s.watch_folder.trim().is_empty() {
            return;
        }
        let Ok(dir) = std::fs::read_dir(s.watch_folder.trim()) else {
            return;
        };
        for entry in dir.flatten() {
            let path = entry.path();
            let is_torrent = path
                .extension()
                .map(|x| x.eq_ignore_ascii_case("torrent"))
                .unwrap_or(false);
            if !is_torrent || !path.is_file() {
                continue;
            }
            let settled = entry
                .metadata()
                .and_then(|m| m.modified())
                .map(|t| t.elapsed().unwrap_or_default() > Duration::from_secs(2))
                .unwrap_or(false);
            if !settled {
                continue;
            }
            std::fs::create_dir_all(&self.paths.torrents).ok();
            let copy = self
                .paths
                .torrents
                .join(format!("watch-{}.torrent", uuid::Uuid::new_v4().simple()));
            if std::fs::copy(&path, &copy).is_err() {
                continue;
            }
            let mut done = path.clone().into_os_string();
            done.push(".added");
            if std::fs::rename(&path, &done).is_err() {
                let _ = std::fs::remove_file(&copy);
                continue;
            }
            let this = self.clone();
            let name = path
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            tauri::async_runtime::spawn(async move {
                let req = AddRequest {
                    kind: Some("torrent".into()),
                    torrent_file: Some(copy.to_string_lossy().into_owned()),
                    page_title: Some(name),
                    ..Default::default()
                };
                if let Err(e) = this.add(req).await {
                    this.toast("error", format!("Watch folder: {e:#}"));
                }
            });
        }
    }

    fn when_done_check(self: &Arc<Self>) {
        let s = self.settings();
        if s.when_done == "nothing" {
            self.auto.lock().when_done_armed = false;
            return;
        }
        let busy = self.entries.lock().iter().any(|e| {
            matches!(e.rec.status, Status::Active | Status::Queued)
                && !(e.rec.kind == Kind::Torrent && e.rec.completed_at.is_some())
        });
        {
            let mut st = self.auto.lock();
            if busy {
                st.when_done_armed = true;
                return;
            }
            if !st.when_done_armed {
                return;
            }
            st.when_done_armed = false;
        }
        let action = s.when_done.clone();
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut next = this.settings();
            next.when_done = "nothing".into();
            let _ = this.save_settings(next).await;
            this.persist();
            match action.as_str() {
                "quit" => this.app.exit(0),
                other => {
                    if let Err(e) = system_action(other) {
                        this.toast("error", format!("{e:#}"));
                    }
                }
            }
        });
    }
}

pub(super) fn downloaded_files(rec: &DownloadRecord) -> Vec<PathBuf> {
    const LIMIT: usize = 2000;
    let mut out: Vec<PathBuf> = Vec::new();
    let add = |p: PathBuf, out: &mut Vec<PathBuf>| collect_files(&p, out, LIMIT);
    let base = PathBuf::from(&rec.dir);
    let top = base.join(&rec.name);
    match rec.kind {
        Kind::Media => {
            let listed: Vec<PathBuf> = rec
                .media
                .as_ref()
                .map(|m| m.files.iter().map(PathBuf::from).collect())
                .unwrap_or_default();
            for f in &listed {
                if f.exists() {
                    add(f.clone(), &mut out);
                } else if let Some(found) = find_by_media_id(f) {
                    add(found, &mut out);
                }
            }
            if out.is_empty() {
                if let Some(found) = find_by_media_id(&top) {
                    add(found, &mut out);
                } else {
                    add(top, &mut out);
                }
            }
        }
        Kind::Torrent => {
            if top.is_dir() {
                add(top.clone(), &mut out);
            } else {
                for f in rec
                    .torrent
                    .as_ref()
                    .map(|t| t.files.clone())
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|f| f.included)
                {
                    let direct = base.join(&f.name);
                    let nested = top.join(&f.name);
                    add(if direct.is_file() { direct } else { nested }, &mut out);
                }
                if out.is_empty() {
                    add(top, &mut out);
                }
            }
        }
        Kind::Http => add(top, &mut out),
    }
    out.sort();
    out.dedup();
    out
}

pub(super) fn media_id(name: &str) -> Option<&str> {
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    let open = stem.rfind('[')?;
    let id = stem[open + 1..].strip_suffix(']')?;
    (id.len() >= 6
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    .then_some(id)
}

pub(super) fn find_by_media_id(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_string_lossy().into_owned();
    let id = media_id(&name)?;
    let tag = format!("[{id}]");
    let dir = path.parent()?;
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            let n = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            n.contains(&tag)
                && !n.ends_with(".part")
                && !n.ends_with(".ytdl")
                && !n.contains(".part-Frag")
        })
        .max_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
}

fn collect_files(path: &Path, out: &mut Vec<PathBuf>, limit: usize) {
    if out.len() >= limit {
        return;
    }
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return;
    };
    if meta.is_file() {
        out.push(path.to_path_buf());
    } else if meta.is_dir() {
        let Ok(rd) = std::fs::read_dir(path) else {
            return;
        };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for e in entries {
            collect_files(&e, out, limit);
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudFinding {
    pub download: String,
    pub path: String,
    pub name: String,
    pub verdict: String,
    pub detail: String,
    pub route: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudReport {
    pub id: String,
    pub name: String,
    pub running: bool,
    pub stopped: bool,
    pub total: usize,
    pub done: usize,
    pub current: Option<String>,
    pub clean: usize,
    pub threats: usize,
    pub suspicious: usize,
    pub unknown: usize,
    pub not_checked: usize,
    pub findings: Vec<CloudFinding>,
    pub summary: Option<String>,
    pub state: String,
    pub ids: Vec<String>,
    pub stage: String,
    pub stage_sent: u64,
    pub stage_total: u64,
    pub started_ms: i64,
    pub finished_ms: i64,
    pub known: usize,
    pub uploaded: usize,
    pub uploaded_bytes: u64,
    pub recent: Vec<CloudFinding>,
}

const RECENT_FILES: usize = 300;

pub const ALL_REPORT: &str = "all";
const MAX_ALL_FILES: usize = 5000;

impl CloudReport {
    pub fn add(
        &mut self,
        download: &str,
        shown: String,
        path: &Path,
        v: &crate::cloudscan::FileVerdict,
    ) {
        self.done += 1;
        self.stage.clear();
        self.stage_sent = 0;
        self.stage_total = 0;
        match v.route {
            "hash" => self.known += 1,
            "upload" => {
                self.uploaded += 1;
                self.uploaded_bytes += v.uploaded;
            }
            _ => {}
        }
        let row = CloudFinding {
            download: download.to_string(),
            path: path.to_string_lossy().into_owned(),
            name: shown.clone(),
            verdict: v.verdict.clone(),
            detail: v.detail.clone(),
            route: v.route.to_string(),
            size: v.size,
        };
        self.recent.push(row.clone());
        if self.recent.len() > RECENT_FILES {
            self.recent.remove(0);
        }
        match v.verdict.as_str() {
            "clean" | "possible_clean" => self.clean += 1,
            "unknown" => self.unknown += 1,
            "malicious" => self.threats += 1,
            "suspicious" => self.suspicious += 1,
            _ => self.not_checked += 1,
        }
        if !matches!(v.verdict.as_str(), "clean" | "possible_clean" | "unknown") {
            self.findings.push(row);
        }
    }

    pub fn stage(&mut self, stage: crate::cloudscan::Stage) {
        use crate::cloudscan::Stage;
        let (name, sent, total) = match stage {
            Stage::Hashing { done, total } => ("hashing", done as u64, total as u64),
            Stage::Checking { count } => ("checking", 0, count as u64),
            Stage::Waiting { count } => ("waiting", 0, count as u64),
            Stage::Uploading { sent, total } => ("uploading", sent, total),
            Stage::Analyzing => ("analyzing", 0, 0),
            Stage::Reconnecting => ("reconnecting", 0, 0),
        };
        self.stage = name.into();
        self.stage_sent = sent;
        self.stage_total = total;
    }
}

fn all_name(download: &str, base: &Path, path: &Path) -> String {
    let inner = shown_name(base, path, false);
    if inner == download || inner.is_empty() {
        download.to_string()
    } else {
        let rest = Path::new(&inner)
            .strip_prefix(download)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or(inner);
        format!("{download} › {rest}")
    }
}

fn shown_name(base: &Path, path: &Path, unpacked: bool) -> String {
    if unpacked {
        return path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

pub(super) fn matches_extension(path: &Path, wanted: &[String]) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    wanted
        .iter()
        .any(|ext| ext == crate::settings::ALL_FILES || name.ends_with(&format!(".{ext}")))
}

impl Manager {
    pub(super) fn after_download(self: &Arc<Self>, rec: DownloadRecord) {
        let s = self.settings();
        if s.cloud_scan && s.cloud_scan_accepted {
            let files: Vec<PathBuf> = downloaded_files(&rec)
                .into_iter()
                .filter(|f| matches_extension(f, &s.cloud_scan_extensions))
                .collect();
            self.cloud_scan(&rec, files, s.cloud_scan_limit_mb, None);
        }
        if cfg!(all(unix, not(target_os = "macos"))) || !s.mark_downloads {
            return;
        }
        let files = downloaded_files(&rec);
        if files.is_empty() {
            return;
        }
        let this = self.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if crate::security::smartscreen_status() != "on" {
                return;
            }
            let mut marked = 0;
            for f in &files {
                if crate::security::mark_from_internet(f, &rec.url, rec.referrer.as_deref()) {
                    marked += 1;
                }
            }
            if marked == 0 {
                return;
            }
            this.with_entry(&rec.id, |e| {
                if e.rec.scan_state != "threat" {
                    e.rec.scan_state = "marked".into();
                    e.rec.scan_detail = None;
                }
            });
            this.mark_dirty();
            this.save_needed.store(true, Ordering::Relaxed);
        });
    }

    fn emit_cloud(&self, id: &str) {
        use tauri::Emitter;
        if let Some(r) = self.cloud_reports.lock().get(id).cloned() {
            let _ = self.app.emit(super::EVT_CLOUD, r);
        }
    }

    pub fn cloud_report(&self, id: &str) -> Option<CloudReport> {
        self.cloud_reports.lock().get(id).cloned()
    }

    pub fn stop_cloud_scan(&self, id: &str) {
        if let Some(stop) = self.cloud_stops.lock().get(id) {
            stop.store(true, Ordering::Relaxed);
        }
    }

    pub fn reveal_path_of(&self, id: &str, path: &str) -> Result<PathBuf> {
        let rec = self.record(id)?;
        let base = std::fs::canonicalize(&rec.dir).context("the download folder was not found")?;
        let target = std::fs::canonicalize(path).context("the file was not found")?;
        if !target.starts_with(&base) {
            bail!("the file is not inside the download folder");
        }
        Ok(target)
    }

    fn cloud_scanning(&self) -> HashSet<String> {
        let stops = self.cloud_stops.lock();
        let mut ids: HashSet<String> = stops.keys().cloned().collect();
        if stops.contains_key(ALL_REPORT) {
            if let Some(r) = self.cloud_reports.lock().get(ALL_REPORT) {
                ids.extend(r.ids.iter().cloned());
            }
        }
        ids
    }

    fn cloud_options(&self) -> crate::cloudscan::Options {
        let s = self.settings();
        crate::cloudscan::Options {
            compress: s.cloud_compress_uploads,
            connections: s.cloud_connections as usize,
            cache: s
                .cloud_fingerprint_cache
                .then(|| self.paths.data.join("cloud-fingerprints.json")),
        }
    }

    pub fn scan_all_in_cloud(self: &Arc<Self>) -> Result<usize> {
        let s = self.settings();
        if !s.cloud_scan_accepted {
            bail!("accept the Cloud Malware Scan License Agreement first");
        }
        if self.cloud_stops.lock().contains_key(ALL_REPORT) {
            bail!("Multron Cloud is already scanning all downloads");
        }
        let busy = self.cloud_scanning();
        let records: Vec<DownloadRecord> = self
            .entries
            .lock()
            .iter()
            .filter(|e| e.rec.status == Status::Completed && !busy.contains(&e.rec.id))
            .map(|e| e.rec.clone())
            .collect();
        let mut seen = HashSet::new();
        let mut jobs: Vec<(PathBuf, String, String, PathBuf)> = Vec::new();
        let mut ids: Vec<String> = Vec::new();
        for rec in &records {
            if jobs.len() >= MAX_ALL_FILES {
                break;
            }
            let mut added = false;
            for f in downloaded_files(rec) {
                if jobs.len() >= MAX_ALL_FILES {
                    break;
                }
                if seen.insert(f.clone()) {
                    jobs.push((f, rec.id.clone(), rec.name.clone(), PathBuf::from(&rec.dir)));
                    added = true;
                }
            }
            if added {
                ids.push(rec.id.clone());
            }
        }
        if jobs.is_empty() {
            bail!("there are no finished downloads to scan");
        }
        let limit_mb = s.cloud_scan_manual_limit_mb;
        let stop = Arc::new(AtomicBool::new(false));
        self.cloud_stops
            .lock()
            .insert(ALL_REPORT.into(), stop.clone());
        self.cloud_reports.lock().insert(
            ALL_REPORT.into(),
            CloudReport {
                id: ALL_REPORT.into(),
                name: "All downloads".into(),
                running: true,
                total: jobs.len(),
                current: jobs.first().map(|(p, _, n, b)| all_name(n, b, p)),
                state: "scanning".into(),
                ids: ids.clone(),
                started_ms: now_ms(),
                ..Default::default()
            },
        );
        for id in &ids {
            self.with_entry(id, |e| {
                e.rec.cloud_state = "scanning".into();
                e.rec.cloud_detail = None;
            });
        }
        self.emit_cloud(ALL_REPORT);
        self.mark_dirty();
        let count = jobs.len();
        let this = self.clone();
        let options = self.cloud_options();
        tauri::async_runtime::spawn(async move {
            let files: Vec<PathBuf> = jobs.iter().map(|j| j.0.clone()).collect();
            let mut per: HashMap<String, Vec<(PathBuf, crate::cloudscan::FileVerdict)>> =
                HashMap::new();
            let summary = crate::cloudscan::scan(
                &files,
                limit_mb,
                &options,
                &stop,
                |index, stage| {
                    if let Some(r) = this.cloud_reports.lock().get_mut(ALL_REPORT) {
                        r.stage(stage);
                        r.current = jobs.get(index).map(|(p, _, n, b)| all_name(n, b, p));
                    }
                    this.emit_cloud(ALL_REPORT);
                },
                |index, verdict| {
                    if let Some((path, id, name, base)) = jobs.get(index) {
                        per.entry(id.clone())
                            .or_default()
                            .push((path.clone(), verdict.clone()));
                        if let Some(r) = this.cloud_reports.lock().get_mut(ALL_REPORT) {
                            r.add(id, all_name(name, base, path), path, verdict);
                        }
                    }
                    this.emit_cloud(ALL_REPORT);
                },
            )
            .await;
            let stopped = stop.load(Ordering::Relaxed);
            this.cloud_stops.lock().remove(ALL_REPORT);
            for id in &ids {
                let result = per
                    .get(id)
                    .map(|list| crate::cloudscan::summarize(list, limit_mb));
                this.with_entry(id, |e| match &result {
                    Some(r) => {
                        e.rec.cloud_state = r.state.into();
                        e.rec.cloud_detail = Some(r.detail.clone());
                    }
                    None => {
                        e.rec.cloud_state = if summary.state == "error" {
                            "error".into()
                        } else {
                            String::new()
                        };
                        e.rec.cloud_detail =
                            (summary.state == "error").then(|| summary.detail.clone());
                    }
                });
            }
            let done = this
                .cloud_reports
                .lock()
                .get(ALL_REPORT)
                .map(|r| r.done)
                .unwrap_or(0);
            let mut detail = summary.detail.clone();
            if stopped {
                detail = format!("{detail} (stopped after {done} of {} files)", files.len());
            }
            if let Some(r) = this.cloud_reports.lock().get_mut(ALL_REPORT) {
                r.running = false;
                r.finished_ms = now_ms();
                r.stopped = stopped;
                r.current = None;
                r.summary = Some(detail.clone());
                r.state = summary.state.to_string();
            }
            this.emit_cloud(ALL_REPORT);
            match summary.state {
                "threat" | "suspicious" => this.notify(None, "threat", &detail),
                "clean" | "unknown" => this.notify(None, "scan_cloud", &this.word("no_threat")),
                _ => this.notify(None, "scan_cloud", &detail),
            }
            this.mark_dirty();
            this.save_needed.store(true, Ordering::Relaxed);
        });
        Ok(count)
    }

    pub fn scan_in_cloud(self: &Arc<Self>, id: &str) -> Result<()> {
        let s = self.settings();
        if !s.cloud_scan_accepted {
            bail!("accept the Cloud Malware Scan License Agreement first");
        }
        let rec = self.record(id)?;
        if rec.status != Status::Completed {
            bail!("the download is not finished yet");
        }
        if self.cloud_scanning().contains(&rec.id) {
            bail!("Multron Cloud is already scanning this download");
        }
        let files = downloaded_files(&rec);
        if files.is_empty() {
            bail!("the downloaded files were not found");
        }
        self.cloud_scan(&rec, files, s.cloud_scan_manual_limit_mb, None);
        Ok(())
    }

    pub async fn scan_locked_in_cloud(self: &Arc<Self>, id: &str, password: String) -> Result<()> {
        let s = self.settings();
        if !s.cloud_scan_accepted {
            bail!("accept the Cloud Malware Scan License Agreement first");
        }
        let rec = self.record(id)?;
        if self.cloud_scanning().contains(&rec.id) {
            bail!("Multron Cloud is already scanning this download");
        }
        let archives: Vec<PathBuf> = downloaded_files(&rec)
            .into_iter()
            .filter(|f| {
                matches!(
                    crate::archive::locked(f),
                    Some(crate::archive::Kind::Zip | crate::archive::Kind::SevenZip)
                )
            })
            .collect();
        if archives.is_empty() {
            bail!("no password-protected ZIP or 7z archive was found in this download");
        }
        let work = self
            .paths
            .data
            .join("cloud-scan")
            .join(uuid::Uuid::new_v4().to_string());
        let dir = work.clone();
        let extracted = tauri::async_runtime::spawn_blocking(move || {
            let mut out = crate::archive::Extracted::default();
            for (i, archive) in archives.iter().enumerate() {
                let part = crate::archive::extract(archive, &password, &dir.join(i.to_string()))?;
                out.files.extend(part.files);
                out.skipped += part.skipped;
            }
            Ok::<_, anyhow::Error>(out)
        })
        .await
        .map_err(|e| anyhow!("{e}"))?;
        let extracted = match extracted {
            Ok(x) => x,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&work);
                if crate::archive::is_wrong_password(&e) {
                    bail!("the password is wrong");
                }
                return Err(e);
            }
        };
        if extracted.files.is_empty() {
            let _ = std::fs::remove_dir_all(&work);
            bail!("the archive has no files that can be scanned");
        }
        self.cloud_scan(
            &rec,
            extracted.files,
            s.cloud_scan_manual_limit_mb,
            Some((work, extracted.skipped)),
        );
        Ok(())
    }

    fn cloud_scan(
        self: &Arc<Self>,
        rec: &DownloadRecord,
        files: Vec<PathBuf>,
        limit_mb: u32,
        unpacked: Option<(PathBuf, usize)>,
    ) {
        if files.is_empty() {
            return;
        }
        let id = rec.id.clone();
        let name = rec.name.clone();
        let base = PathBuf::from(&rec.dir);
        let stop = Arc::new(AtomicBool::new(false));
        self.cloud_stops.lock().insert(id.clone(), stop.clone());
        self.cloud_reports.lock().insert(
            id.clone(),
            CloudReport {
                id: id.clone(),
                name: name.clone(),
                running: true,
                total: files.len(),
                state: "scanning".into(),
                started_ms: now_ms(),
                ..Default::default()
            },
        );
        self.emit_cloud(&id);
        self.with_entry(&id, |e| {
            e.rec.cloud_state = "scanning".into();
            e.rec.cloud_detail = None;
        });
        self.mark_dirty();
        let this = self.clone();
        let options = self.cloud_options();
        tauri::async_runtime::spawn(async move {
            let is_unpacked = unpacked.is_some();
            let mut summary = crate::cloudscan::scan(
                &files,
                limit_mb,
                &options,
                &stop,
                |index, stage| {
                    if let Some(r) = this.cloud_reports.lock().get_mut(&id) {
                        r.stage(stage);
                        r.current = files.get(index).map(|p| shown_name(&base, p, is_unpacked));
                    }
                    this.emit_cloud(&id);
                },
                |index, verdict| {
                    if let (Some(path), Some(r)) =
                        (files.get(index), this.cloud_reports.lock().get_mut(&id))
                    {
                        r.add(&id, shown_name(&base, path, is_unpacked), path, verdict);
                    }
                    this.emit_cloud(&id);
                },
            )
            .await;
            let stopped = stop.load(Ordering::Relaxed);
            this.cloud_stops.lock().remove(&id);
            let done = this
                .cloud_reports
                .lock()
                .get(&id)
                .map(|r| r.done)
                .unwrap_or(0);
            if stopped {
                summary.detail = format!(
                    "{} (stopped after {done} of {} files)",
                    summary.detail,
                    files.len()
                );
                if done == 0 {
                    summary.state = "skipped";
                }
            }
            if let Some(r) = this.cloud_reports.lock().get_mut(&id) {
                r.running = false;
                r.finished_ms = now_ms();
                r.stopped = stopped;
                r.current = None;
                r.summary = Some(summary.detail.clone());
                r.state = summary.state.to_string();
            }
            this.emit_cloud(&id);
            if let Some((work, skipped)) = unpacked {
                let _ = std::fs::remove_dir_all(&work);
                summary.detail = if skipped > 0 {
                    format!(
                        "{} (inside the archive; {skipped} files over 100 MB were not checked)",
                        summary.detail
                    )
                } else {
                    format!("{} (inside the archive)", summary.detail)
                };
            }
            this.with_entry(&id, |e| {
                e.rec.cloud_state = summary.state.into();
                e.rec.cloud_detail = Some(summary.detail.clone());
            });
            match summary.state {
                "threat" | "suspicious" => {
                    this.notify(Some(&id), "threat", &format!("{name}: {}", summary.detail));
                    this.toast("error", format!("{name}: {}", summary.detail));
                }
                "clean" | "unknown" => {
                    this.notify(
                        Some(&id),
                        "scan_cloud",
                        &format!("{name}: {}", this.word("no_threat")),
                    );
                }
                "locked" => {}
                _ => {
                    this.notify(
                        Some(&id),
                        "scan_cloud",
                        &format!("{name}: {}", summary.detail),
                    );
                }
            }
            this.mark_dirty();
            this.save_needed.store(true, Ordering::Relaxed);
        });
    }

    pub(super) fn run_on_complete(&self, rec: &DownloadRecord) {
        let command = self.settings.read().on_complete_command.trim().to_string();
        if command.is_empty() {
            return;
        }
        let file = match rec.media.as_ref().and_then(|m| m.files.first()) {
            Some(f) => PathBuf::from(f),
            None => PathBuf::from(&rec.dir).join(&rec.name),
        };
        let quote = |p: &str| format!("\"{}\"", p.replace('"', ""));
        let line = command
            .replace("{file}", &quote(&file.to_string_lossy()))
            .replace("{dir}", &quote(&rec.dir))
            .replace("{name}", &quote(&rec.name))
            .replace("{url}", &quote(&rec.url));
        if let Err(e) = shell(&line).spawn() {
            tracing::warn!("on-complete command failed: {e}");
        }
    }

    pub async fn torrent_recheck(self: &Arc<Self>, id: &str) -> Result<()> {
        let rec = self.record(id)?;
        let t = rec
            .torrent
            .clone()
            .ok_or_else(|| anyhow!("not a torrent"))?;
        let hash = t
            .info_hash
            .clone()
            .ok_or_else(|| anyhow!("metadata is still being fetched"))?;
        let engine = self.torrent_engine().await?;
        let file = self.paths.torrents.join(format!("{hash}.torrent"));
        engine.remove(&hash, false).await?;
        self.with_entry(id, |e| {
            e.live.phase = Some("checking".into());
            e.rec.status = Status::Active;
            e.rec.completed_at = None;
            if let Some(t) = e.rec.torrent.as_mut() {
                t.user_paused = false;
                t.queued = false;
                t.seed_done = false;
            }
        });
        engine
            .restore(
                Some(&file),
                &t.source,
                Path::new(&rec.dir),
                t.only_files.clone(),
                false,
            )
            .await?;
        self.mark_dirty();
        self.emit_now();
        Ok(())
    }

    pub fn torrent_hash(&self, id: &str) -> Result<String> {
        self.record(id)?
            .torrent
            .and_then(|t| t.info_hash)
            .ok_or_else(|| anyhow!("metadata is still being fetched"))
    }

    pub fn torrent_file_path(&self, id: &str) -> Result<PathBuf> {
        let rec = self.record(id)?;
        let hash = rec
            .torrent
            .and_then(|t| t.info_hash)
            .ok_or_else(|| anyhow!("metadata is still being fetched"))?;
        let file = self.paths.torrents.join(format!("{hash}.torrent"));
        if !file.is_file() {
            bail!("the .torrent file is not available");
        }
        Ok(file)
    }

    pub async fn create_torrent(
        self: &Arc<Self>,
        source: PathBuf,
        trackers: Vec<String>,
        piece_kib: Option<u32>,
        save_to: PathBuf,
        seed: bool,
    ) -> Result<String> {
        if !source.exists() {
            bail!("{} does not exist", source.display());
        }
        let created = crate::torrent::create_torrent_file(&source, trackers, piece_kib).await?;
        if let Some(parent) = save_to.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&save_to, &created.bytes)
            .with_context(|| format!("could not write {}", save_to.display()))?;
        if seed {
            let dir = source
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.add(AddRequest {
                kind: Some("torrent".into()),
                torrent_file: Some(save_to.to_string_lossy().into_owned()),
                dir: Some(dir),
                page_title: Some(created.name.clone()),
                ..Default::default()
            })
            .await?;
        }
        Ok(created.info_hash)
    }

    pub async fn open_stream(
        self: &Arc<Self>,
        hash: &str,
        file: usize,
    ) -> Result<(Box<dyn crate::torrent::ReadSeek>, u64, String)> {
        let engine = self.torrent_engine().await?;
        let name = engine
            .files(hash)
            .get(file)
            .map(|f| f.name.clone())
            .ok_or_else(|| anyhow!("file not found"))?;
        let (stream, len) = engine.stream(hash, file).await?;
        Ok((stream, len, name))
    }

    fn rss_path(&self) -> PathBuf {
        self.paths.data.join("rss.json")
    }

    fn with_rss<T>(&self, f: impl FnOnce(&mut RssStore) -> T) -> T {
        let mut st = self.auto.lock();
        if st.rss.is_none() {
            let loaded = std::fs::read(self.rss_path())
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            st.rss = Some(loaded);
        }
        f(st.rss.as_mut().expect("loaded above"))
    }

    fn save_rss(&self) {
        let data = self.with_rss(|r| serde_json::to_vec(r).ok());
        if let Some(bytes) = data {
            let _ = std::fs::write(self.rss_path(), bytes);
        }
    }

    pub fn rss_views(&self) -> Vec<FeedView> {
        let feeds = self.settings().rss_feeds;
        self.with_rss(|r| {
            let downloaded: HashSet<&String> = r.downloaded.iter().collect();
            feeds
                .iter()
                .map(|f| {
                    let state = r.feeds.get(&f.url).cloned().unwrap_or_default();
                    let done = state
                        .items
                        .iter()
                        .filter(|i| downloaded.contains(&i.id))
                        .map(|i| i.id.clone())
                        .collect();
                    FeedView {
                        url: f.url.clone(),
                        name: if f.name.trim().is_empty() {
                            state.title.clone().unwrap_or_else(|| f.url.clone())
                        } else {
                            f.name.clone()
                        },
                        enabled: f.enabled,
                        state,
                        downloaded: done,
                    }
                })
                .collect()
        })
    }

    pub async fn refresh_rss(self: &Arc<Self>) -> Result<usize> {
        {
            let mut st = self.auto.lock();
            if st.rss_busy {
                return Ok(0);
            }
            st.rss_busy = true;
            st.rss_last = Some(Instant::now());
        }
        let s = self.settings();
        let mut added = 0;
        for feed in s.rss_feeds.iter().filter(|f| f.enabled) {
            let result = async {
                let resp = self
                    .api_client
                    .get(&feed.url)
                    .timeout(Duration::from_secs(30))
                    .send()
                    .await?
                    .error_for_status()?;
                Ok::<_, anyhow::Error>(resp.text().await?)
            }
            .await;
            let (title, items, error) = match result {
                Ok(xml) => {
                    let (title, items) = rss::parse(&xml);
                    if items.is_empty() && title.is_none() {
                        (
                            None,
                            Vec::new(),
                            Some("not an RSS or Atom feed".to_string()),
                        )
                    } else {
                        (title, items, None)
                    }
                }
                Err(e) => (None, Vec::new(), Some(format!("{e:#}"))),
            };
            let matched: Vec<(RssItem, crate::settings::RssRule)> = self.with_rss(|r| {
                let state = r.feeds.entry(feed.url.clone()).or_default();
                state.error = error.clone();
                state.updated = Some(now_ms());
                if error.is_none() {
                    state.title = title.clone().or(state.title.take());
                    let mut merged = items.clone();
                    for old in &state.items {
                        if !merged.iter().any(|i| i.id == old.id) {
                            merged.push(old.clone());
                        }
                    }
                    merged.truncate(RSS_KEEP_ITEMS);
                    state.items = merged;
                }
                let downloaded: HashSet<String> = r.downloaded.iter().cloned().collect();
                items
                    .iter()
                    .filter(|i| !downloaded.contains(&i.id))
                    .filter_map(|i| {
                        s.rss_rules
                            .iter()
                            .find(|rule| rss::matches(rule, &feed.url, &i.title))
                            .map(|rule| (i.clone(), rule.clone()))
                    })
                    .collect()
            });
            for (item, rule) in matched {
                if self
                    .rss_add(&item, &rule.save_dir, rule.start_paused)
                    .await
                    .is_ok()
                {
                    added += 1;
                }
            }
        }
        self.save_rss();
        self.auto.lock().rss_busy = false;
        if added > 0 {
            self.toast("info", format!("RSS: {added} new downloads"));
        }
        let _ = self.app.emit("rss://changed", ());
        Ok(added)
    }

    async fn rss_add(self: &Arc<Self>, item: &RssItem, dir: &str, paused: bool) -> Result<String> {
        let id = self
            .add(AddRequest {
                url: item.download.clone(),
                kind: Some("auto".into()),
                dir: (!dir.trim().is_empty()).then(|| dir.trim().to_string()),
                page_title: Some(item.title.clone()),
                start_paused: paused,
                ..Default::default()
            })
            .await?;
        self.with_rss(|r| {
            r.downloaded.push(item.id.clone());
            let len = r.downloaded.len();
            if len > RSS_KEEP_DOWNLOADED {
                r.downloaded.drain(0..len - RSS_KEEP_DOWNLOADED);
            }
        });
        Ok(id)
    }

    pub async fn rss_download(self: &Arc<Self>, feed: &str, item_id: &str) -> Result<String> {
        let item = self
            .with_rss(|r| {
                r.feeds
                    .get(feed)
                    .and_then(|f| f.items.iter().find(|i| i.id == item_id).cloned())
            })
            .ok_or_else(|| anyhow!("item not found"))?;
        let id = self.rss_add(&item, "", false).await?;
        self.save_rss();
        let _ = self.app.emit("rss://changed", ());
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_reports_count_results_and_keep_problems() {
        use crate::cloudscan::FileVerdict;
        let base = Path::new("/d/Movie Pack");
        let mut r = CloudReport {
            total: 4,
            ..Default::default()
        };
        let v = |verdict: &str, detail: &str| FileVerdict::new(verdict, detail);
        let files = [
            base.join("movie.mkv"),
            base.join("extras").join("setup.exe"),
            base.join("codec.dll"),
            base.join("big.iso"),
        ];
        let verdicts = [
            v("clean", ""),
            v("malicious", "Trojan.Fake"),
            v("suspicious", "packed"),
            v("skipped", "larger than the 100 MB upload limit"),
        ];
        for (f, verdict) in files.iter().zip(&verdicts) {
            r.add("x", shown_name(base, f, false), f, verdict);
        }
        assert_eq!(
            (r.done, r.clean, r.threats, r.suspicious, r.not_checked),
            (4, 1, 1, 1, 1)
        );
        assert_eq!(r.findings.len(), 3);
        assert_eq!(
            r.findings[0].name,
            Path::new("extras").join("setup.exe").to_string_lossy()
        );
        assert_eq!(r.findings[0].detail, "Trojan.Fake");
        assert_eq!(
            shown_name(base, &PathBuf::from("/tmp/x/0001/inner.exe"), true),
            "inner.exe"
        );
    }

    #[test]
    fn names_files_in_the_all_downloads_report() {
        let base = Path::new("/d");
        assert_eq!(
            all_name("setup.exe", base, &base.join("setup.exe")),
            "setup.exe"
        );
        let inner = base.join("Movie Pack").join("extras").join("x.exe");
        let shown = all_name("Movie Pack", base, &inner);
        assert!(shown.starts_with("Movie Pack › "));
        assert!(shown.ends_with("x.exe"));
        assert!(!shown.contains("Movie Pack › Movie Pack"));
    }

    #[test]
    fn picks_files_for_the_automatic_cloud_scan() {
        let wanted: Vec<String> = ["exe", "ps1", "dmg", "appimage"]
            .iter()
            .map(|e| e.to_string())
            .collect();
        assert!(matches_extension(Path::new("C:/d/Setup.EXE"), &wanted));
        assert!(matches_extension(Path::new("/d/run.ps1"), &wanted));
        assert!(matches_extension(Path::new("/d/Installer.dmg"), &wanted));
        assert!(matches_extension(Path::new("/d/tool.AppImage"), &wanted));
        assert!(!matches_extension(Path::new("/d/movie.mkv"), &wanted));
        assert!(!matches_extension(Path::new("/d/notes.exe.txt"), &wanted));
        assert!(!matches_extension(Path::new("/d/exe"), &wanted));
        assert!(matches_extension(
            Path::new("/d/photos.zip"),
            &["zip".to_string()]
        ));
        assert!(!crate::settings::DEFAULT_CLOUD_EXTENSIONS.is_empty());
        let all = vec![crate::settings::ALL_FILES.to_string()];
        assert!(matches_extension(Path::new("/d/movie.mkv"), &all));
        assert!(matches_extension(Path::new("/d/README"), &all));
    }

    #[test]
    fn finds_media_files_whose_reported_name_was_mangled() {
        let dir = std::env::temp_dir().join(format!("mdm-id-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let real = dir.join("JİGOLOYA AŞIK [ATiiBYVBUlc].mp4");
        std::fs::write(&real, b"video").unwrap();
        std::fs::write(dir.join("JİGOLOYA AŞIK [ATiiBYVBUlc].f251.webm.part"), b"x").unwrap();
        let reported = dir.join("JGOLOYA AIK [ATiiBYVBUlc].mp4");
        assert_eq!(
            media_id("JGOLOYA AIK [ATiiBYVBUlc].mp4"),
            Some("ATiiBYVBUlc")
        );
        assert_eq!(media_id("holiday.mp4"), None);
        assert_eq!(find_by_media_id(&reported), Some(real));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn finds_every_file_of_a_multi_file_torrent() {
        let dir = std::env::temp_dir().join(format!("mdm-files-{}", uuid::Uuid::new_v4()));
        let top = dir.join("Album");
        std::fs::create_dir_all(top.join("CD2")).unwrap();
        std::fs::write(top.join("setup.exe"), b"x").unwrap();
        std::fs::write(top.join("CD2").join("track.mp3"), b"x").unwrap();
        let mut rec = super::super::blank_record("magnet:?xt=urn:btih:1");
        rec.kind = Kind::Torrent;
        rec.dir = dir.to_string_lossy().into_owned();
        rec.name = "Album".into();
        let files = downloaded_files(&rec);
        assert_eq!(
            files,
            vec![top.join("CD2").join("track.mp3"), top.join("setup.exe")]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    fn sched(from: &str, to: &str, days: Vec<u8>) -> Settings {
        Settings {
            alt_schedule_from: from.into(),
            alt_schedule_to: to.into(),
            alt_schedule_days: days,
            ..Default::default()
        }
    }

    #[test]
    fn schedule_windows() {
        let day = sched("08:00", "20:00", vec![0, 1, 2, 3, 4]);
        assert!(in_schedule(&day, 0, 9 * 60));
        assert!(!in_schedule(&day, 0, 21 * 60));
        assert!(!in_schedule(&day, 5, 9 * 60));
        let night = sched("22:00", "07:00", vec![4]);
        assert!(in_schedule(&night, 4, 23 * 60));
        assert!(in_schedule(&night, 5, 6 * 60));
        assert!(!in_schedule(&night, 5, 8 * 60));
        assert!(!in_schedule(&night, 3, 23 * 60));
    }
}
