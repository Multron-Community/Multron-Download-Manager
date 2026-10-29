use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::engine::{self, Progress};
use crate::util;

fn download_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .http1_only()
            .user_agent(engine::APP_USER_AGENT)
            .tcp_nodelay(true)
            .pool_max_idle_per_host(16)
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(30))
            .min_tls_version(reqwest::tls::Version::TLS_1_2)
            .build()
            .expect("download client")
    })
}

const YTDLP_REPO: &str = "yt-dlp/yt-dlp";
const DENO_REPO: &str = "denoland/deno";

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub ytdlp_installed: bool,
    pub ytdlp_version: Option<String>,
    pub ytdlp_path: String,
    pub ffmpeg_available: bool,
    pub ffmpeg_path: Option<String>,
    pub ffmpeg_managed: bool,
    pub ffmpeg_auto_install: bool,
    pub deno_available: bool,
    pub deno_managed: bool,
    pub deno_version: Option<String>,
    pub deno_auto_install: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgress {
    pub tool: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed: u64,
    pub done: bool,
}

#[derive(Default)]
pub struct SpeedMeter {
    last: Option<(u64, std::time::Instant)>,
    speed: f64,
}

impl SpeedMeter {
    pub fn sample(&mut self, bytes: u64) -> u64 {
        let now = std::time::Instant::now();
        match self.last {
            Some((before, at)) => {
                let secs = now.duration_since(at).as_secs_f64();
                if secs >= 0.2 {
                    let current = bytes.saturating_sub(before) as f64 / secs;
                    self.speed = if self.speed == 0.0 {
                        current
                    } else {
                        self.speed * 0.6 + current * 0.4
                    };
                    self.last = Some((bytes, now));
                }
            }
            None => self.last = Some((bytes, now)),
        }
        self.speed as u64
    }
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub title: String,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
    pub uploader: Option<String>,
    pub site: Option<String>,
    pub heights: Vec<u32>,
    pub is_playlist: bool,
    pub entries: Option<u32>,
    pub filesize: Option<u64>,
    pub sizes: std::collections::BTreeMap<String, QualitySize>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QualitySize {
    pub size: u64,
    pub exact: bool,
}

fn format_size(f: &Value) -> Option<QualitySize> {
    let n = |k: &str| f.get(k).and_then(Value::as_u64).filter(|n| *n > 0);
    n("filesize")
        .map(|size| QualitySize { size, exact: true })
        .or_else(|| n("filesize_approx").map(|size| QualitySize { size, exact: false }))
}

fn codec_rank(vcodec: &str) -> u8 {
    match vcodec.split('.').next().unwrap_or("") {
        "avc1" | "h264" => 3,
        "vp9" | "vp09" => 2,
        "av01" => 1,
        _ => 0,
    }
}

fn quality_sizes(v: &Value) -> std::collections::BTreeMap<String, QualitySize> {
    let mut out = std::collections::BTreeMap::new();
    let Some(formats) = v.get("formats").and_then(Value::as_array) else {
        return out;
    };
    let text = |f: &Value, k: &str| {
        f.get(k)
            .and_then(Value::as_str)
            .unwrap_or("none")
            .to_string()
    };
    let num = |f: &Value, k: &str| f.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let videos: Vec<&Value> = formats
        .iter()
        .filter(|f| {
            text(f, "vcodec") != "none" && f.get("height").and_then(Value::as_u64).is_some()
        })
        .collect();
    let audios: Vec<&Value> = formats
        .iter()
        .filter(|f| text(f, "vcodec") == "none" && text(f, "acodec") != "none")
        .collect();
    let rank = |f: &Value, codec: &str| {
        (
            format_size(f).is_some(),
            text(f, "acodec").starts_with(codec),
            num(f, "abr").max(num(f, "tbr")) as u64,
        )
    };
    let best_audio = |codec: &str| audios.iter().copied().max_by_key(|f| rank(f, codec));
    let pick_video = |cap: Option<u32>| {
        let within: Vec<&Value> = videos
            .iter()
            .copied()
            .filter(|f| cap.is_none_or(|c| f["height"].as_u64().unwrap_or(0) <= c as u64))
            .collect();
        let (pool, top) = if within.is_empty() {
            let low = videos.iter().filter_map(|f| f["height"].as_u64()).min()?;
            (videos.clone(), low)
        } else {
            let high = within.iter().filter_map(|f| f["height"].as_u64()).max()?;
            (within, high)
        };
        pool.into_iter()
            .filter(|f| f["height"].as_u64() == Some(top))
            .max_by_key(|f| {
                (
                    format_size(f).is_some(),
                    f.get("dynamic_range")
                        .and_then(Value::as_str)
                        .is_none_or(|r| r == "SDR"),
                    codec_rank(&text(f, "vcodec")),
                    num(f, "fps") as u64,
                    num(f, "tbr") as u64,
                )
            })
    };
    let aac = best_audio("mp4a");
    let mut add = |id: &str, cap: Option<u32>| {
        let Some(video) = pick_video(cap) else { return };
        let Some(vs) = format_size(video) else { return };
        let total = if text(video, "acodec") == "none" {
            match aac.and_then(format_size) {
                Some(a) => QualitySize {
                    size: vs.size + a.size,
                    exact: vs.exact && a.exact,
                },
                None => return,
            }
        } else {
            vs
        };
        out.insert(id.to_string(), total);
    };
    add("best", None);
    for h in HEIGHT_TIERS {
        add(&h.to_string(), Some(*h));
    }
    if let Some(a) = aac.and_then(format_size) {
        out.insert("audio-m4a".into(), a);
    }
    if let Some(a) = best_audio("opus").and_then(format_size) {
        out.insert("audio-opus".into(), QualitySize { exact: false, ..a });
    }
    if let Some(a) = aac.and_then(format_size) {
        out.insert("audio-aac".into(), QualitySize { exact: false, ..a });
    }
    if let Some(d) = v
        .get("duration")
        .and_then(Value::as_f64)
        .filter(|d| *d > 0.0)
    {
        let guess = |bytes_per_second: f64| QualitySize {
            size: (d * bytes_per_second) as u64,
            exact: false,
        };
        out.insert("audio-mp3".into(), guess(245_000.0 / 8.0));
        out.insert("audio-vorbis".into(), guess(48_000.0));
        out.insert("audio-wav".into(), guess(192_000.0));
        out.insert("audio-flac".into(), guess(200_000.0));
        out.insert("audio-alac".into(), guess(200_000.0));
    }
    out
}

pub struct Tools {
    pub dir: PathBuf,
    client: reqwest::Client,
    install_lock: tokio::sync::Mutex<()>,
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn ytdlp_asset() -> &'static str {
    if cfg!(windows) {
        "yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else if cfg!(target_arch = "aarch64") {
        "yt-dlp_linux_aarch64"
    } else {
        "yt-dlp_linux"
    }
}

fn ffmpeg_build_asset() -> Option<&'static str> {
    if cfg!(windows) {
        Some("ffmpeg-master-latest-win64-gpl.zip")
    } else if !cfg!(target_os = "linux") {
        None
    } else if cfg!(target_arch = "x86_64") {
        Some("ffmpeg-master-latest-linux64-gpl.tar.xz")
    } else if cfg!(target_arch = "aarch64") {
        Some("ffmpeg-master-latest-linuxarm64-gpl.tar.xz")
    } else {
        None
    }
}

fn deno_asset() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("deno-x86_64-pc-windows-msvc.zip")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("deno-aarch64-apple-darwin.zip")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("deno-x86_64-apple-darwin.zip")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("deno-x86_64-unknown-linux-gnu.zip")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("deno-aarch64-unknown-linux-gnu.zip")
    } else {
        None
    }
}

pub fn deno_auto_install() -> bool {
    deno_asset().is_some()
}

fn parse_deno_version(out: &str) -> Option<String> {
    out.lines()
        .next()?
        .strip_prefix("deno ")?
        .split_whitespace()
        .next()
        .map(str::to_string)
}

pub fn ffmpeg_auto_install() -> bool {
    cfg!(target_os = "macos") || ffmpeg_build_asset().is_some()
}

pub fn hide_console(cmd: &mut Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

impl Tools {
    pub fn new(dir: PathBuf, client: reqwest::Client) -> Self {
        Self {
            dir,
            client,
            install_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub fn ytdlp_path(&self) -> PathBuf {
        self.dir.join(exe("yt-dlp"))
    }

    fn managed_ffmpeg(&self) -> PathBuf {
        self.dir.join(exe("ffmpeg"))
    }

    pub fn ffmpeg_location(&self) -> Option<PathBuf> {
        if self.managed_ffmpeg().is_file() {
            return Some(self.dir.clone());
        }
        util::which("ffmpeg").and_then(|p| p.parent().map(Path::to_path_buf))
    }

    pub async fn status(&self) -> ToolStatus {
        let path = self.ytdlp_path();
        let installed = path.is_file();
        let version = if installed {
            self.ytdlp_version().await
        } else {
            None
        };
        let ffmpeg = self.ffmpeg_location();
        let deno = self.deno_location();
        let deno_version = match &deno {
            Some(p) => deno_version_of(p).await,
            None => None,
        };
        ToolStatus {
            ytdlp_installed: installed,
            ytdlp_version: version,
            ytdlp_path: path.to_string_lossy().into_owned(),
            ffmpeg_available: ffmpeg.is_some(),
            ffmpeg_path: ffmpeg.map(|p| p.to_string_lossy().into_owned()),
            ffmpeg_managed: self.managed_ffmpeg().is_file(),
            ffmpeg_auto_install: ffmpeg_auto_install(),
            deno_available: deno.is_some(),
            deno_managed: self.managed_deno().is_file(),
            deno_version,
            deno_auto_install: deno_auto_install(),
        }
    }

    fn managed_deno(&self) -> PathBuf {
        self.dir.join(exe("deno"))
    }

    pub fn deno_location(&self) -> Option<PathBuf> {
        let managed = self.managed_deno();
        if managed.is_file() {
            return Some(managed);
        }
        util::which("deno")
    }

    pub async fn install_deno(
        &self,
        on_progress: impl Fn(ToolProgress),
        cancel: &CancellationToken,
    ) -> Result<String> {
        let _guard = tokio::select! {
            g = self.install_lock.lock() => g,
            _ = cancel.cancelled() => bail!("cancelled"),
        };
        self.fetch_deno(&on_progress, cancel).await
    }

    pub async fn update_deno(&self, on_progress: impl Fn(ToolProgress)) -> Result<bool> {
        let managed = self.managed_deno();
        if !managed.is_file() {
            return Ok(false);
        }
        let latest = latest_release_tag(&self.client, DENO_REPO).await?;
        let installed = deno_version_of(&managed).await;
        if installed.as_deref() == Some(latest.trim_start_matches('v')) {
            return Ok(false);
        }
        let _guard = self.install_lock.lock().await;
        self.fetch_deno(&on_progress, &CancellationToken::new())
            .await?;
        Ok(true)
    }

    async fn fetch_deno(
        &self,
        on_progress: &impl Fn(ToolProgress),
        cancel: &CancellationToken,
    ) -> Result<String> {
        let Some(asset) = deno_asset() else {
            bail!("Deno is not available for this system; install it from deno.com")
        };
        tokio::fs::create_dir_all(&self.dir).await?;
        let archive = self.dir.join("deno.zip.download");
        let url = format!("https://github.com/{DENO_REPO}/releases/latest/download/{asset}");
        self.fetch_to(&url, &archive, "Deno", on_progress, cancel)
            .await?;
        let staged = self.dir.join(format!("{}.new", exe("deno")));
        let (from, to) = (archive.clone(), staged.clone());
        let extracted = tokio::task::spawn_blocking(move || extract_deno(&from, &to)).await;
        let _ = tokio::fs::remove_file(&archive).await;
        extracted??;
        let version = match deno_version_of(&staged).await {
            Some(v) => v,
            None => {
                let _ = tokio::fs::remove_file(&staged).await;
                bail!("the downloaded Deno does not run on this system")
            }
        };
        let target = self.managed_deno();
        if target.exists() {
            tokio::fs::remove_file(&target)
                .await
                .context("Deno is in use; it will be updated later")?;
        }
        tokio::fs::rename(&staged, &target).await?;
        Ok(version)
    }

    pub async fn ytdlp_version(&self) -> Option<String> {
        let mut cmd = Command::new(self.ytdlp_path());
        cmd.arg("--version").stdin(Stdio::null());
        hide_console(&mut cmd);
        let out = tokio::time::timeout(Duration::from_secs(20), cmd.output())
            .await
            .ok()?
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    pub async fn latest_ytdlp_version(&self) -> Result<String> {
        latest_release_tag(&self.client, YTDLP_REPO).await
    }

    pub async fn install_ytdlp(&self, on_progress: impl Fn(ToolProgress)) -> Result<String> {
        let _guard = self.install_lock.lock().await;
        tokio::fs::create_dir_all(&self.dir).await?;
        let url = format!(
            "https://github.com/{YTDLP_REPO}/releases/latest/download/{}",
            ytdlp_asset()
        );
        let tmp = self.dir.join(format!("{}.download", exe("yt-dlp")));
        self.fetch_to(
            &url,
            &tmp,
            "yt-dlp",
            &on_progress,
            &CancellationToken::new(),
        )
        .await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        }
        let target = self.ytdlp_path();
        if target.exists() {
            tokio::fs::remove_file(&target)
                .await
                .context("yt-dlp is in use; wait for media downloads to finish and try again")?;
        }
        tokio::fs::rename(&tmp, &target).await?;
        let version = self
            .ytdlp_version()
            .await
            .ok_or_else(|| anyhow!("downloaded yt-dlp does not run on this system"))?;
        Ok(version)
    }

    pub async fn ensure_ytdlp(&self, on_progress: impl Fn(ToolProgress)) -> Result<PathBuf> {
        let path = self.ytdlp_path();
        if !path.is_file() {
            self.install_ytdlp(on_progress).await?;
        }
        Ok(path)
    }

    pub async fn install_ffmpeg(
        &self,
        on_progress: impl Fn(ToolProgress),
        cancel: &CancellationToken,
    ) -> Result<()> {
        let _guard = tokio::select! {
            g = self.install_lock.lock() => g,
            _ = cancel.cancelled() => bail!("cancelled"),
        };
        if self.managed_ffmpeg().is_file() {
            return Ok(());
        }
        self.fetch_ffmpeg(&on_progress, cancel).await
    }

    fn ffmpeg_stamp_path(&self) -> PathBuf {
        self.dir.join("ffmpeg-version.json")
    }

    fn read_ffmpeg_stamp(&self) -> Option<(String, i64)> {
        let v: Value =
            serde_json::from_slice(&std::fs::read(self.ffmpeg_stamp_path()).ok()?).ok()?;
        Some((
            v.get("source")?.as_str()?.to_string(),
            v.get("installedAt")?.as_i64()?,
        ))
    }

    fn write_ffmpeg_stamp(&self, source: &str) {
        let body = serde_json::json!({ "source": source, "installedAt": util::now_ms() });
        let _ = std::fs::write(self.ffmpeg_stamp_path(), body.to_string());
    }

    pub async fn latest_ffmpeg_source(&self) -> Result<String> {
        if let Some(asset) = ffmpeg_build_asset() {
            let release: Value = self
                .client
                .get("https://api.github.com/repos/yt-dlp/FFmpeg-Builds/releases/tags/latest")
                .header("Accept", "application/vnd.github+json")
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            release
                .get("assets")
                .and_then(Value::as_array)
                .and_then(|a| {
                    a.iter()
                        .find(|x| x.get("name").and_then(Value::as_str) == Some(asset))
                })
                .and_then(|x| x.get("updated_at").and_then(Value::as_str))
                .map(str::to_string)
                .ok_or_else(|| anyhow!("the FFmpeg build was not found in the latest release"))
        } else if cfg!(target_os = "macos") {
            let info: Value = self
                .client
                .get("https://evermeet.cx/ffmpeg/info/ffmpeg/release")
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            info.get("version")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| anyhow!("the FFmpeg version could not be read"))
        } else {
            bail!("FFmpeg is updated by the system package manager")
        }
    }

    pub async fn update_ffmpeg(&self, on_progress: impl Fn(ToolProgress)) -> Result<bool> {
        if !self.managed_ffmpeg().is_file() {
            return Ok(false);
        }
        let latest = self.latest_ffmpeg_source().await?;
        let now = util::now_ms();
        match self.read_ffmpeg_stamp() {
            Some((source, _)) if source == latest => return Ok(false),
            Some((_, installed))
                if !cfg!(target_os = "macos") && now - installed < 7 * 24 * 3600 * 1000 =>
            {
                return Ok(false)
            }
            None => {
                let age = std::fs::metadata(self.managed_ffmpeg())
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(u64::MAX);
                if age < 7 * 24 * 3600 {
                    self.write_ffmpeg_stamp(&latest);
                    return Ok(false);
                }
            }
            _ => {}
        }
        let _guard = self.install_lock.lock().await;
        self.fetch_ffmpeg(&on_progress, &CancellationToken::new())
            .await?;
        self.write_ffmpeg_stamp(&latest);
        Ok(true)
    }

    async fn fetch_ffmpeg(
        &self,
        on_progress: &impl Fn(ToolProgress),
        cancel: &CancellationToken,
    ) -> Result<()> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let staging = self.dir.join("ffmpeg-update");
        let _ = tokio::fs::remove_dir_all(&staging).await;
        tokio::fs::create_dir_all(&staging).await?;
        let sources: Vec<(String, &str)> = if let Some(asset) = ffmpeg_build_asset() {
            vec![(
                format!("https://github.com/yt-dlp/FFmpeg-Builds/releases/download/latest/{asset}"),
                "ffmpeg",
            )]
        } else if cfg!(target_os = "macos") {
            vec![
                (
                    "https://evermeet.cx/ffmpeg/getrelease/zip".to_string(),
                    "ffmpeg",
                ),
                (
                    "https://evermeet.cx/ffmpeg/getrelease/ffprobe/zip".to_string(),
                    "ffprobe",
                ),
            ]
        } else {
            bail!("Install FFmpeg with your package manager, for example `sudo apt install ffmpeg`, `sudo dnf install ffmpeg` or `sudo pacman -S ffmpeg`");
        };
        for (url, label) in sources {
            let tar = url.ends_with(".tar.xz");
            let ext = if tar { "tar.xz" } else { "zip" };
            let archive = self.dir.join(format!("{label}.{ext}.download"));
            self.fetch_to(&url, &archive, label, on_progress, cancel)
                .await?;
            let target = staging.clone();
            let archive2 = archive.clone();
            let extracted = tokio::task::spawn_blocking(move || {
                if tar {
                    extract_ffmpeg_tar(&archive2, &target)
                } else {
                    extract_ffmpeg(&archive2, &target)
                }
            })
            .await;
            let _ = tokio::fs::remove_file(&archive).await;
            extracted??;
        }
        if !staging.join(exe("ffmpeg")).is_file() {
            let _ = tokio::fs::remove_dir_all(&staging).await;
            bail!("ffmpeg was not found in the downloaded archive");
        }
        for name in [exe("ffmpeg"), exe("ffprobe")] {
            let from = staging.join(&name);
            if from.is_file() {
                let to = self.dir.join(&name);
                if to.exists() {
                    tokio::fs::remove_file(&to)
                        .await
                        .context("FFmpeg is in use; it will be updated later")?;
                }
                tokio::fs::rename(&from, &to).await?;
            }
        }
        let _ = tokio::fs::remove_dir_all(&staging).await;
        Ok(())
    }

    async fn fetch_to(
        &self,
        url: &str,
        dest: &Path,
        label: &str,
        on_progress: &impl Fn(ToolProgress),
        cancel: &CancellationToken,
    ) -> Result<()> {
        let client = download_client();
        let mut headers = reqwest::header::HeaderMap::new();
        let probe = engine::probe_with_fallback(client, url, &mut headers)
            .await
            .with_context(|| format!("download of {label} failed"))?;
        let progress = Progress::new(probe.total, &[], 8);
        let job = Arc::new(engine::Job {
            client: client.clone(),
            url: probe.final_url.clone(),
            headers,
            part: dest.to_path_buf(),
            resumable: probe.resumable,
            progress: progress.clone(),
            cancel: cancel.child_token(),
            limiters: Vec::new(),
            retries: 4,
            transform: None,
            if_range: probe.validator.clone(),
            max_request: None,
        });
        let mut run = tokio::spawn(engine::run(job));
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        let mut meter = SpeedMeter::default();
        let mut report = |done: bool| {
            let downloaded = progress.downloaded();
            ToolProgress {
                tool: label.to_string(),
                downloaded,
                total: progress.total(),
                speed: meter.sample(downloaded),
                done,
            }
        };
        let finished = loop {
            tokio::select! {
                r = &mut run => break r.context("download task failed")?,
                _ = tick.tick() => on_progress(report(false)),
            }
        };
        on_progress(report(true));
        match finished {
            Ok(true) => Ok(()),
            Ok(false) => {
                let _ = tokio::fs::remove_file(dest).await;
                bail!("cancelled")
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(dest).await;
                Err(e.context(format!("download of {label} failed")))
            }
        }
    }

    pub async fn probe(
        &self,
        url: &str,
        cookies: Option<&Path>,
        browser: &str,
    ) -> Result<MediaInfo> {
        let exe = self.ensure_ytdlp(|_| {}).await?;
        let mut cmd = Command::new(exe);
        cmd.args(["--encoding", "utf-8"]);
        cmd.args(["-J", "--flat-playlist", "--no-warnings", "--skip-download"]);
        apply_cookies(&mut cmd, cookies, browser);
        apply_js_runtime(&mut cmd, self.deno_location().as_deref());
        cmd.arg("--").arg(url);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PYTHONIOENCODING", "utf-8")
            .kill_on_drop(true);
        hide_console(&mut cmd);
        let out = tokio::time::timeout(Duration::from_secs(90), cmd.output())
            .await
            .map_err(|_| anyhow!("yt-dlp took too long to answer"))??;
        if !out.status.success() {
            bail!(last_error(&String::from_utf8_lossy(&out.stderr)));
        }
        let v: Value =
            serde_json::from_slice(&out.stdout).context("yt-dlp returned invalid JSON")?;
        Ok(parse_info(&v))
    }
}

const INSTAGRAM_PAGES: &[&str] = &[
    "p", "reel", "reels", "tv", "stories", "explore", "accounts", "direct", "about", "legal",
];

pub fn instagram_profile(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return false;
    };
    let host = u.host_str().unwrap_or("").to_ascii_lowercase();
    if host != "instagram.com" && !host.ends_with(".instagram.com") {
        return false;
    }
    let parts: Vec<&str> = u
        .path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();
    match parts.as_slice() {
        [user] => !INSTAGRAM_PAGES.contains(user),
        [user, tab] => {
            !INSTAGRAM_PAGES.contains(user) && matches!(*tab, "reels" | "tagged" | "saved" | "igtv")
        }
        _ => false,
    }
}

pub fn site_name(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.trim_start_matches("www.").to_string())
        })
        .unwrap_or_else(|| "this site".into())
}

pub fn is_youtube(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|h| {
            h == "youtu.be"
                || h == "youtube.com"
                || h.ends_with(".youtube.com")
                || h == "youtube-nocookie.com"
        })
}

pub fn needs_sign_in(error: &str) -> bool {
    let e = error.to_lowercase();
    [
        "sign in",
        "private video",
        "members-only",
        "join this channel",
        "not a bot",
        "age-restricted",
        "inappropriate for some users",
        "login required",
        "need to log in",
        "log in to access",
    ]
    .iter()
    .any(|k| e.contains(k))
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Failure {
    Network,
    Corrupt,
    Other,
}

pub fn classify_failure(error: &str) -> Failure {
    let e = error.to_lowercase();
    const CORRUPT: [&str; 7] = [
        "invalid data found",
        "error opening input",
        "postprocessing:",
        "moov atom not found",
        "conversion failed",
        "unable to merge",
        "could not find codec parameters",
    ];
    const NETWORK: [&str; 30] = [
        "unable to download",
        "http error 403",
        "http error 5",
        "timed out",
        "timeout",
        "connection reset",
        "connection aborted",
        "connection refused",
        "connection closed",
        "connection interrupted",
        "remote end closed",
        "eof occurred",
        "ssl",
        "tls",
        "certificate",
        "getaddrinfo",
        "name resolution",
        "name or service not known",
        "no route",
        "network is unreachable",
        "could not connect",
        "error sending request",
        "incompleteread",
        "giving up after",
        "got error",
        "download stalled",
        "os error 10053",
        "os error 10054",
        "os error 10060",
        "os error 10051",
    ];
    if CORRUPT.iter().any(|k| e.contains(k)) {
        Failure::Corrupt
    } else if NETWORK.iter().any(|k| e.contains(k)) {
        Failure::Network
    } else {
        Failure::Other
    }
}

pub fn cookies_rejected(error: &str) -> bool {
    let e = error.to_lowercase();
    e.contains("page needs to be reloaded") || e.contains("cookies are no longer valid")
}

fn apply_cookies(cmd: &mut Command, cookies: Option<&Path>, browser: &str) {
    if let Some(file) = cookies {
        cmd.arg("--cookies").arg(file);
    } else if !browser.trim().is_empty() {
        cmd.arg("--cookies-from-browser").arg(browser.trim());
    }
}

fn parse_info(v: &Value) -> MediaInfo {
    let is_playlist = v.get("_type").and_then(Value::as_str) == Some("playlist");
    let mut heights: Vec<u32> = v
        .get("formats")
        .and_then(Value::as_array)
        .map(|formats| {
            formats
                .iter()
                .filter(|f| f.get("vcodec").and_then(Value::as_str) != Some("none"))
                .filter_map(|f| f.get("height").and_then(Value::as_u64))
                .map(|h| h as u32)
                .collect()
        })
        .unwrap_or_default();
    heights.sort_unstable_by(|a, b| b.cmp(a));
    heights.dedup();
    let thumbnail = v
        .get("thumbnail")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            v.get("thumbnails")
                .and_then(Value::as_array)
                .and_then(|t| t.last())
                .and_then(|t| t.get("url"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    MediaInfo {
        title: v
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("media")
            .to_string(),
        thumbnail,
        duration: v.get("duration").and_then(Value::as_f64),
        uploader: v
            .get("uploader")
            .or_else(|| v.get("channel"))
            .and_then(Value::as_str)
            .map(str::to_string),
        site: v
            .get("extractor_key")
            .and_then(Value::as_str)
            .map(str::to_string),
        heights,
        is_playlist,
        entries: v
            .get("entries")
            .and_then(Value::as_array)
            .map(|e| e.len() as u32),
        filesize: v
            .get("filesize")
            .or_else(|| v.get("filesize_approx"))
            .and_then(Value::as_u64),
        sizes: quality_sizes(v),
    }
}

async fn deno_version_of(path: &Path) -> Option<String> {
    let mut cmd = Command::new(path);
    cmd.arg("--version").stdin(Stdio::null());
    hide_console(&mut cmd);
    let out = tokio::time::timeout(Duration::from_secs(20), cmd.output())
        .await
        .ok()?
        .ok()?;
    out.status
        .success()
        .then(|| parse_deno_version(&String::from_utf8_lossy(&out.stdout)))
        .flatten()
}

fn extract_deno(archive: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name()?.replace('\\', "/");
        if name.rsplit('/').next() == Some(exe("deno").as_str()) {
            let mut out = std::fs::File::create(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))?;
            }
            return Ok(());
        }
    }
    bail!("deno was not found in the downloaded archive")
}

fn extract_ffmpeg(archive: &Path, dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name()?.replace('\\', "/");
        let base = name.rsplit('/').next().unwrap_or(&name).to_string();
        if base == exe("ffmpeg") || base == exe("ffprobe") {
            let out_path = dir.join(&base);
            let mut out = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(0o755))?;
            }
        }
    }
    Ok(())
}

fn extract_ffmpeg_tar(archive: &Path, dir: &Path) -> Result<()> {
    let raw = dir.join("unpacked");
    std::fs::create_dir_all(&raw)?;
    let untar = |members: &[&str]| {
        std::process::Command::new("tar")
            .arg("-xJf")
            .arg(archive)
            .arg("-C")
            .arg(&raw)
            .args(members)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("tar is needed to unpack FFmpeg")
    };
    let mut status = untar(&["--wildcards", "*/bin/ffmpeg", "*/bin/ffprobe"])?;
    if !status.success() {
        status = untar(&[])?;
    }
    if !status.success() {
        let _ = std::fs::remove_dir_all(&raw);
        bail!("FFmpeg could not be unpacked (tar exited with {status})");
    }
    for top in std::fs::read_dir(&raw)?.flatten() {
        for name in ["ffmpeg", "ffprobe"] {
            let from = top.path().join("bin").join(name);
            if from.is_file() {
                let to = dir.join(name);
                std::fs::rename(&from, &to)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&to, std::fs::Permissions::from_mode(0o755))?;
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&raw);
    Ok(())
}

pub async fn latest_release_tag(client: &reqwest::Client, repo: &str) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
    }
    let release: Release = client
        .get(format!(
            "https://api.github.com/repos/{repo}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(release.tag_name)
}

fn last_error(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .find(|l| l.contains("ERROR"))
        .or_else(|| stderr.lines().rev().find(|l| !l.trim().is_empty()))
        .map(|l| l.trim().trim_start_matches("ERROR:").trim().to_string())
        .unwrap_or_else(|| "yt-dlp failed".to_string())
}

#[derive(Clone)]
pub struct MediaJob {
    pub exe: PathBuf,
    pub url: String,
    pub dir: PathBuf,
    pub quality: String,
    pub container: String,
    pub playlist: bool,
    pub fragments: u32,
    pub rate_limit: u64,
    pub ffmpeg: Option<PathBuf>,
    pub cookies_file: Option<PathBuf>,
    pub cookies_browser: String,
    pub embed_metadata: bool,
    pub embed_thumbnail: bool,
    pub filename: Option<String>,
    pub js_runtime: Option<PathBuf>,
    pub local_info: Option<PathBuf>,
}

fn output_template(job: &MediaJob) -> String {
    let named = job
        .filename
        .as_deref()
        .map(output_name)
        .filter(|n| !n.is_empty());
    match (&named, job.playlist) {
        (_, true) => {
            "%(playlist_title|Playlist).120B/%(playlist_index)03d - %(title).150B [%(id)s].%(ext)s"
                .to_string()
        }
        (Some(name), false) => format!("{name}.%(ext)s"),
        (None, false) => "%(title).180B [%(id)s].%(ext)s".to_string(),
    }
}

pub async fn extract(job: &MediaJob) -> Result<Value> {
    let mut cmd = Command::new(&job.exe);
    cmd.args([
        "--encoding",
        "utf-8",
        "-J",
        "--no-warnings",
        "--no-playlist",
    ])
    .arg("-P")
    .arg(&job.dir)
    .args(["-o", output_template(job).as_str()]);
    if let Some(ff) = &job.ffmpeg {
        cmd.arg("--ffmpeg-location").arg(ff);
    }
    cmd.args(format_args(
        &job.quality,
        job.ffmpeg.is_some(),
        &job.container,
    ));
    apply_cookies(&mut cmd, job.cookies_file.as_deref(), &job.cookies_browser);
    apply_js_runtime(&mut cmd, job.js_runtime.as_deref());
    cmd.arg("--").arg(&job.url);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .kill_on_drop(true);
    hide_console(&mut cmd);
    let out = tokio::time::timeout(Duration::from_secs(90), cmd.output())
        .await
        .map_err(|_| anyhow!("yt-dlp took too long to answer"))??;
    if !out.status.success() {
        bail!(last_error(&String::from_utf8_lossy(&out.stderr)));
    }
    serde_json::from_slice(&out.stdout).context("yt-dlp returned invalid JSON")
}

pub fn direct_formats(info: &Value) -> Option<Vec<Value>> {
    if info.get("_type").and_then(Value::as_str) == Some("playlist")
        || info.get("is_live").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    let picked: Vec<Value> = match info.get("requested_formats").and_then(Value::as_array) {
        Some(list) if !list.is_empty() => list.clone(),
        _ => vec![info.clone()],
    };
    let direct = |f: &Value| {
        matches!(
            f.get("protocol").and_then(Value::as_str),
            Some("https" | "http")
        ) && f
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(|u| u.starts_with("http"))
            && f.get("format_id").and_then(Value::as_str).is_some()
    };
    picked.iter().all(direct).then_some(picked)
}

pub fn format_headers(format: &Value) -> reqwest::header::HeaderMap {
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue, COOKIE};
    let mut headers = HeaderMap::new();
    if let Some(map) = format.get("http_headers").and_then(Value::as_object) {
        for (k, v) in map {
            if let (Ok(name), Some(Ok(value))) = (
                HeaderName::from_bytes(k.as_bytes()),
                v.as_str().map(HeaderValue::from_str),
            ) {
                headers.insert(name, value);
            }
        }
    }
    if let Some(raw) = format.get("cookies").and_then(Value::as_str) {
        const ATTRIBUTES: &[&str] = &[
            "domain", "path", "expires", "max-age", "secure", "httponly", "samesite",
        ];
        let pairs: Vec<&str> = raw
            .split(';')
            .map(str::trim)
            .filter(|part| {
                part.split_once('=').is_some_and(|(k, _)| {
                    !ATTRIBUTES.contains(&k.trim().to_ascii_lowercase().as_str())
                })
            })
            .collect();
        if let Ok(value) = HeaderValue::from_str(&pairs.join("; ")) {
            if !pairs.is_empty() {
                headers.insert(COOKIE, value);
            }
        }
    }
    headers
}

pub fn use_local_files(info: &mut Value, local: &[(String, PathBuf)]) {
    let file_url = |p: &Path| url::Url::from_file_path(p).ok().map(|u| u.to_string());
    let swap = |f: &mut Value| {
        let id = f
            .get("format_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some((_, path)) = local.iter().find(|(fid, _)| Some(fid) == id.as_ref()) {
            if let Some(u) = file_url(path) {
                f["url"] = Value::String(u);
                if let Some(obj) = f.as_object_mut() {
                    obj.remove("fragments");
                    obj.remove("downloader_options");
                }
            }
        }
    };
    for key in ["formats", "requested_formats"] {
        if let Some(list) = info.get_mut(key).and_then(Value::as_array_mut) {
            list.iter_mut().for_each(swap);
        }
    }
    if info.get("requested_formats").is_none() {
        swap(info);
    }
}

fn youtube_split_args(url: &str, fragments: u32) -> &'static [&'static str] {
    if fragments > 1 && is_youtube(url) {
        &["--extractor-args", "youtube:formats=dashy"]
    } else {
        &[]
    }
}

fn apply_js_runtime(cmd: &mut Command, runtime: Option<&Path>) {
    if let Some(p) = runtime {
        cmd.arg("--js-runtimes")
            .arg(format!("deno:{}", p.to_string_lossy()));
    }
}

#[derive(Debug, Clone)]
pub enum MediaEvent {
    Progress {
        downloaded: u64,
        total: Option<u64>,
        estimate: Option<u64>,
        speed: Option<u64>,
        eta: Option<u64>,
        index: Option<u32>,
        count: Option<u32>,
    },
    Phase(String),
    Title(String),
    Size(u64),
    File(PathBuf),
    Destination(PathBuf),
}

#[derive(Default, Debug)]
pub struct MediaTotals {
    finished: u64,
    part_seen: u64,
    part_total: Option<u64>,
    exact: Option<u64>,
    shown: u64,
    guess: Option<u64>,
}

impl MediaTotals {
    pub fn set_exact(&mut self, size: u64) {
        if size > 0 {
            self.exact = Some(size);
        }
    }

    pub fn update(
        &mut self,
        downloaded: u64,
        total: Option<u64>,
        estimate: Option<u64>,
    ) -> (u64, Option<u64>) {
        let part_done = match self.part_total {
            Some(t) => t > 0 && self.part_seen >= t / 10 * 9,
            None => self.part_seen > 0,
        };
        if part_done && downloaded < self.part_seen / 4 {
            self.finished += self.part_total.unwrap_or(0).max(self.part_seen);
            self.part_seen = 0;
            self.part_total = None;
        }
        self.part_seen = self.part_seen.max(downloaded);
        if let Some(t) = total {
            self.part_total = Some(t.max(self.part_seen));
        }
        self.shown = self.shown.max(self.finished + self.part_seen);
        let size = if let Some(e) = self.exact {
            Some(e.max(self.shown))
        } else if let Some(t) = self.part_total {
            Some(self.finished + t)
        } else {
            let fresh = estimate
                .filter(|e| *e > 0 && self.part_seen as f64 >= *e as f64 * 0.2)
                .map(|e| self.finished + e.max(self.part_seen));
            match (self.guess, fresh) {
                (Some(g), _) if (self.shown as f64) < g as f64 * 0.98 => Some(g),
                (_, Some(f)) => {
                    let g = f.max(self.shown + self.shown / 50);
                    self.guess = Some(g);
                    Some(g)
                }
                (g, None) => g.map(|g| g.max(self.shown)),
            }
        };
        (self.shown, size.filter(|t| *t >= self.shown))
    }
}

pub fn exact_size(line: &str) -> Option<u64> {
    let mut parts = line.splitn(3, '|');
    let sizes: Vec<u64> = serde_json::from_str::<Vec<Value>>(parts.next()?.trim())
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    let formats = serde_json::from_str::<Vec<Value>>(parts.next()?.trim())
        .map(|v| v.len())
        .unwrap_or(0);
    if formats > 0 {
        return (sizes.len() == formats).then(|| sizes.iter().sum());
    }
    parts.next()?.trim().parse().ok().filter(|n| *n > 0)
}

const PROGRESS_PREFIX: &str = "[MDM]";

fn video_sort(height: Option<u32>, container: &str) -> String {
    let res = height.map_or_else(|| "res".to_string(), |h| format!("res:{h}"));
    match container.to_ascii_lowercase().as_str() {
        "mp4" | "mp4-h264" | "mov" | "avi" | "flv" => format!("{res},vcodec:h264,fps,acodec:aac"),
        "webm" => format!("{res},fps,ext:webm:webm"),
        _ => format!("{res},fps,ext:mp4:m4a"),
    }
}

const AUDIO_FORMATS: &[&str] = &["m4a", "opus", "aac", "flac", "wav", "alac", "vorbis"];

const HEIGHT_TIERS: &[u32] = &[6480, 4320, 2880, 2160, 1440, 1080, 720, 480, 360, 240, 144];

fn merge_format(container: &str) -> &str {
    if container.eq_ignore_ascii_case("mp4-h264") {
        "mp4"
    } else {
        container
    }
}

fn video_selector(cap: Option<u32>, container: &str) -> String {
    let (video, only) = match container.to_ascii_lowercase().as_str() {
        "webm" => ("[ext=webm]", "[ext=webm]"),
        "mp4-h264" => ("[vcodec^=avc]", "[acodec^=mp4a]"),
        _ => ("", ""),
    };
    let limit = cap
        .map(|h| format!("[height<=?{h}]{video}"))
        .unwrap_or_else(|| video.to_string());
    let mut choices: Vec<String> = HEIGHT_TIERS
        .iter()
        .filter(|t| cap.is_none_or(|c| **t <= c))
        .flat_map(|t| {
            [
                format!("bv*[height>={t}]{limit}[filesize>0]+ba{only}[filesize>0]"),
                format!("bv*[height>={t}]{limit}+ba{only}"),
            ]
        })
        .collect();
    choices.push(format!("bv*{limit}+ba{only}"));
    choices.push(format!("b{limit}"));
    choices.push("bv*+ba/b".into());
    choices.join("/")
}

fn format_args(quality: &str, has_ffmpeg: bool, container: &str) -> Vec<String> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    match quality {
        "audio-mp3" if has_ffmpeg => s(&[
            "-f",
            "ba/b",
            "-x",
            "--audio-format",
            "mp3",
            "--audio-quality",
            "0",
        ]),
        q if has_ffmpeg
            && q.strip_prefix("audio-")
                .is_some_and(|f| AUDIO_FORMATS.contains(&f)) =>
        {
            let target = &q["audio-".len()..];
            let source = if target == "m4a" || target == "aac" {
                "ba[ext=m4a]/ba/b"
            } else if target == "opus" || target == "vorbis" {
                "ba[acodec=opus]/ba/b"
            } else {
                "ba/b"
            };
            let mut args = s(&[
                "-f",
                source,
                "-x",
                "--audio-format",
                target,
                "--audio-quality",
                "0",
            ]);
            if target == "alac" {
                args.extend(s(&["--postprocessor-args", "ExtractAudio:-c:a alac"]));
            }
            args
        }
        q if q == "audio" || q.starts_with("audio-") => {
            if has_ffmpeg {
                s(&["-f", "ba[ext=m4a]/ba/b", "-x", "--audio-format", "m4a"])
            } else {
                s(&["-f", "ba[ext=m4a]/ba/b"])
            }
        }
        q => {
            let height: Option<u32> = q.trim_end_matches('p').parse().ok();
            if !has_ffmpeg {
                return match height {
                    Some(h) => vec!["-f".into(), format!("b[height<={h}]/b")],
                    None => s(&["-f", "b"]),
                };
            }
            vec![
                "-f".into(),
                video_selector(height, container),
                "-S".into(),
                video_sort(height, container),
                "--merge-output-format".into(),
                merge_format(container).into(),
            ]
        }
    }
}

fn output_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let trimmed: String = clean
        .trim()
        .trim_end_matches('.')
        .chars()
        .take(150)
        .collect();
    trimmed.trim().replace('%', "%%")
}

fn parse_num(s: Option<&str>) -> Option<f64> {
    s.and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
}

fn post_phase(processor: &str) -> &'static str {
    if processor.starts_with("Merger") {
        "merging"
    } else if ["ExtractAudio", "VideoConvertor", "VideoRemuxer"]
        .iter()
        .any(|p| processor.contains(p))
    {
        "converting"
    } else if processor.contains("Metadata") {
        "tagging"
    } else if processor.contains("EmbedThumbnail") {
        "thumbnail"
    } else {
        "finishing"
    }
}

fn parse_progress(line: &str) -> Option<MediaEvent> {
    let rest = line.split_once(PROGRESS_PREFIX)?.1;
    let f: Vec<&str> = rest.split_whitespace().collect();
    let downloaded = parse_num(f.first().copied())? as u64;
    Some(MediaEvent::Progress {
        downloaded,
        total: parse_num(f.get(1).copied()).map(|t| t as u64),
        estimate: parse_num(f.get(2).copied()).map(|t| t as u64),
        speed: parse_num(f.get(3).copied()).map(|v| v as u64),
        eta: parse_num(f.get(4).copied()).map(|v| v as u64),
        index: parse_num(f.get(5).copied()).map(|v| v as u32),
        count: parse_num(f.get(6).copied()).map(|v| v as u32),
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn merge(
    ffmpeg_dir: &Path,
    video: &str,
    audio: &str,
    referrer: Option<&str>,
    user_agent: Option<&str>,
    dest: &Path,
    cancel: CancellationToken,
    mut on_event: impl FnMut(MediaEvent),
) -> Result<bool> {
    let part = dest.with_extension("part.mp4");
    let mut cmd = Command::new(ffmpeg_dir.join(exe("ffmpeg")));
    cmd.args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y"]);
    for url in [video, audio] {
        if let Some(agent) = user_agent.filter(|a| !a.trim().is_empty()) {
            cmd.arg("-user_agent").arg(agent);
        }
        if let Some(referer) = referrer.filter(|r| r.starts_with("https://")) {
            cmd.arg("-referer").arg(referer);
        }
        cmd.arg("-i").arg(url);
    }
    cmd.args([
        "-map",
        "0:v:0",
        "-map",
        "1:a:0?",
        "-c",
        "copy",
        "-movflags",
        "+faststart",
        "-progress",
        "pipe:1",
        "-nostats",
        "-f",
        "mp4",
    ])
    .arg(&part)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
    hide_console(&mut cmd);
    #[cfg(unix)]
    cmd.process_group(0);

    on_event(MediaEvent::Phase("downloading".into()));
    let mut child = cmd.spawn().context("could not start FFmpeg")?;
    let pid = child.id();
    let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
    let stderr = child.stderr.take().ok_or_else(|| anyhow!("no stderr"))?;
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut tail: Vec<String> = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            tail.push(line);
            if tail.len() > 20 {
                tail.remove(0);
            }
        }
        tail.join("\n")
    });
    let mut lines = BufReader::new(stdout).lines();
    loop {
        let line = tokio::select! {
            _ = cancel.cancelled() => {
                kill_tree(&mut child, pid).await;
                let _ = tokio::fs::remove_file(&part).await;
                return Ok(false);
            }
            l = lines.next_line() => l,
        };
        let Ok(Some(line)) = line else { break };
        if let Some(size) = line
            .strip_prefix("total_size=")
            .and_then(|s| s.trim().parse::<u64>().ok())
        {
            on_event(MediaEvent::Progress {
                downloaded: size,
                total: None,
                estimate: None,
                speed: None,
                eta: None,
                index: None,
                count: None,
            });
        }
    }
    let status = tokio::select! {
        _ = cancel.cancelled() => {
            kill_tree(&mut child, pid).await;
            let _ = tokio::fs::remove_file(&part).await;
            return Ok(false);
        }
        s = child.wait() => s?,
    };
    let stderr = stderr_task.await.unwrap_or_default();
    if !status.success() {
        let _ = tokio::fs::remove_file(&part).await;
        let why = stderr.lines().last().unwrap_or("").trim().to_string();
        if why.is_empty() {
            bail!("FFmpeg could not download and join the picture and sound");
        }
        bail!("FFmpeg could not download and join the picture and sound: {why}");
    }
    tokio::fs::rename(&part, dest)
        .await
        .context("could not save the joined video")?;
    on_event(MediaEvent::File(dest.to_path_buf()));
    Ok(true)
}

pub async fn run(
    job: MediaJob,
    cancel: CancellationToken,
    mut on_event: impl FnMut(MediaEvent),
) -> Result<bool> {
    let mut cmd = Command::new(&job.exe);
    cmd.args(["--encoding", "utf-8"]);
    let template = output_template(&job);
    cmd.args(["--newline", "--no-quiet", "--progress", "--no-colors", "--continue", "--no-mtime", "--windows-filenames"])
        .args(["--progress-template", &format!(
            "download:{PROGRESS_PREFIX} %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s %(info.playlist_index)s %(info.n_entries)s"
        )])
        .args(["--progress-template", "postprocess:MDMPP %(progress.postprocessor)s %(progress.status)s"])
        .args(["--print", "before_dl:MDMTITLE:%(title)s"])
        .args(["--print", "before_dl:MDMSIZE:%(requested_formats.:.filesize)j|%(requested_formats.:.format_id)j|%(filesize)s"])
        .args(["--print", "after_move:MDMFILE:%(filepath)s"])
        .args(["--concurrent-fragments", &job.fragments.clamp(1, crate::settings::MAX_THREADS).to_string()])
        .args(["--retries", "15", "--fragment-retries", "15", "--file-access-retries", "5"])
        .args(["--retry-sleep", "exp=1:15", "--retry-sleep", "fragment:exp=1:15", "--socket-timeout", "20"])
        .arg("-P")
        .arg(&job.dir)
        .args(["-o", template.as_str()]);
    cmd.arg(if job.playlist {
        "--yes-playlist"
    } else {
        "--no-playlist"
    });
    if let Some(ff) = &job.ffmpeg {
        cmd.arg("--ffmpeg-location").arg(ff);
    }
    cmd.args(format_args(
        &job.quality,
        job.ffmpeg.is_some(),
        &job.container,
    ));
    if job.ffmpeg.is_some() {
        if job.embed_metadata {
            cmd.arg("--embed-metadata");
        }
        if job.embed_thumbnail {
            cmd.arg("--embed-thumbnail");
        }
    }
    match &job.local_info {
        Some(info) => {
            cmd.arg("--enable-file-urls")
                .arg("--load-info-json")
                .arg(info);
        }
        None => {
            if job.rate_limit > 0 {
                cmd.args(["--limit-rate", &format!("{}K", job.rate_limit)]);
            }
            apply_cookies(&mut cmd, job.cookies_file.as_deref(), &job.cookies_browser);
            apply_js_runtime(&mut cmd, job.js_runtime.as_deref());
            cmd.args(youtube_split_args(&job.url, job.fragments));
            cmd.arg("--").arg(&job.url);
        }
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .kill_on_drop(true);
    hide_console(&mut cmd);
    #[cfg(unix)]
    cmd.process_group(0);

    let mut child = cmd.spawn().context("could not start yt-dlp")?;
    let pid = child.id();
    let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
    let stderr = child.stderr.take().ok_or_else(|| anyhow!("no stderr"))?;
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut tail: Vec<String> = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            tail.push(line);
            if tail.len() > 40 {
                tail.remove(0);
            }
        }
        tail.join("\n")
    });

    let mut lines = BufReader::new(stdout).lines();
    loop {
        let line = tokio::select! {
            _ = cancel.cancelled() => {
                kill_tree(&mut child, pid).await;
                return Ok(false);
            }
            l = lines.next_line() => l,
        };
        let Ok(Some(line)) = line else { break };
        if let Some(ev) = parse_progress(&line) {
            on_event(ev);
        } else if let Some(title) = line.strip_prefix("MDMTITLE:") {
            on_event(MediaEvent::Title(title.trim().to_string()));
        } else if let Some(size) = line.strip_prefix("MDMSIZE:") {
            if let Some(n) = exact_size(size) {
                on_event(MediaEvent::Size(n));
            }
        } else if let Some(file) = line.strip_prefix("MDMFILE:") {
            on_event(MediaEvent::File(PathBuf::from(file.trim())));
        } else if let Some(pp) = line.strip_prefix("MDMPP ") {
            let phase = post_phase(pp);
            on_event(MediaEvent::Phase(phase.into()));
        } else if line.starts_with("[Merger]") {
            on_event(MediaEvent::Phase("merging".into()));
        } else if line.starts_with("[ExtractAudio]") || line.starts_with("[VideoConvertor]") {
            on_event(MediaEvent::Phase("converting".into()));
        } else if line.starts_with("[Metadata]") {
            on_event(MediaEvent::Phase("tagging".into()));
        } else if line.starts_with("[EmbedThumbnail]") {
            on_event(MediaEvent::Phase("thumbnail".into()));
        } else if let Some(dest) = line.strip_prefix("[download] Destination:") {
            on_event(MediaEvent::Destination(PathBuf::from(dest.trim())));
            on_event(MediaEvent::Phase("downloading".into()));
        } else if let Some(dest) = line
            .strip_prefix("[Merger] Merging formats into ")
            .map(|d| d.trim().trim_matches('"'))
        {
            on_event(MediaEvent::Destination(PathBuf::from(dest)));
            on_event(MediaEvent::Phase("merging".into()));
        }
    }
    let status = tokio::select! {
        _ = cancel.cancelled() => {
            kill_tree(&mut child, pid).await;
            return Ok(false);
        }
        s = child.wait() => s?,
    };
    let stderr = stderr_task.await.unwrap_or_default();
    if status.success() {
        Ok(true)
    } else {
        bail!(last_error(&stderr))
    }
}

async fn kill_tree(child: &mut tokio::process::Child, pid: Option<u32>) {
    #[cfg(windows)]
    if let Some(pid) = pid {
        let mut cmd = Command::new("taskkill");
        cmd.args(["/T", "/F", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_console(&mut cmd);
        let _ = cmd.status().await;
    }
    #[cfg(unix)]
    if let Some(pid) = pid {
        let _ = Command::new("kill")
            .args(["-TERM", &format!("-{pid}")])
            .status()
            .await;
    }
    let _ = child.kill().await;
}

pub fn write_cookie_file(path: &Path, cookies: &[crate::model::BrowserCookie]) -> Result<()> {
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    for c in cookies {
        let include_sub = c.domain.starts_with('.');
        let domain = if c.http_only {
            format!("#HttpOnly_{}", c.domain)
        } else {
            c.domain.clone()
        };
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            domain,
            if include_sub { "TRUE" } else { "FALSE" },
            c.path,
            if c.secure { "TRUE" } else { "FALSE" },
            c.expiration_date.map(|e| e as i64).unwrap_or(0),
            c.name,
            c.value
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp4_prefers_codecs_every_player_supports() {
        assert_eq!(video_sort(None, "mp4"), "res,vcodec:h264,fps,acodec:aac");
        assert_eq!(
            video_sort(Some(1080), "mp4"),
            "res:1080,vcodec:h264,fps,acodec:aac"
        );
        assert_eq!(video_sort(None, "mkv"), "res,fps,ext:mp4:m4a");
        let args = format_args("best", true, "mp4");
        assert!(args.contains(&"res,vcodec:h264,fps,acodec:aac".to_string()));
        assert!(args.contains(&video_selector(None, "mp4")));
        assert_eq!(
            format_args("720", false, "mp4"),
            vec!["-f", "b[height<=720]/b"]
        );
    }

    #[test]
    fn classifies_failures() {
        assert_eq!(
            classify_failure("Postprocessing: Error opening input files: Invalid data found when processing input"),
            Failure::Corrupt
        );
        assert_eq!(
            classify_failure("ERROR: unable to download video data: <urlopen error [WinError 10054] An existing connection was forcibly closed>"),
            Failure::Network
        );
        assert_eq!(
            classify_failure("ERROR: unable to download video data: HTTP Error 403: Forbidden"),
            Failure::Network
        );
        assert_eq!(
            classify_failure("could not connect to https://x"),
            Failure::Network
        );
        assert_eq!(
            classify_failure("server answered HTTP 404 Not Found"),
            Failure::Other
        );
        assert_eq!(
            classify_failure("ERROR: [youtube] x: Video unavailable"),
            Failure::Other
        );
    }

    #[test]
    fn video_size_comes_from_exact_sizes_only() {
        let mut m = MediaTotals::default();
        assert_eq!(m.update(20, None, Some(2_900_000)), (20, None));
        assert_eq!(m.update(400, None, Some(1500)), (400, Some(1500)));
        assert_eq!(m.update(300, None, Some(1200)), (400, Some(1500)));
        assert_eq!(m.update(900, None, Some(1550)), (900, Some(1500)));
        assert_eq!(m.update(1480, None, Some(1600)), (1480, Some(1600)));
        assert_eq!(m.update(1550, None, Some(1600)), (1550, Some(1600)));

        let mut exact = MediaTotals::default();
        exact.set_exact(1000);
        assert_eq!(exact.update(100, None, Some(9000)), (100, Some(1000)));
        assert_eq!(exact.update(800, Some(800), None), (800, Some(1000)));
        assert_eq!(exact.update(10, None, Some(50)), (810, Some(1000)));
        assert_eq!(exact.update(200, Some(200), None), (1000, Some(1000)));

        let mut parts = MediaTotals::default();
        assert_eq!(parts.update(500, Some(500), None), (500, Some(500)));
        assert_eq!(parts.update(20, Some(100), None), (520, Some(600)));
        assert_eq!(parts.update(100, Some(100), None), (600, Some(600)));
    }

    #[test]
    fn reads_exact_sizes_only_when_every_part_has_one() {
        assert_eq!(exact_size("[700, 300]|[\"137\", \"140\"]|NA"), Some(1000));
        assert_eq!(exact_size("[300]|[\"270\", \"140\"]|NA"), None);
        assert_eq!(exact_size("NA|NA|4096"), Some(4096));
        assert_eq!(exact_size("NA|NA|NA"), None);
    }

    #[test]
    fn finds_videos_that_can_be_downloaded_with_several_connections() {
        let merged = serde_json::json!({
            "requested_formats": [
                { "format_id": "hd", "protocol": "https", "url": "https://cdn.example/v.mp4" },
                { "format_id": "a", "protocol": "https", "url": "https://cdn.example/a.m4a" }
            ]
        });
        assert_eq!(direct_formats(&merged).map(|f| f.len()), Some(2));
        let single = serde_json::json!({ "format_id": "720", "protocol": "https", "url": "https://cdn.example/v.mp4" });
        assert_eq!(direct_formats(&single).map(|f| f.len()), Some(1));
        let hls = serde_json::json!({ "format_id": "hls-1", "protocol": "m3u8_native", "url": "https://cdn.example/v.m3u8" });
        assert!(direct_formats(&hls).is_none());
        let live = serde_json::json!({ "format_id": "1", "protocol": "https", "url": "https://x/v", "is_live": true });
        assert!(direct_formats(&live).is_none());
        assert!(direct_formats(&serde_json::json!({ "_type": "playlist" })).is_none());
    }

    #[test]
    fn hands_downloaded_parts_back_to_yt_dlp() {
        let dir = std::env::temp_dir();
        let video = dir.join("v.part");
        let mut info = serde_json::json!({
            "formats": [
                { "format_id": "hd", "protocol": "https", "url": "https://cdn.example/v.mp4" },
                { "format_id": "sd", "protocol": "https", "url": "https://cdn.example/s.mp4" }
            ],
            "requested_formats": [
                { "format_id": "hd", "protocol": "https", "url": "https://cdn.example/v.mp4" }
            ]
        });
        use_local_files(&mut info, &[("hd".into(), video.clone())]);
        let expected = url::Url::from_file_path(&video).unwrap().to_string();
        assert_eq!(info["formats"][0]["url"], expected.as_str());
        assert_eq!(info["formats"][1]["url"], "https://cdn.example/s.mp4");
        assert_eq!(info["requested_formats"][0]["url"], expected.as_str());

        let headers = format_headers(&serde_json::json!({
            "http_headers": { "User-Agent": "UA", "Referer": "https://site.example/" },
            "cookies": "sid=abc; Domain=.site.example; Path=/; Secure; lang=en; Expires=123"
        }));
        assert_eq!(headers["user-agent"], "UA");
        assert_eq!(headers["cookie"], "sid=abc; lang=en");
    }

    #[test]
    fn splits_youtube_videos_into_parts_for_several_connections() {
        let yt = "https://www.youtube.com/watch?v=1";
        assert_eq!(
            youtube_split_args(yt, 8),
            &["--extractor-args", "youtube:formats=dashy"]
        );
        assert!(youtube_split_args(yt, 1).is_empty());
        assert!(youtube_split_args("https://vimeo.com/1", 8).is_empty());
    }

    #[test]
    fn reads_the_deno_version() {
        assert_eq!(
            parse_deno_version("deno 2.5.4 (stable, release, x86_64-pc-windows-msvc)\nv8 14.0\n")
                .as_deref(),
            Some("2.5.4")
        );
        assert_eq!(parse_deno_version("node v22"), None);
        assert!(deno_auto_install() == deno_asset().is_some());
    }

    #[test]
    fn recognizes_instagram_profiles() {
        assert!(instagram_profile("https://www.instagram.com/instagram/"));
        assert!(instagram_profile("https://instagram.com/natgeo"));
        assert!(instagram_profile("https://www.instagram.com/natgeo/reels/"));
        assert!(!instagram_profile(
            "https://www.instagram.com/reel/Chunk8-jurw/"
        ));
        assert!(!instagram_profile(
            "https://www.instagram.com/natgeo/reel/Chunk8-jurw/"
        ));
        assert!(!instagram_profile(
            "https://www.instagram.com/p/aye83DjauH/"
        ));
        assert!(!instagram_profile(
            "https://www.instagram.com/stories/natgeo/3412345678901234567/"
        ));
        assert!(!instagram_profile("https://www.instagram.com/explore/"));
        assert!(!instagram_profile("https://www.youtube.com/natgeo"));
        assert!(needs_sign_in(
            "ERROR: [instagram:story] You need to log in to access this content."
        ));
    }

    fn test_ffmpeg() -> Option<PathBuf> {
        let managed = std::env::var_os("APPDATA")
            .map(|a| {
                PathBuf::from(a)
                    .join("com.multron.downloadmanager")
                    .join("tools")
            })
            .filter(|d| d.join(exe("ffmpeg")).is_file());
        managed.or_else(|| util::which("ffmpeg").and_then(|p| p.parent().map(Path::to_path_buf)))
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "reads a YouTube video's formats"]
    async fn live_quality_sizes() {
        let tools = PathBuf::from(std::env::var("MULTRON_TOOLS_DIR").expect("MULTRON_TOOLS_DIR"));
        let url = std::env::var("MULTRON_TEST_URL").expect("MULTRON_TEST_URL");
        let info = Tools::new(tools, reqwest::Client::new())
            .probe(&url, None, "")
            .await
            .unwrap();
        for (id, s) in &info.sizes {
            println!(
                "size {id}: {}{:.1} MB",
                if s.exact { "" } else { "~" },
                s.size as f64 / 1048576.0
            );
        }
        assert!(info.sizes.contains_key("best"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    #[ignore = "downloads a YouTube video with the multi-connection engine"]
    async fn live_youtube_engine_qualities() {
        let tools = PathBuf::from(std::env::var("MULTRON_TOOLS_DIR").expect("MULTRON_TOOLS_DIR"));
        let out = PathBuf::from(std::env::var("MULTRON_TEST_DIR").expect("MULTRON_TEST_DIR"));
        let url = std::env::var("MULTRON_TEST_URL").expect("MULTRON_TEST_URL");
        let qualities = std::env::var("MULTRON_QUALITIES").unwrap_or_else(|_| "1080".into());
        let client = reqwest::Client::builder().http1_only().build().unwrap();
        for quality in qualities.split(',') {
            let dir = out.join(format!("engine-{quality}"));
            std::fs::create_dir_all(&dir).unwrap();
            let mut job = MediaJob {
                exe: tools.join(exe("yt-dlp")),
                url: url.clone(),
                dir: dir.clone(),
                quality: quality.into(),
                container: "mp4".into(),
                playlist: false,
                fragments: 8,
                rate_limit: 0,
                ffmpeg: Some(tools.clone()),
                cookies_file: None,
                cookies_browser: String::new(),
                embed_metadata: true,
                embed_thumbnail: false,
                filename: None,
                js_runtime: Some(tools.join(exe("deno"))),
                local_info: None,
            };
            let started = std::time::Instant::now();
            let mut info = extract(&job).await.unwrap();
            let formats = direct_formats(&info).expect("formats are not direct");
            let found = started.elapsed().as_secs_f64();
            let mut local = Vec::new();
            let mut bytes = 0u64;
            let dl = std::time::Instant::now();
            for f in &formats {
                let id = f["format_id"].as_str().unwrap().to_string();
                let headers = format_headers(f);
                let probe = crate::engine::probe(&client, f["url"].as_str().unwrap(), &headers)
                    .await
                    .unwrap();
                let part = dir.join(format!(".f{id}.mdmpart"));
                let progress = crate::engine::Progress::new(probe.total, &[], 8);
                let done = crate::engine::run(Arc::new(crate::engine::Job {
                    client: client.clone(),
                    url: probe.final_url.clone(),
                    headers,
                    part: part.clone(),
                    resumable: probe.resumable,
                    progress: progress.clone(),
                    cancel: CancellationToken::new(),
                    limiters: Vec::new(),
                    retries: 4,
                    transform: None,
                    if_range: probe.validator.clone(),
                    max_request: Some(8 << 20),
                }))
                .await
                .unwrap();
                assert!(done);
                bytes += std::fs::metadata(&part).unwrap().len();
                local.push((id, part));
            }
            let secs = dl.elapsed().as_secs_f64();
            use_local_files(&mut info, &local);
            let info_path = dir.join(".info.json");
            std::fs::write(&info_path, serde_json::to_vec(&info).unwrap()).unwrap();
            job.local_info = Some(info_path);
            let mut file = None;
            let finish = std::time::Instant::now();
            assert!(run(job, CancellationToken::new(), |ev| {
                if let MediaEvent::File(f) = ev {
                    file = Some(f);
                }
            })
            .await
            .unwrap());
            let file = file.expect("no file");
            println!(
                "{quality}: formats {:?}, found in {found:.1}s, {:.1} MB downloaded in {secs:.1}s = {:.2} MB/s, joined in {:.1}s, file {:.1} MB",
                local.iter().map(|(i, _)| i.as_str()).collect::<Vec<_>>(),
                bytes as f64 / 1048576.0,
                bytes as f64 / 1048576.0 / secs,
                finish.elapsed().as_secs_f64(),
                std::fs::metadata(&file).unwrap().len() as f64 / 1048576.0
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "downloads a YouTube video in every quality"]
    async fn live_youtube_qualities() {
        let tools = PathBuf::from(std::env::var("MULTRON_TOOLS_DIR").expect("MULTRON_TOOLS_DIR"));
        let out = PathBuf::from(std::env::var("MULTRON_TEST_DIR").expect("MULTRON_TEST_DIR"));
        let url = std::env::var("MULTRON_TEST_URL")
            .unwrap_or_else(|_| "https://www.youtube.com/watch?v=o6ALnu6XZEo".into());
        let qualities = std::env::var("MULTRON_QUALITIES")
            .unwrap_or_else(|_| "best,1080,720,480,audio-m4a,audio-mp3".into());
        for quality in qualities.split(',') {
            let dir = out.join(quality);
            std::fs::create_dir_all(&dir).unwrap();
            let job = MediaJob {
                exe: tools.join(exe("yt-dlp")),
                url: url.clone(),
                dir: dir.clone(),
                quality: quality.into(),
                container: std::env::var("MULTRON_CONTAINER").unwrap_or_else(|_| "mp4".into()),
                playlist: false,
                fragments: 8,
                rate_limit: 0,
                ffmpeg: Some(tools.clone()),
                cookies_file: None,
                cookies_browser: String::new(),
                embed_metadata: true,
                embed_thumbnail: false,
                filename: None,
                js_runtime: Some(tools.join(exe("deno"))),
                local_info: None,
            };
            let mut totals = MediaTotals::default();
            let mut exact = None;
            let mut seen_totals: Vec<u64> = Vec::new();
            let mut last_shown = 0u64;
            let mut backwards = 0;
            let mut first_total_at: Option<u64> = None;
            let mut file = None;
            let mut phases: Vec<String> = Vec::new();
            let started = std::time::Instant::now();
            let ok = run(job, CancellationToken::new(), |ev| match ev {
                MediaEvent::Size(n) => {
                    exact = Some(n);
                    totals.set_exact(n);
                }
                MediaEvent::Progress {
                    downloaded,
                    total,
                    estimate,
                    ..
                } => {
                    let (shown, size) = totals.update(downloaded, total, estimate);
                    if shown < last_shown {
                        backwards += 1;
                    }
                    last_shown = shown;
                    if let Some(t) = size {
                        if first_total_at.is_none() {
                            first_total_at = Some(shown);
                        }
                        if seen_totals.last() != Some(&t) {
                            seen_totals.push(t);
                        }
                    }
                }
                MediaEvent::File(f) => file = Some(f),
                MediaEvent::Phase(p) if phases.last() != Some(&p) => phases.push(p),
                _ => {}
            })
            .await
            .unwrap();
            let secs = started.elapsed().as_secs_f64();
            let file = file.expect("no file reported");
            let len = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
            println!(
                "{quality}: ok={ok} file={:?} {:.1} MB in {secs:.1}s, exact={exact:?}, totals shown={seen_totals:?}, first total after {:?} bytes, backwards={backwards}, steps={phases:?}",
                file.file_name().unwrap_or_default(),
                len as f64 / 1048576.0,
                first_total_at
            );
            assert!(ok && len > 0, "{quality} produced no file");
            assert_eq!(backwards, 0, "{quality} went backwards");
            assert!(
                seen_totals.len() <= 1,
                "{quality} total changed: {seen_totals:?}"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "downloads Deno from GitHub"]
    async fn live_installs_deno() {
        let dir = PathBuf::from(std::env::var("MULTRON_TOOLS_DIR").expect("MULTRON_TOOLS_DIR"));
        let tools = Tools::new(dir, reqwest::Client::new());
        let version = tools
            .install_deno(|_| {}, &CancellationToken::new())
            .await
            .unwrap();
        println!("deno {version} at {:?}", tools.deno_location());
        assert!(tools.managed_deno().is_file());
        let status = tools.status().await;
        assert_eq!(status.deno_version.as_deref(), Some(version.as_str()));
    }

    #[tokio::test]
    async fn joins_separate_picture_and_sound_into_one_file() {
        let Some(ffmpeg) = test_ffmpeg() else { return };
        let dir = std::env::temp_dir().join(format!("mdm-merge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let make = |args: &[&str], out: &str| {
            let ok = std::process::Command::new(ffmpeg.join(exe("ffmpeg")))
                .args(["-hide_banner", "-loglevel", "error", "-y"])
                .args(args)
                .arg(dir.join(out))
                .status()
                .unwrap()
                .success();
            assert!(ok, "could not make {out}");
            std::fs::read(dir.join(out)).unwrap()
        };
        let video = make(
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=160x120:rate=10",
                "-an",
                "-c:v",
                "mpeg4",
            ],
            "v.mp4",
        );
        let audio = make(
            &[
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-vn",
                "-c:a",
                "aac",
            ],
            "a.m4a",
        );
        let seen = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let log = seen.clone();
        let app = axum::Router::new()
            .route(
                "/v.mp4",
                axum::routing::get(move || async move { video.clone() }),
            )
            .route(
                "/a.m4a",
                axum::routing::get(move || async move { audio.clone() }),
            )
            .layer(axum::middleware::from_fn(
                move |req: axum::extract::Request, next: axum::middleware::Next| {
                    let log = log.clone();
                    async move {
                        let referer = req
                            .headers()
                            .get("referer")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("")
                            .to_string();
                        log.lock().unwrap().push(referer);
                        next.run(req).await
                    }
                },
            ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let dest = dir.join("someone_story_1.mp4");
        let mut files = Vec::new();
        let done = merge(
            &ffmpeg,
            &format!("{base}/v.mp4"),
            &format!("{base}/a.m4a"),
            Some("https://www.instagram.com/stories/someone/1/"),
            Some("Mozilla/5.0 test"),
            &dest,
            CancellationToken::new(),
            |ev| {
                if let MediaEvent::File(p) = ev {
                    files.push(p);
                }
            },
        )
        .await
        .unwrap();
        assert!(done);
        assert_eq!(files, vec![dest.clone()]);
        assert!(!dest.with_extension("part.mp4").exists());
        let probe = std::process::Command::new(ffmpeg.join(exe("ffprobe")))
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "csv=p=0",
            ])
            .arg(&dest)
            .output()
            .unwrap();
        let streams = String::from_utf8_lossy(&probe.stdout);
        assert!(
            streams.contains("video") && streams.contains("audio"),
            "{streams}"
        );
        assert!(seen
            .lock()
            .unwrap()
            .iter()
            .all(|r| r == "https://www.instagram.com/stories/someone/1/"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn decides_when_cookies_help() {
        assert!(is_youtube("https://www.youtube.com/watch?v=1"));
        assert!(is_youtube("https://youtu.be/1"));
        assert!(is_youtube("https://music.youtube.com/watch?v=1"));
        assert!(!is_youtube("https://notyoutube.com/watch?v=1"));
        assert!(cookies_rejected(
            "ERROR: [youtube] 0jZHOv2BxdU: The page needs to be reloaded."
        ));
        assert!(!cookies_rejected("ERROR: HTTP Error 404"));
        assert!(needs_sign_in(
            "ERROR: [youtube] x: Sign in to confirm your age."
        ));
        assert!(needs_sign_in("Sign in to confirm you’re not a bot"));
        assert!(needs_sign_in(
            "ERROR: Private video. Sign in if you have been granted access"
        ));
        assert!(!needs_sign_in(
            "ERROR: [youtube] x: The page needs to be reloaded."
        ));
    }

    #[test]
    fn output_names_are_safe_file_names() {
        assert_eq!(
            output_name("That small heart attack."),
            "That small heart attack"
        );
        assert_eq!(output_name("a/b: c? 100%"), "a_b_ c_ 100%%");
        assert_eq!(output_name("   "), "");
    }

    #[test]
    fn names_each_processing_step() {
        assert_eq!(post_phase("Merger started"), "merging");
        assert_eq!(post_phase("ExtractAudio processing"), "converting");
        assert_eq!(post_phase("FFmpegMetadata started"), "tagging");
        assert_eq!(post_phase("EmbedThumbnail started"), "thumbnail");
        assert_eq!(post_phase("MoveFiles started"), "finishing");
    }

    #[test]
    fn works_out_the_size_of_every_quality() {
        let info = serde_json::json!({
            "duration": 100.0,
            "formats": [
                { "format_id": "140", "vcodec": "none", "acodec": "mp4a.40.2", "abr": 128, "filesize": 1000 },
                { "format_id": "251", "vcodec": "none", "acodec": "opus", "abr": 140, "filesize": 1100 },
                { "format_id": "137", "vcodec": "avc1.640028", "acodec": "none", "height": 1080, "fps": 30, "filesize": 30000 },
                { "format_id": "299", "vcodec": "avc1.64002a", "acodec": "none", "height": 1080, "fps": 60, "filesize": 50000 },
                { "format_id": "313", "vcodec": "vp9", "acodec": "none", "height": 2160, "fps": 30, "filesize_approx": 200000 },
                { "format_id": "136", "vcodec": "avc1.4d401f", "acodec": "none", "height": 720, "fps": 30, "filesize": 15000 }
            ]
        });
        let sizes = quality_sizes(&info);
        assert_eq!(
            sizes["best"],
            QualitySize {
                size: 201000,
                exact: false
            }
        );
        assert_eq!(sizes["4320"], sizes["best"]);
        assert_eq!(
            sizes["1080"],
            QualitySize {
                size: 51000,
                exact: true
            }
        );
        assert_eq!(
            sizes["720"],
            QualitySize {
                size: 16000,
                exact: true
            }
        );
        assert_eq!(
            sizes["144"],
            QualitySize {
                size: 16000,
                exact: true
            }
        );
        assert_eq!(
            sizes["audio-m4a"],
            QualitySize {
                size: 1000,
                exact: true
            }
        );
        assert_eq!(sizes["audio-opus"].size, 1100);
        assert_eq!(sizes["audio-mp3"].size, 3_062_500);
    }

    #[test]
    fn converts_audio_to_every_format() {
        for f in ["aac", "flac", "wav", "alac", "vorbis", "opus", "m4a"] {
            let args = format_args(&format!("audio-{f}"), true, "mp4");
            let at = args.iter().position(|a| a == "--audio-format").unwrap();
            assert_eq!(args[at + 1], f);
            assert!(args.contains(&"-x".to_string()));
        }
        assert_eq!(
            format_args("audio-wav", false, "mp4"),
            vec!["-f", "ba[ext=m4a]/ba/b"]
        );
        assert!(
            format_args("audio-alac", true, "mp4").contains(&"ExtractAudio:-c:a alac".to_string())
        );
    }

    #[test]
    fn webm_keeps_to_formats_webm_can_hold() {
        let args = format_args("best", true, "webm");
        assert!(args.contains(&"res,fps,ext:webm:webm".to_string()));
        assert!(args.iter().any(
            |a| a.contains("bv*[height>=2160][ext=webm][filesize>0]+ba[ext=webm][filesize>0]")
        ));
        assert!(args.iter().any(|a| a.ends_with("bv*+ba/b")));
        assert!(format_args("best", true, "mov")
            .contains(&"res,vcodec:h264,fps,acodec:aac".to_string()));
        assert!(format_args("best", true, "mov").contains(&"mov".to_string()));
    }

    #[test]
    fn plays_everywhere_mp4_takes_only_h264_and_aac() {
        let args = format_args("best", true, "mp4-h264");
        let selector = &args[1];
        assert!(selector.starts_with(
            "bv*[height>=6480][vcodec^=avc][filesize>0]+ba[acodec^=mp4a][filesize>0]"
        ));
        assert!(selector.ends_with("bv*+ba/b"));
        let at = args
            .iter()
            .position(|a| a == "--merge-output-format")
            .unwrap();
        assert_eq!(args[at + 1], "mp4");
        assert!(format_args("1080", true, "mp4")
            .contains(&"res:1080,vcodec:h264,fps,acodec:aac".to_string()));
    }

    #[test]
    fn opus_stays_audio_without_ffmpeg() {
        assert_eq!(
            format_args("audio-opus", false, "mp4"),
            vec!["-f", "ba[ext=m4a]/ba/b"]
        );
        assert!(format_args("audio-opus", true, "mp4").contains(&"opus".to_string()));
    }

    #[test]
    fn picks_the_highest_resolution_before_a_known_size() {
        let best = video_selector(None, "mp4");
        assert!(
            best.starts_with("bv*[height>=6480][filesize>0]+ba[filesize>0]/bv*[height>=6480]+ba/")
        );
        let p4320 = best.find("[height>=4320]").unwrap();
        let p1080 = best.find("[height>=1080]").unwrap();
        assert!(p4320 < p1080);
        let capped = video_selector(Some(2160), "mp4");
        assert!(!capped.contains("height>=4320"));
        assert!(capped.starts_with("bv*[height>=2160][height<=?2160][filesize>0]"));
        assert!(capped.ends_with("bv*+ba/b"));
    }

    #[test]
    fn progress_lines() {
        match parse_progress("[MDM] 1024 NA 4096.0 512.5 3 2 10") {
            Some(MediaEvent::Progress {
                downloaded,
                total,
                estimate,
                speed,
                eta,
                index,
                count,
            }) => {
                assert_eq!(downloaded, 1024);
                assert_eq!(total, None);
                assert_eq!(estimate, Some(4096));
                assert_eq!(speed, Some(512));
                assert_eq!(eta, Some(3));
                assert_eq!(index, Some(2));
                assert_eq!(count, Some(10));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(parse_progress("[download] 10%").is_none());
    }

    #[test]
    fn formats() {
        let a = format_args("1080", true, "mkv");
        assert!(a.contains(&"res:1080,fps,ext:mp4:m4a".to_string()));
        let b = format_args("720", false, "mp4");
        assert_eq!(b, vec!["-f", "b[height<=720]/b"]);
    }
}
