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

impl Manager {
    pub(super) fn after_download(self: &Arc<Self>, rec: DownloadRecord) {
        let s = self.settings();
        if s.cloud_scan && s.cloud_scan_accepted {
            self.cloud_scan(&rec, s.cloud_scan_limit_mb);
        }
        if cfg!(all(unix, not(target_os = "macos"))) || (!s.mark_downloads && !s.scan_downloads) {
            return;
        }
        let files = downloaded_files(&rec);
        if files.is_empty() {
            if s.scan_downloads && rec.kind == Kind::Http && cfg!(windows) {
                let why = "the file was removed right after it was downloaded, most likely by your antivirus";
                self.with_entry(&rec.id, |e| {
                    e.rec.scan_state = "threat".into();
                    e.rec.scan_detail = Some(why.into());
                });
                self.notify("threat", &format!("{}: {why}", rec.name));
                self.mark_dirty();
            }
            return;
        }
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            let (mark, scan) = (
                s.mark_downloads && crate::security::smartscreen_status() == "on",
                s.scan_downloads && cfg!(windows),
            );
            if scan {
                this.with_entry(&rec.id, |e| e.rec.scan_state = "scanning".into());
                this.mark_dirty();
            }
            let url = rec.url.clone();
            let referrer = rec.referrer.clone();
            let list = files.clone();
            let verdict = tauri::async_runtime::spawn_blocking(move || {
                if mark {
                    for f in &list {
                        crate::security::mark_from_internet(f, &url, referrer.as_deref());
                    }
                }
                scan.then(|| crate::security::scan(&list))
            })
            .await
            .ok()
            .flatten();
            let Some(verdict) = verdict else { return };
            let (state, detail) = match &verdict {
                crate::security::Verdict::Clean(by) => ("clean", Some(by.clone())),
                crate::security::Verdict::Threat(what) => ("threat", Some(what.clone())),
                crate::security::Verdict::Unavailable(why) => ("skipped", Some(why.clone())),
            };
            this.with_entry(&rec.id, |e| {
                e.rec.scan_state = state.into();
                e.rec.scan_detail = detail.clone();
            });
            if let crate::security::Verdict::Threat(what) = verdict {
                this.notify("threat", &format!("{}: {what}", rec.name));
                this.toast("error", format!("{}: {what}", rec.name));
            }
            this.mark_dirty();
            this.save_needed.store(true, Ordering::Relaxed);
        });
    }

    fn cloud_scan(self: &Arc<Self>, rec: &DownloadRecord, limit_mb: u32) {
        let files = downloaded_files(rec);
        if files.is_empty() {
            return;
        }
        let id = rec.id.clone();
        let name = rec.name.clone();
        self.with_entry(&id, |e| {
            e.rec.cloud_state = "scanning".into();
            e.rec.cloud_detail = None;
        });
        self.mark_dirty();
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            let summary = crate::cloudscan::scan(&files, limit_mb).await;
            this.with_entry(&id, |e| {
                e.rec.cloud_state = summary.state.into();
                e.rec.cloud_detail = Some(summary.detail.clone());
            });
            if matches!(summary.state, "threat" | "suspicious") {
                this.notify("threat", &format!("{name}: {}", summary.detail));
                this.toast("error", format!("{name}: {}", summary.detail));
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
