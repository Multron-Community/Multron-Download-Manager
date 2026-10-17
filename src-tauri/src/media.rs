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
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgress {
    pub tool: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub done: bool,
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
        ToolStatus {
            ytdlp_installed: installed,
            ytdlp_version: version,
            ytdlp_path: path.to_string_lossy().into_owned(),
            ffmpeg_available: ffmpeg.is_some(),
            ffmpeg_path: ffmpeg.map(|p| p.to_string_lossy().into_owned()),
            ffmpeg_managed: self.managed_ffmpeg().is_file(),
            ffmpeg_auto_install: ffmpeg_auto_install(),
        }
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
        });
        let mut run = tokio::spawn(engine::run(job));
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        let report = |done: bool| ToolProgress {
            tool: label.to_string(),
            downloaded: progress.downloaded(),
            total: progress.total(),
            done,
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
    }
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
}

#[derive(Debug, Clone)]
pub enum MediaEvent {
    Progress {
        downloaded: u64,
        total: Option<u64>,
        speed: Option<u64>,
        eta: Option<u64>,
        index: Option<u32>,
        count: Option<u32>,
    },
    Phase(String),
    Title(String),
    File(PathBuf),
    Destination(PathBuf),
}

const PROGRESS_PREFIX: &str = "[MDM]";

fn video_sort(height: Option<u32>, container: &str) -> String {
    let mp4 = container.eq_ignore_ascii_case("mp4");
    match (height, mp4) {
        (Some(h), true) => format!("res:{h},vcodec:h264,acodec:aac"),
        (None, true) => "vcodec:h264,res,acodec:aac".into(),
        (Some(h), false) => format!("res:{h},ext:mp4:m4a"),
        (None, false) => "res,ext:mp4:m4a".into(),
    }
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
        "audio-m4a" | "audio-mp3" | "audio" => {
            if has_ffmpeg {
                s(&["-f", "ba[ext=m4a]/ba/b", "-x", "--audio-format", "m4a"])
            } else {
                s(&["-f", "ba[ext=m4a]/ba/b"])
            }
        }
        "audio-opus" if has_ffmpeg => s(&["-f", "ba/b", "-x", "--audio-format", "opus"]),
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
                "bv*+ba/b".into(),
                "-S".into(),
                video_sort(height, container),
                "--merge-output-format".into(),
                container.into(),
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

fn parse_progress(line: &str) -> Option<MediaEvent> {
    let rest = line.split_once(PROGRESS_PREFIX)?.1;
    let f: Vec<&str> = rest.split_whitespace().collect();
    let downloaded = parse_num(f.first().copied())? as u64;
    let total = parse_num(f.get(1).copied()).or(parse_num(f.get(2).copied()));
    Some(MediaEvent::Progress {
        downloaded,
        total: total.map(|t| t as u64),
        speed: parse_num(f.get(3).copied()).map(|v| v as u64),
        eta: parse_num(f.get(4).copied()).map(|v| v as u64),
        index: parse_num(f.get(5).copied()).map(|v| v as u32),
        count: parse_num(f.get(6).copied()).map(|v| v as u32),
    })
}

pub async fn run(
    job: MediaJob,
    cancel: CancellationToken,
    mut on_event: impl FnMut(MediaEvent),
) -> Result<bool> {
    let mut cmd = Command::new(&job.exe);
    cmd.args(["--encoding", "utf-8"]);
    let named = job
        .filename
        .as_deref()
        .map(output_name)
        .filter(|n| !n.is_empty());
    let template = match (&named, job.playlist) {
        (_, true) => {
            "%(playlist_title|Playlist).120B/%(playlist_index)03d - %(title).150B [%(id)s].%(ext)s"
                .to_string()
        }
        (Some(name), false) => format!("{name}.%(ext)s"),
        (None, false) => "%(title).180B [%(id)s].%(ext)s".to_string(),
    };
    cmd.args(["--newline", "--progress", "--no-colors", "--continue", "--no-mtime", "--windows-filenames"])
        .args(["--progress-template", &format!(
            "download:{PROGRESS_PREFIX} %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s %(info.playlist_index)s %(info.n_entries)s"
        )])
        .args(["--progress-template", "postprocess:MDMPP %(progress.postprocessor)s %(progress.status)s"])
        .args(["--print", "before_dl:MDMTITLE:%(title)s"])
        .args(["--print", "after_move:MDMFILE:%(filepath)s"])
        .args(["--concurrent-fragments", &job.fragments.clamp(1, 32).to_string()])
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
    if job.rate_limit > 0 {
        cmd.args(["--limit-rate", &format!("{}K", job.rate_limit)]);
    }
    apply_cookies(&mut cmd, job.cookies_file.as_deref(), &job.cookies_browser);
    cmd.arg("--").arg(&job.url);
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
        } else if let Some(file) = line.strip_prefix("MDMFILE:") {
            on_event(MediaEvent::File(PathBuf::from(file.trim())));
        } else if let Some(pp) = line.strip_prefix("MDMPP ") {
            let phase = if pp.starts_with("Merger") {
                "merging"
            } else if pp.contains("ExtractAudio") || pp.contains("VideoConvertor") {
                "converting"
            } else {
                "tagging"
            };
            on_event(MediaEvent::Phase(phase.into()));
        } else if line.starts_with("[Merger]") {
            on_event(MediaEvent::Phase("merging".into()));
        } else if line.starts_with("[ExtractAudio]") || line.starts_with("[VideoConvertor]") {
            on_event(MediaEvent::Phase("converting".into()));
        } else if line.starts_with("[Metadata]") || line.starts_with("[EmbedThumbnail]") {
            on_event(MediaEvent::Phase("tagging".into()));
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
        assert_eq!(video_sort(None, "mp4"), "vcodec:h264,res,acodec:aac");
        assert_eq!(
            video_sort(Some(1080), "mp4"),
            "res:1080,vcodec:h264,acodec:aac"
        );
        assert_eq!(video_sort(None, "mkv"), "res,ext:mp4:m4a");
        let args = format_args("best", true, "mp4");
        assert!(args.contains(&"vcodec:h264,res,acodec:aac".to_string()));
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
    fn progress_lines() {
        match parse_progress("[MDM] 1024 NA 4096.0 512.5 3 2 10") {
            Some(MediaEvent::Progress {
                downloaded,
                total,
                speed,
                eta,
                index,
                count,
            }) => {
                assert_eq!(downloaded, 1024);
                assert_eq!(total, Some(4096));
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
        assert!(a.contains(&"res:1080,ext:mp4:m4a".to_string()));
        let b = format_args("720", false, "mp4");
        assert_eq!(b, vec!["-f", "b[height<=720]/b"]);
    }
}
