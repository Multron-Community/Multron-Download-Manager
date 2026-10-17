use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::updates::{ReleaseAsset, REPO};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyUpdate {
    pub version: String,
    pub path: PathBuf,
    #[serde(skip)]
    kind: PackageKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageKind {
    WindowsSetup,
    Dmg,
    AppImage,
    Deb,
    Rpm,
}

fn kind_of(name: &str) -> Option<PackageKind> {
    let n = name.to_ascii_lowercase();
    if n.ends_with("-setup.exe") {
        Some(PackageKind::WindowsSetup)
    } else if n.ends_with(".dmg") {
        Some(PackageKind::Dmg)
    } else if n.ends_with(".appimage") {
        Some(PackageKind::AppImage)
    } else if n.ends_with(".deb") {
        Some(PackageKind::Deb)
    } else if n.ends_with(".rpm") {
        Some(PackageKind::Rpm)
    } else {
        None
    }
}

fn wanted_kind() -> Option<PackageKind> {
    if cfg!(windows) {
        Some(PackageKind::WindowsSetup)
    } else if cfg!(target_os = "macos") {
        Some(PackageKind::Dmg)
    } else if std::env::var_os("APPIMAGE").is_some() {
        Some(PackageKind::AppImage)
    } else if Path::new("/usr/bin/dpkg").exists() {
        Some(PackageKind::Deb)
    } else if Path::new("/usr/bin/rpm").exists() {
        Some(PackageKind::Rpm)
    } else {
        None
    }
}

fn arch_names() -> &'static [&'static str] {
    match std::env::consts::ARCH {
        "x86_64" => &["x64", "x86_64", "amd64"],
        "aarch64" => &["aarch64", "arm64"],
        _ => &[],
    }
}

pub fn pick_asset(assets: &[ReleaseAsset]) -> Option<&ReleaseAsset> {
    let kind = wanted_kind()?;
    let arch = arch_names();
    assets.iter().find(|a| {
        let lower = a.name.to_ascii_lowercase();
        kind_of(&a.name) == Some(kind) && arch.iter().any(|x| lower.contains(x))
    })
}

fn update_dir() -> PathBuf {
    std::env::temp_dir().join("multron-update")
}

pub async fn download(
    client: &reqwest::Client,
    version: &str,
    asset: &ReleaseAsset,
    sums_url: Option<&str>,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<ReadyUpdate> {
    let kind = kind_of(&asset.name).ok_or_else(|| anyhow!("unsupported update package"))?;
    let expected = match sums_url {
        Some(url) => {
            let sums = client
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            Some(
                expected_sha256(&sums, &asset.name)
                    .ok_or_else(|| anyhow!("{} is not listed in SHA256SUMS.txt", asset.name))?,
            )
        }
        None => None,
    };

    let dir = update_dir();
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join(&asset.name);
    let part = dir.join(format!("{}.part", asset.name));
    let resp = client
        .get(&asset.browser_download_url)
        .send()
        .await?
        .error_for_status()
        .context("could not download the update")?;
    let total = resp
        .content_length()
        .or(Some(asset.size).filter(|s| *s > 0));
    let mut file = tokio::fs::File::create(&part).await?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        progress(done, total);
    }
    file.flush().await?;
    drop(file);

    let actual = hex(&hasher.finalize());
    if let Some(expected) = expected {
        if !actual.eq_ignore_ascii_case(&expected) {
            let _ = tokio::fs::remove_file(&part).await;
            bail!("the update failed its SHA-256 check");
        }
    }
    tokio::fs::rename(&part, &path).await?;
    Ok(ReadyUpdate {
        version: version.to_string(),
        path,
        kind,
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn expected_sha256(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let file = parts.next()?.trim_start_matches('*');
        (file == name && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    })
}

pub fn install(update: &ReadyUpdate, relaunch: bool) -> Result<()> {
    let pkg = update.path.to_string_lossy().into_owned();
    match update.kind {
        PackageKind::WindowsSetup => {
            let mut cmd = Command::new(&update.path);
            cmd.args([
                "/VERYSILENT",
                "/SUPPRESSMSGBOXES",
                "/NORESTART",
                "/CLOSEAPPLICATIONS",
            ]);
            if relaunch {
                cmd.arg("/relaunch=1");
            }
            detach(&mut cmd);
            cmd.spawn().context("could not start the installer")?;
        }
        PackageKind::Dmg => {
            let exe = std::env::current_exe()?;
            let bundle = exe
                .ancestors()
                .find(|p| p.extension().is_some_and(|e| e == "app"))
                .ok_or_else(|| anyhow!("not running from an .app bundle"))?
                .to_path_buf();
            let script = format!(
                r#"sleep 2
mnt=$(mktemp -d)
hdiutil attach -nobrowse -quiet -mountpoint "$mnt" {dmg} || exit 1
app=$(ls -d "$mnt"/*.app | head -n 1)
if [ -n "$app" ]; then
  rm -rf {bundle}.old && mv {bundle} {bundle}.old && ditto "$app" {bundle} && rm -rf {bundle}.old || mv {bundle}.old {bundle}
fi
hdiutil detach -quiet "$mnt"
{relaunch}"#,
                dmg = sh_quote(&pkg),
                bundle = sh_quote(&bundle.to_string_lossy()),
                relaunch = if relaunch {
                    format!("open {}", sh_quote(&bundle.to_string_lossy()))
                } else {
                    String::new()
                },
            );
            spawn_sh(&script)?;
        }
        PackageKind::AppImage => {
            let target = std::env::var("APPIMAGE").context("APPIMAGE is not set")?;
            let script = format!(
                "sleep 2\ncp -f {pkg} {target}.new && chmod +x {target}.new && mv -f {target}.new {target}\n{relaunch}",
                pkg = sh_quote(&pkg),
                target = sh_quote(&target),
                relaunch = if relaunch { format!("{} &", sh_quote(&target)) } else { String::new() },
            );
            spawn_sh(&script)?;
        }
        PackageKind::Deb | PackageKind::Rpm => {
            let installer = if update.kind == PackageKind::Deb {
                format!("pkexec apt-get install -y {}", sh_quote(&pkg))
            } else {
                format!("pkexec dnf install -y {}", sh_quote(&pkg))
            };
            let exe = std::env::current_exe()?;
            let script = format!(
                "sleep 2\n{installer}\n{relaunch}",
                relaunch = if relaunch {
                    format!("{} &", sh_quote(&exe.to_string_lossy()))
                } else {
                    String::new()
                },
            );
            spawn_sh(&script)?;
        }
    }
    Ok(())
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn spawn_sh(script: &str) -> Result<()> {
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c").arg(script);
    detach(&mut cmd);
    cmd.spawn().context("could not start the update script")?;
    Ok(())
}

fn detach(cmd: &mut Command) {
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
}

static READY: std::sync::Mutex<Option<ReadyUpdate>> = std::sync::Mutex::new(None);
static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static INSTALLING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub const EVT_READY: &str = "app://update-ready";

pub const EVT_MANDATORY: &str = "app://mandatory-update";

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MandatoryStatus {
    pub version: String,
    pub phase: String,
    pub error: Option<String>,
}

static MANDATORY: std::sync::Mutex<Option<MandatoryStatus>> = std::sync::Mutex::new(None);

pub fn mandatory_status() -> Option<MandatoryStatus> {
    MANDATORY.lock().ok()?.clone()
}

fn set_mandatory(app: &tauri::AppHandle, status: Option<MandatoryStatus>) {
    use tauri::Emitter;
    if let Ok(mut m) = MANDATORY.lock() {
        *m = status.clone();
    }
    let _ = app.emit(EVT_MANDATORY, status);
}

pub async fn mandatory_update(app: &tauri::AppHandle, client: &reqwest::Client) {
    let info = crate::updates::check_app(client).await;
    if info.error.is_some() || !info.has_update || info.asset.is_none() {
        return;
    }
    let version = info.latest.clone().unwrap_or_default();
    let status = |phase: &str, error: Option<String>| MandatoryStatus {
        version: version.clone(),
        phase: phase.into(),
        error,
    };
    let mut last_error = None;
    for attempt in 1..=3u64 {
        set_mandatory(app, Some(status("downloading", last_error.clone())));
        match fetch_latest(app, client).await {
            Ok(Some(_)) => {
                set_mandatory(app, Some(status("installing", None)));
                tokio::time::sleep(std::time::Duration::from_millis(900)).await;
                match install_ready(true) {
                    Ok(true) => {
                        app.exit(0);
                        return;
                    }
                    Ok(false) => last_error = Some("the update could not be started".into()),
                    Err(e) => last_error = Some(format!("{e:#}")),
                }
            }
            Ok(None) => {
                set_mandatory(app, None);
                return;
            }
            Err(e) => last_error = Some(format!("{e:#}")),
        }
        tracing::warn!("mandatory update attempt {attempt} failed: {last_error:?}");
        tokio::time::sleep(std::time::Duration::from_secs(4 * attempt)).await;
    }
    set_mandatory(app, Some(status("failed", last_error)));
}

pub fn ready() -> Option<ReadyUpdate> {
    READY.lock().ok()?.clone()
}

pub async fn fetch_latest(
    app: &tauri::AppHandle,
    client: &reqwest::Client,
) -> Result<Option<ReadyUpdate>> {
    use std::sync::atomic::Ordering;
    use tauri::Emitter;
    let info = crate::updates::check_app(client).await;
    if let Some(e) = info.error {
        bail!("update check failed: {e}");
    }
    let Some(latest) = info.latest.filter(|_| info.has_update) else {
        return Ok(None);
    };
    if let Some(r) = ready().filter(|r| r.version == latest) {
        return Ok(Some(r));
    }
    let asset = info.asset.ok_or_else(|| {
        anyhow!(
            "no update package for this system; download it from {}",
            releases_page()
        )
    })?;
    if BUSY.swap(true, Ordering::SeqCst) {
        bail!("the update is already downloading");
    }
    let label = format!("Multron Download Manager {latest}");
    let emit = |downloaded: u64, total: Option<u64>, done: bool| {
        let _ = app.emit(
            crate::manager::EVT_TOOLS,
            crate::media::ToolProgress {
                tool: label.clone(),
                downloaded,
                total,
                done,
            },
        );
    };
    let result = download(client, &latest, &asset, info.sums_url.as_deref(), |d, t| {
        emit(d, t, false)
    })
    .await;
    emit(0, None, true);
    BUSY.store(false, Ordering::SeqCst);
    let update = result?;
    if let Ok(mut r) = READY.lock() {
        *r = Some(update.clone());
    }
    let _ = app.emit(EVT_READY, &update);
    Ok(Some(update))
}

pub fn install_ready(relaunch: bool) -> Result<bool> {
    use std::sync::atomic::Ordering;
    let Some(update) = ready() else {
        return Ok(false);
    };
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    install(&update, relaunch)?;
    Ok(true)
}

pub fn sums_url(assets: &[ReleaseAsset]) -> Option<String> {
    assets
        .iter()
        .find(|a| a.name == "SHA256SUMS.txt")
        .map(|a| a.browser_download_url.clone())
}

pub fn releases_page() -> String {
    format!("https://github.com/{REPO}/releases/latest")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            name: name.into(),
            browser_download_url: format!("https://example.com/{name}"),
            size: 1,
        }
    }

    #[test]
    fn reads_sha256sums() {
        let sums = "abc  other.zip\n0123456789abcdef0123456789abcdef0123456789abcdef0123456789ABCDEF *Multron-Download-Manager-0.3.0-x64-Setup.exe\n";
        assert_eq!(
            expected_sha256(sums, "Multron-Download-Manager-0.3.0-x64-Setup.exe").as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        assert_eq!(expected_sha256(sums, "missing.exe"), None);
    }

    #[test]
    fn picks_the_package_for_this_system() {
        let assets = [
            asset("Multron-Download-Manager-0.3.0-x64-Setup.exe"),
            asset("Multron.Download.Manager_0.3.0_aarch64.dmg"),
            asset("Multron.Download.Manager_0.3.0_x64.dmg"),
            asset("Multron.Download.Manager_0.3.0_amd64.deb"),
            asset("Multron.Download.Manager-0.3.0-1.x86_64.rpm"),
            asset("multron-browser-extension-0.3.0.zip"),
            asset("SHA256SUMS.txt"),
        ];
        let picked = pick_asset(&assets).map(|a| a.name.as_str());
        if cfg!(all(windows, target_arch = "x86_64")) {
            assert_eq!(picked, Some("Multron-Download-Manager-0.3.0-x64-Setup.exe"));
        }
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert_eq!(picked, Some("Multron.Download.Manager_0.3.0_aarch64.dmg"));
        }
        assert_eq!(
            sums_url(&assets).as_deref(),
            Some("https://example.com/SHA256SUMS.txt")
        );
    }
}
