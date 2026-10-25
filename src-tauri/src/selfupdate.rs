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
    #[serde(skip)]
    sha256: String,
}

const MAX_UPDATE_BYTES: u64 = 1 << 30;
const UPDATE_PUBLIC_KEY: &str = include_str!("../update-key.pub");
const MANIFEST_APP: &str = "multron-download-manager";
pub const MANIFEST_NAME: &str = "update.json";
pub const SIGNATURE_NAME: &str = "update.json.sig";
const TRUSTED_HOSTS: &[&str] = &["github.com", "api.github.com"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageKind {
    WindowsSetup,
    WindowsPortable,
    Dmg,
    AppImage,
    Deb,
    Rpm,
}

fn kind_of(name: &str) -> Option<PackageKind> {
    let n = name.to_ascii_lowercase();
    if n.ends_with("-setup.exe") {
        Some(PackageKind::WindowsSetup)
    } else if n.ends_with("-portable.zip") {
        Some(PackageKind::WindowsPortable)
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
    if cfg!(windows) && crate::portable::dir().is_some() {
        Some(PackageKind::WindowsPortable)
    } else if cfg!(windows) {
        Some(PackageKind::WindowsSetup)
    } else if cfg!(target_os = "macos") {
        Some(PackageKind::Dmg)
    } else if running_as_appimage() {
        Some(PackageKind::AppImage)
    } else if Path::new("/usr/bin/dpkg").exists() {
        Some(PackageKind::Deb)
    } else if Path::new("/usr/bin/rpm").exists() {
        Some(PackageKind::Rpm)
    } else {
        None
    }
}

fn running_as_appimage() -> bool {
    let (Some(image), Some(mount)) = (std::env::var_os("APPIMAGE"), std::env::var_os("APPDIR"))
    else {
        return false;
    };
    Path::new(&image).is_file() && std::env::current_exe().is_ok_and(|exe| exe.starts_with(&mount))
}

fn arch_names() -> &'static [&'static str] {
    arch_names_for(std::env::consts::ARCH)
}

fn arch_names_for(arch: &str) -> &'static [&'static str] {
    match arch {
        "x86_64" => &["x64", "x86_64", "amd64"],
        "aarch64" => &["aarch64", "arm64"],
        _ => &[],
    }
}

const PACKAGE_FLAVOR: Option<&str> = option_env!("MULTRON_PACKAGE_FLAVOR");
const FLAVORS: &[(&str, PackageKind)] = &[
    ("debian12", PackageKind::Deb),
    ("fedora", PackageKind::Rpm),
    ("opensuse", PackageKind::Rpm),
];

fn flavor_of(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    FLAVORS
        .iter()
        .map(|(f, _)| *f)
        .find(|f| lower.contains(&format!("_{f}_")))
}

fn rpm_flavor(os_release: &str) -> &'static str {
    let suse = os_release.lines().any(|line| {
        let Some((key, value)) = line.split_once('=') else {
            return false;
        };
        matches!(key.trim(), "ID" | "ID_LIKE") && value.to_ascii_lowercase().contains("suse")
    });
    if suse {
        "opensuse"
    } else {
        "fedora"
    }
}

fn wanted_flavor(kind: PackageKind) -> Option<&'static str> {
    if let Some(built) = PACKAGE_FLAVOR {
        if let Some((name, _)) = FLAVORS.iter().find(|(f, k)| *f == built && *k == kind) {
            return Some(name);
        }
    }
    (kind == PackageKind::Rpm)
        .then(|| rpm_flavor(&std::fs::read_to_string("/etc/os-release").unwrap_or_default()))
}

pub fn pick_asset(assets: &[ReleaseAsset]) -> Option<&ReleaseAsset> {
    let kind = wanted_kind()?;
    pick_asset_for(assets, kind, arch_names(), wanted_flavor(kind))
}

fn pick_asset_for<'a>(
    assets: &'a [ReleaseAsset],
    kind: PackageKind,
    arch: &[&str],
    flavor: Option<&str>,
) -> Option<&'a ReleaseAsset> {
    assets.iter().find(|a| {
        let lower = a.name.to_ascii_lowercase();
        kind_of(&a.name) == Some(kind)
            && arch.iter().any(|x| lower.contains(x))
            && flavor_of(&a.name) == flavor
    })
}

fn rpm_installer(pkg: &str) -> String {
    if Path::new("/usr/bin/zypper").exists() {
        format!(
            "pkexec zypper --non-interactive install --allow-unsigned-rpm {}",
            sh_quote(pkg)
        )
    } else if Path::new("/usr/bin/dnf").exists() {
        format!("pkexec dnf install -y {}", sh_quote(pkg))
    } else {
        format!("pkexec rpm -U {}", sh_quote(pkg))
    }
}

fn trusted_url(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https" && u.host_str().is_some_and(|h| TRUSTED_HOSTS.contains(&h))
    })
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':', '\0'])
        && !name.contains("..")
}

fn private_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn file_sha256(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex(&hasher.finalize()))
}

fn verify_ready(update: &ReadyUpdate) -> Result<()> {
    let actual = file_sha256(&update.path).context("the downloaded update is missing")?;
    if update.sha256.len() != 64 || !actual.eq_ignore_ascii_case(&update.sha256) {
        let _ = std::fs::remove_file(&update.path);
        bail!("the downloaded update was changed after it was verified, so it was not installed");
    }
    Ok(())
}

pub struct Signed<'a> {
    pub manifest_url: &'a str,
    pub signature_url: &'a str,
    pub key: [u8; 32],
    pub current: &'a str,
}

#[derive(serde::Deserialize)]
struct Manifest {
    name: String,
    version: String,
    files: std::collections::BTreeMap<String, String>,
}

pub fn pinned_key() -> Result<[u8; 32]> {
    #[cfg(debug_assertions)]
    if let Ok(key) = std::env::var("MULTRON_UPDATE_PUBKEY") {
        return decode_key(&key);
    }
    decode_key(UPDATE_PUBLIC_KEY)
}

fn decode_key(text: &str) -> Result<[u8; 32]> {
    use base64::Engine as _;
    let text = text.trim();
    if text.is_empty() {
        bail!("update signing is not set up in this build");
    }
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .ok()
        .and_then(|b| <[u8; 32]>::try_from(b).ok())
        .ok_or_else(|| anyhow!("the pinned update key is invalid"))
}

fn verify_manifest(
    manifest: &[u8],
    signature: &str,
    key: &[u8; 32],
    version: &str,
    current: &str,
) -> Result<Manifest> {
    use base64::Engine as _;
    use ed25519_dalek::{Signature, VerifyingKey};
    let key =
        VerifyingKey::from_bytes(key).map_err(|_| anyhow!("the pinned update key is invalid"))?;
    let signature = base64::engine::general_purpose::STANDARD
        .decode(signature.trim())
        .ok()
        .and_then(|b| Signature::from_slice(&b).ok())
        .ok_or_else(|| anyhow!("the update signature is malformed"))?;
    key.verify_strict(manifest, &signature).map_err(|_| {
        anyhow!("the update signature is not valid, so the update was not installed")
    })?;
    let m: Manifest =
        serde_json::from_slice(manifest).context("the signed update manifest is invalid")?;
    if m.name != MANIFEST_APP {
        bail!("the signed update manifest is for another app");
    }
    if m.version != version {
        bail!(
            "the signed update manifest is for version {}, not {version}",
            m.version
        );
    }
    let newer = match (
        semver::Version::parse(&m.version),
        semver::Version::parse(current),
    ) {
        (Ok(new), Ok(cur)) => new > cur,
        _ => false,
    };
    if !newer {
        bail!("refusing to install {} over {current}", m.version);
    }
    Ok(m)
}

pub async fn download(
    client: &reqwest::Client,
    version: &str,
    asset: &ReleaseAsset,
    signed: Option<Signed<'_>>,
    dir: &Path,
    mut progress: impl FnMut(u64, Option<u64>, u64),
) -> Result<ReadyUpdate> {
    let kind = kind_of(&asset.name).ok_or_else(|| anyhow!("unsupported update package"))?;
    if !safe_name(&asset.name) {
        bail!("the update package has an invalid name");
    }
    let signed = signed
        .ok_or_else(|| anyhow!("the release is not signed, so the update cannot be verified"))?;
    let manifest = client
        .get(signed.manifest_url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let signature = client
        .get(signed.signature_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let m = verify_manifest(&manifest, &signature, &signed.key, version, signed.current)?;
    let expected = m
        .files
        .get(&asset.name)
        .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .cloned()
        .ok_or_else(|| anyhow!("{} is not in the signed update manifest", asset.name))?;

    private_dir(dir)?;
    let path = dir.join(&asset.name);
    let part = dir.join(format!("{}.part", asset.name));
    let _ = tokio::fs::remove_file(&path).await;
    let _ = tokio::fs::remove_file(&part).await;
    let resp = client
        .get(&asset.browser_download_url)
        .send()
        .await?
        .error_for_status()
        .context("could not download the update")?;
    let total = resp
        .content_length()
        .or(Some(asset.size).filter(|s| *s > 0));
    if total.is_some_and(|t| t > MAX_UPDATE_BYTES) {
        bail!("the update package is too large");
    }
    let mut file = tokio::fs::File::create(&part).await?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut meter = crate::media::SpeedMeter::default();
    let mut reported = std::time::Instant::now();
    progress(0, total, 0);
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        if done > MAX_UPDATE_BYTES {
            drop(file);
            let _ = tokio::fs::remove_file(&part).await;
            bail!("the update package is too large");
        }
        let speed = meter.sample(done);
        if reported.elapsed() >= std::time::Duration::from_millis(250) {
            reported = std::time::Instant::now();
            progress(done, total, speed);
        }
    }
    progress(done, total, meter.sample(done));
    file.flush().await?;
    drop(file);

    let actual = hex(&hasher.finalize());
    if !actual.eq_ignore_ascii_case(&expected) {
        let _ = tokio::fs::remove_file(&part).await;
        bail!("the update failed its SHA-256 check");
    }
    tokio::fs::rename(&part, &path).await?;
    Ok(ReadyUpdate {
        version: version.to_string(),
        path,
        kind,
        sha256: actual,
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
        PackageKind::WindowsPortable => {
            let target = crate::portable::dir().ok_or_else(|| {
                anyhow!("Multron Download Manager is not running as a portable copy")
            })?;
            let exe = std::env::current_exe()?;
            let exe_name = exe
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let staging = update.path.with_extension("staging");
            let _ = std::fs::remove_dir_all(&staging);
            let installed = (|| -> Result<()> {
                crate::portable::extract(&update.path, &staging)?;
                let root = crate::portable::package_root(&staging, &exe_name).ok_or_else(|| {
                    anyhow!("the portable package does not contain Multron Download Manager")
                })?;
                crate::portable::replace_files(&root, &target)
            })();
            let _ = std::fs::remove_dir_all(&staging);
            installed?;
            if relaunch {
                relaunch_later(&exe)?;
            }
        }
        PackageKind::Dmg => {
            let exe = std::env::current_exe()?;
            let bundle = exe
                .ancestors()
                .find(|p| p.extension().is_some_and(|e| e == "app"))
                .ok_or_else(|| anyhow!("not running from an .app bundle"))?
                                .to_path_buf();
            #[cfg(target_os = "macos")]
            remove_quarantine(&update.path);
            let script = format!(
                r#"sleep 2
mnt=$(mktemp -d)
hdiutil attach -nobrowse -quiet -mountpoint "$mnt" {dmg} || exit 1
app=$(ls -d "$mnt"/*.app | head -n 1)
if [ -n "$app" ]; then
  rm -rf {bundle}.old && mv {bundle} {bundle}.old && ditto "$app" {bundle} && rm -rf {bundle}.old || mv {bundle}.old {bundle}
fi
hdiutil detach -quiet "$mnt"
xattr -rd com.apple.quarantine {bundle} 2>/dev/null || true
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
            if !running_as_appimage() {
                bail!("Multron Download Manager is not running from an AppImage");
            }
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
                rpm_installer(&pkg)
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

#[cfg(windows)]
fn relaunch_later(exe: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        &relaunch_script(std::process::id(), exe),
    ]);
    detach(&mut cmd);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.spawn()
        .context("could not restart Multron Download Manager")?;
    Ok(())
}

#[cfg(not(windows))]
fn relaunch_later(_exe: &Path) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn remove_quarantine(path: &Path) {
    match Command::new("xattr")
        .args(["-rd", "com.apple.quarantine"])
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(s) if s.success() => {
            tracing::debug!("removed quarantine flag from {}", path.display());
        }
        Ok(s) => {
            tracing::warn!(
                "xattr returned {} while removing quarantine from {}",
                s,
                path.display()
            );
        }
        Err(e) => {
            tracing::warn!(
                "could not run xattr to remove quarantine from {}: {e}",
                path.display()
            );
        }
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn relaunch_script(pid: u32, exe: &Path) -> String {
    let path = exe.to_string_lossy();
    let path = path
        .strip_prefix(r"\\?\")
        .unwrap_or(&path)
        .replace('\'', "''");
    format!(
        "Wait-Process -Id {pid} -Timeout 120 -ErrorAction SilentlyContinue; Start-Process -FilePath '{path}'"
    )
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

const MAX_ATTEMPTS: u32 = 2;
const ATTEMPT_PAUSE_SECS: i64 = 12 * 3600;

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct Attempt {
    version: String,
    count: u32,
    at: i64,
}

static ATTEMPT_FILE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn init(data_dir: &Path) {
    let file = data_dir.join("update-attempt.json");
    forget_settled(&file, env!("CARGO_PKG_VERSION"));
    let _ = ATTEMPT_FILE.set(file);
}

fn forget_settled(file: &Path, current: &str) {
    if read_attempt(file).is_some_and(|a| settled(&a, current)) {
        let _ = std::fs::remove_file(file);
    }
}

fn read_attempt(file: &Path) -> Option<Attempt> {
    serde_json::from_slice(&std::fs::read(file).ok()?).ok()
}

fn settled(a: &Attempt, current: &str) -> bool {
    match (
        semver::Version::parse(&a.version),
        semver::Version::parse(current),
    ) {
        (Ok(tried), Ok(current)) => current >= tried,
        _ => true,
    }
}

fn paused(a: Option<&Attempt>, version: &str, now: i64) -> bool {
    a.is_some_and(|a| {
        a.version == version && a.count >= MAX_ATTEMPTS && now - a.at < ATTEMPT_PAUSE_SECS
    })
}

fn next_attempt(a: Option<Attempt>, version: &str, now: i64) -> Attempt {
    match a {
        Some(a) if a.version == version => Attempt {
            version: a.version,
            count: a.count + 1,
            at: now,
        },
        _ => Attempt {
            version: version.to_string(),
            count: 1,
            at: now,
        },
    }
}

fn install_paused(version: &str) -> bool {
    ATTEMPT_FILE
        .get()
        .is_some_and(|f| paused_at(f, version, now_secs()))
}

fn paused_at(file: &Path, version: &str, now: i64) -> bool {
    paused(read_attempt(file).as_ref(), version, now)
}

fn note_attempt(version: &str) {
    if let Some(file) = ATTEMPT_FILE.get() {
        record_at(file, version, now_secs());
    }
}

fn record_at(file: &Path, version: &str, now: i64) {
    let next = next_attempt(read_attempt(file), version, now);
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(bytes) = serde_json::to_vec(&next) {
        let _ = std::fs::write(file, bytes);
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
    if install_paused(&version) {
        use tauri::Emitter;
        tracing::warn!("the update to {version} did not install twice; not trying again for now");
        set_mandatory(app, None);
        let _ = app.emit(
            crate::manager::EVT_TOAST,
            serde_json::json!({
                "kind": "warning",
                "message": format!("Multron Download Manager {version} could not be installed automatically. Download it from {}", releases_page()),
            }),
        );
        return;
    }
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
                match install_ready(true, false) {
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

pub struct Policy<'a> {
    pub key: [u8; 32],
    pub current: &'a str,
    pub official_only: bool,
}

pub async fn prepare(
    client: &reqwest::Client,
    info: crate::updates::AppUpdate,
    policy: &Policy<'_>,
    dir: &Path,
    progress: impl FnMut(u64, Option<u64>, u64),
) -> Result<Option<ReadyUpdate>> {
    if let Some(e) = info.error {
        bail!("update check failed: {e}");
    }
    let Some(latest) = info.latest.filter(|_| info.has_update) else {
        return Ok(None);
    };
    let asset = info.asset.ok_or_else(|| {
        anyhow!(
            "no update package for this system; download it from {}",
            releases_page()
        )
    })?;
    let (Some(manifest_url), Some(signature_url)) = (info.manifest_url, info.signature_url) else {
        bail!("the release is not signed, so the update cannot be verified");
    };
    if policy.official_only
        && ![&asset.browser_download_url, &manifest_url, &signature_url]
            .into_iter()
            .all(|u| trusted_url(u))
    {
        bail!("the update does not come from the official GitHub releases");
    }
    download(
        client,
        &latest,
        &asset,
        Some(Signed {
            manifest_url: &manifest_url,
            signature_url: &signature_url,
            key: policy.key,
            current: policy.current,
        }),
        dir,
        progress,
    )
    .await
    .map(Some)
}

pub async fn fetch_latest(
    app: &tauri::AppHandle,
    client: &reqwest::Client,
) -> Result<Option<ReadyUpdate>> {
    use std::sync::atomic::Ordering;
    use tauri::Emitter;
    let info = crate::updates::check_app(client).await;
    if let Some(r) =
        ready().filter(|r| info.has_update && info.latest.as_deref() == Some(r.version.as_str()))
    {
        return Ok(Some(r));
    }
    if info.error.is_none() && !info.has_update {
        return Ok(None);
    }
    let policy = Policy {
        key: pinned_key()?,
        current: env!("CARGO_PKG_VERSION"),
        official_only: !crate::updates::test_mode(),
    };
    let dir = match crate::portable::data_dir() {
        Some(data) => data.join("update"),
        None => {
            use tauri::Manager as _;
            app.path()
                .app_cache_dir()
                .context("no cache folder for the update")?
                .join("update")
        }
    };
    if BUSY.swap(true, Ordering::SeqCst) {
        bail!("the update is already downloading");
    }
    let label = format!(
        "Multron Download Manager {}",
        info.latest.clone().unwrap_or_default()
    );
    let emit = |downloaded: u64, total: Option<u64>, speed: u64, done: bool| {
        let _ = app.emit(
            crate::manager::EVT_TOOLS,
            crate::media::ToolProgress {
                tool: label.clone(),
                downloaded,
                total,
                speed,
                done,
            },
        );
    };
    let result = prepare(client, info, &policy, &dir, |d, t, s| emit(d, t, s, false)).await;
    emit(0, None, 0, true);
    BUSY.store(false, Ordering::SeqCst);
    let Some(update) = result? else {
        return Ok(None);
    };
    if let Ok(mut r) = READY.lock() {
        *r = Some(update.clone());
    }
    let _ = app.emit(EVT_READY, &update);
    Ok(Some(update))
}

pub fn install_ready(relaunch: bool, force: bool) -> Result<bool> {
    use std::sync::atomic::Ordering;
    let Some(update) = ready() else {
        return Ok(false);
    };
    if !force && install_paused(&update.version) {
        return Ok(false);
    }
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    if let Err(e) = verify_ready(&update) {
        if let Ok(mut r) = READY.lock() {
            *r = None;
        }
        INSTALLING.store(false, Ordering::SeqCst);
        return Err(e);
    }
    if let Err(e) = install(&update, relaunch) {
        INSTALLING.store(false, Ordering::SeqCst);
        return Err(e);
    }
    note_attempt(&update.version);
    Ok(true)
}

pub fn asset_url(assets: &[ReleaseAsset], name: &str) -> Option<String> {
    assets
        .iter()
        .find(|a| a.name == name)
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

    const SETUP: &str = "Multron-Download-Manager-9.9.9-x64-Setup.exe";
    const OLD_SETUP: &str = "Multron-Download-Manager-0.0.5-x64-Setup.exe";

    fn signing_key(seed: u8) -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
    }

    fn sign(key: &ed25519_dalek::SigningKey, manifest: &str) -> String {
        use base64::Engine as _;
        use ed25519_dalek::Signer as _;
        base64::engine::general_purpose::STANDARD.encode(key.sign(manifest.as_bytes()).to_bytes())
    }

    fn manifest(version: &str, name: &str, hash: &str) -> String {
        format!(
            r#"{{"name":"multron-download-manager","version":"{version}","files":{{"{name}":"{hash}"}}}}"#
        )
    }

    struct FakeRelease {
        base: String,
        dir: PathBuf,
        key: [u8; 32],
    }

    impl FakeRelease {
        fn signed(&self, manifest: &str, signature: &str) -> Signed<'_> {
            Signed {
                manifest_url: Box::leak(format!("{}/{manifest}", self.base).into_boxed_str()),
                signature_url: Box::leak(format!("{}/{signature}", self.base).into_boxed_str()),
                key: self.key,
                current: "0.0.4",
            }
        }

        fn asset(&self, path: &str, name: &str) -> ReleaseAsset {
            ReleaseAsset {
                name: name.into(),
                browser_download_url: format!("{}/{path}", self.base),
                size: 0,
            }
        }
    }

    async fn fake_release(name: &str) -> FakeRelease {
        use axum::routing::get;
        let genuine = b"genuine installer ".repeat(5000);
        let hash = hex(&Sha256::digest(&genuine));
        let evil = b"malware".repeat(9000);
        let evil_hash = hex(&Sha256::digest(&evil));
        let official = signing_key(7);
        let attacker = signing_key(9);
        let good = manifest("9.9.9", SETUP, &hash);
        let forged = manifest("9.9.9", SETUP, &evil_hash);
        let old = manifest("0.0.5", OLD_SETUP, &hash);
        let routes = [
            ("/update.json", good.clone()),
            ("/update.json.sig", sign(&official, &good)),
            ("/forged.json", forged.clone()),
            ("/forged-unsigned.sig", sign(&official, &good)),
            ("/forged-attacker.sig", sign(&attacker, &forged)),
            ("/old.json", old.clone()),
            ("/old.json.sig", sign(&official, &old)),
            ("/garbage.sig", "not a signature".to_string()),
        ];
        let mut app = axum::Router::new()
            .route("/genuine.exe", get(move || async move { genuine.clone() }))
            .route("/evil.exe", get(move || async move { evil.clone() }));
        for (path, body) in routes {
            app = app.route(path, get(move || async move { body.clone() }));
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let dir = std::env::temp_dir().join(format!("mdm-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        FakeRelease {
            base,
            dir,
            key: official.verifying_key().to_bytes(),
        }
    }

    async fn attempt(
        r: &FakeRelease,
        version: &str,
        asset: ReleaseAsset,
        signed: Option<Signed<'_>>,
    ) -> Result<ReadyUpdate> {
        download(
            &reqwest::Client::new(),
            version,
            &asset,
            signed,
            &r.dir,
            |_, _, _| {},
        )
        .await
    }

    fn assert_rejected(r: &FakeRelease, result: Result<ReadyUpdate>, reason: &str) {
        let err = match result {
            Ok(_) => panic!("the update was accepted, expected: {reason}"),
            Err(e) => e,
        };
        assert!(format!("{err:#}").contains(reason), "{err:#}");
        assert!(!r.dir.join(SETUP).exists());
        assert!(!r.dir.join(format!("{SETUP}.part")).exists());
        let _ = std::fs::remove_dir_all(&r.dir);
    }

    #[tokio::test]
    async fn installs_a_genuine_signed_package() {
        let r = fake_release("genuine").await;
        let update = attempt(
            &r,
            "9.9.9",
            r.asset("genuine.exe", SETUP),
            Some(r.signed("update.json", "update.json.sig")),
        )
        .await
        .unwrap();
        assert_eq!(update.path, r.dir.join(SETUP));
        assert!(verify_ready(&update).is_ok());
        let _ = std::fs::remove_dir_all(&r.dir);
    }

    #[tokio::test]
    async fn rejects_a_package_swapped_on_the_release() {
        let r = fake_release("swapped-asset").await;
        let result = attempt(
            &r,
            "9.9.9",
            r.asset("evil.exe", SETUP),
            Some(r.signed("update.json", "update.json.sig")),
        )
        .await;
        assert_rejected(&r, result, "SHA-256");
    }

    #[tokio::test]
    async fn rejects_checksums_changed_without_the_key() {
        let r = fake_release("forged-manifest").await;
        let result = attempt(
            &r,
            "9.9.9",
            r.asset("evil.exe", SETUP),
            Some(r.signed("forged.json", "forged-unsigned.sig")),
        )
        .await;
        assert_rejected(&r, result, "signature is not valid");
    }

    #[tokio::test]
    async fn rejects_a_manifest_signed_with_another_key() {
        let r = fake_release("attacker-key").await;
        let result = attempt(
            &r,
            "9.9.9",
            r.asset("evil.exe", SETUP),
            Some(r.signed("forged.json", "forged-attacker.sig")),
        )
        .await;
        assert_rejected(&r, result, "signature is not valid");
    }

    #[tokio::test]
    async fn rejects_a_garbage_signature() {
        let r = fake_release("garbage-signature").await;
        let result = attempt(
            &r,
            "9.9.9",
            r.asset("genuine.exe", SETUP),
            Some(r.signed("update.json", "garbage.sig")),
        )
        .await;
        assert_rejected(&r, result, "malformed");
    }

    #[tokio::test]
    async fn rejects_an_old_signed_manifest_replayed_as_a_new_version() {
        let r = fake_release("replay").await;
        let result = attempt(
            &r,
            "9.9.9",
            r.asset("genuine.exe", OLD_SETUP),
            Some(r.signed("old.json", "old.json.sig")),
        )
        .await;
        assert_rejected(&r, result, "for version 0.0.5, not 9.9.9");
    }

    #[tokio::test]
    async fn refuses_to_go_back_to_an_older_signed_version() {
        let r = fake_release("downgrade").await;
        let mut signed = r.signed("old.json", "old.json.sig");
        signed.current = "0.0.6";
        let result = attempt(&r, "0.0.5", r.asset("genuine.exe", OLD_SETUP), Some(signed)).await;
        assert_rejected(&r, result, "refusing to install 0.0.5 over 0.0.6");
    }

    #[tokio::test]
    async fn refuses_an_unsigned_release() {
        let r = fake_release("unsigned").await;
        let result = attempt(&r, "9.9.9", r.asset("evil.exe", SETUP), None).await;
        assert_rejected(&r, result, "not signed");
    }

    #[tokio::test]
    async fn refuses_a_package_missing_from_the_signed_manifest() {
        let r = fake_release("unlisted").await;
        let result = attempt(
            &r,
            "9.9.9",
            r.asset("evil.exe", "Multron-Download-Manager-9.9.8-x64-Setup.exe"),
            Some(r.signed("update.json", "update.json.sig")),
        )
        .await;
        assert_rejected(&r, result, "not in the signed update manifest");
    }

    #[tokio::test]
    async fn refuses_a_package_changed_after_verification() {
        let r = fake_release("changed-on-disk").await;
        let update = attempt(
            &r,
            "9.9.9",
            r.asset("genuine.exe", SETUP),
            Some(r.signed("update.json", "update.json.sig")),
        )
        .await
        .unwrap();
        std::fs::write(&update.path, b"swapped by another program").unwrap();
        let err = verify_ready(&update).unwrap_err();
        assert!(
            err.to_string().contains("changed after it was verified"),
            "{err:#}"
        );
        assert!(!update.path.exists());
        let _ = std::fs::remove_dir_all(&r.dir);
    }

    const PACKAGES: &[&str] = &[
        "Multron-Download-Manager-9.9.9-x64-Setup.exe",
        "Multron.Download.Manager_9.9.9_aarch64.dmg",
        "Multron.Download.Manager_9.9.9_x64.dmg",
        "Multron.Download.Manager_9.9.9_amd64.deb",
        "Multron.Download.Manager_9.9.9_arm64.deb",
        "Multron.Download.Manager_9.9.9_amd64.AppImage",
        "Multron.Download.Manager_9.9.9_aarch64.AppImage",
        "Multron-Download-Manager_9.9.9_fedora_x86_64.rpm",
        "Multron-Download-Manager_9.9.9_fedora_aarch64.rpm",
        "Multron-Download-Manager_9.9.9_opensuse_x86_64.rpm",
        "Multron-Download-Manager_9.9.9_opensuse_aarch64.rpm",
    ];

    struct Scenario {
        tag: &'static str,
        manifest_version: &'static str,
        signer: u8,
        serve_evil_packages: bool,
        signed: bool,
        api_status: u16,
    }

    impl Default for Scenario {
        fn default() -> Self {
            Self {
                tag: "v9.9.9",
                manifest_version: "9.9.9",
                signer: 7,
                serve_evil_packages: false,
                signed: true,
                api_status: 200,
            }
        }
    }

    struct FakeGitHub {
        api: String,
        dir: PathBuf,
    }

    async fn fake_github(name: &str, sc: Scenario) -> FakeGitHub {
        use axum::extract::{Path as UrlPath, State};
        use axum::http::StatusCode;
        use axum::routing::get;
        use std::collections::HashMap;
        use std::sync::Arc;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let genuine = b"genuine package ".repeat(4000);
        let evil = b"malicious package".repeat(4000);
        let hash = hex(&Sha256::digest(&genuine));
        let mut files: HashMap<String, Vec<u8>> = HashMap::new();
        let mut listed = serde_json::Map::new();
        for name in PACKAGES {
            listed.insert(name.to_string(), serde_json::Value::String(hash.clone()));
            let body = if sc.serve_evil_packages {
                &evil
            } else {
                &genuine
            };
            files.insert(name.to_string(), body.clone());
        }
        let manifest = serde_json::to_string_pretty(&serde_json::json!({
            "name": "multron-download-manager",
            "version": sc.manifest_version,
            "files": listed,
        }))
        .unwrap();
        let mut assets: Vec<serde_json::Value> = PACKAGES
            .iter()
            .map(|n| serde_json::json!({ "name": n, "browser_download_url": format!("{base}/files/{n}"), "size": 0 }))
            .collect();
        if sc.signed {
            files.insert(MANIFEST_NAME.into(), manifest.clone().into_bytes());
            files.insert(
                SIGNATURE_NAME.into(),
                sign(&signing_key(sc.signer), &manifest).into_bytes(),
            );
            for n in [MANIFEST_NAME, SIGNATURE_NAME] {
                assets.push(serde_json::json!({ "name": n, "browser_download_url": format!("{base}/files/{n}"), "size": 0 }));
            }
        }
        let release = serde_json::json!({
            "tag_name": sc.tag,
            "html_url": format!("{base}/release"),
            "body": "notes",
            "published_at": "2026-10-18T00:00:00Z",
            "assets": assets,
        })
        .to_string();
        let status = StatusCode::from_u16(sc.api_status).unwrap();
        let files = Arc::new(files);
        let app = axum::Router::new()
            .route(
                "/api/latest",
                get(move || async move { (status, release.clone()) }),
            )
            .route(
                "/files/{name}",
                get(
                    |State(files): State<Arc<HashMap<String, Vec<u8>>>>,
                     UrlPath(name): UrlPath<String>| async move {
                        match files.get(&name) {
                            Some(b) => (StatusCode::OK, b.clone()),
                            None => (StatusCode::NOT_FOUND, Vec::new()),
                        }
                    },
                ),
            )
            .with_state(files);
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let dir = std::env::temp_dir().join(format!("mdm-pipeline-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        FakeGitHub {
            api: format!("{base}/api/latest"),
            dir,
        }
    }

    async fn run_update(gh: &FakeGitHub, official_only: bool) -> Result<Option<ReadyUpdate>> {
        let client = reqwest::Client::new();
        let info = crate::updates::check_app_at(&client, &gh.api, "0.0.5").await;
        let policy = Policy {
            key: signing_key(7).verifying_key().to_bytes(),
            current: "0.0.5",
            official_only,
        };
        prepare(&client, info, &policy, &gh.dir, |_, _, _| {}).await
    }

    fn has_package_for_this_system() -> bool {
        let assets: Vec<ReleaseAsset> = PACKAGES.iter().map(|n| asset(n)).collect();
        pick_asset(&assets).is_some()
    }

    fn expect_refused(gh: &FakeGitHub, result: Result<Option<ReadyUpdate>>, reason: &str) {
        let err = match result {
            Ok(r) => panic!("the update was accepted ({r:?}), expected: {reason}"),
            Err(e) => format!("{e:#}"),
        };
        assert!(err.contains(reason), "{err}");
        let left: Vec<_> = std::fs::read_dir(&gh.dir)
            .map(|d| d.flatten().map(|e| e.file_name()).collect())
            .unwrap_or_default();
        assert!(left.is_empty(), "files left behind: {left:?}");
        let _ = std::fs::remove_dir_all(&gh.dir);
    }

    #[tokio::test]
    async fn pipeline_installs_a_genuine_signed_release() {
        if !has_package_for_this_system() {
            return;
        }
        let gh = fake_github("genuine", Scenario::default()).await;
        let update = run_update(&gh, false).await.unwrap().expect("an update");
        assert_eq!(update.version, "9.9.9");
        assert!(PACKAGES.iter().any(|n| update.path == gh.dir.join(n)));
        verify_ready(&update).unwrap();
        let _ = std::fs::remove_dir_all(&gh.dir);
    }

    #[tokio::test]
    async fn pipeline_refuses_packages_from_anywhere_but_github() {
        if !has_package_for_this_system() {
            return;
        }
        let gh = fake_github("not-github", Scenario::default()).await;
        let result = run_update(&gh, true).await;
        expect_refused(&gh, result, "official GitHub releases");
    }

    #[tokio::test]
    async fn pipeline_refuses_swapped_packages() {
        if !has_package_for_this_system() {
            return;
        }
        let sc = Scenario {
            serve_evil_packages: true,
            ..Default::default()
        };
        let gh = fake_github("swapped", sc).await;
        let result = run_update(&gh, false).await;
        expect_refused(&gh, result, "SHA-256");
    }

    #[tokio::test]
    async fn pipeline_refuses_a_release_signed_with_a_stolen_github_account_but_no_key() {
        if !has_package_for_this_system() {
            return;
        }
        let sc = Scenario {
            signer: 9,
            serve_evil_packages: true,
            ..Default::default()
        };
        let gh = fake_github("wrong-key", sc).await;
        let result = run_update(&gh, false).await;
        expect_refused(&gh, result, "signature is not valid");
    }

    #[tokio::test]
    async fn pipeline_refuses_an_unsigned_release() {
        if !has_package_for_this_system() {
            return;
        }
        let sc = Scenario {
            signed: false,
            serve_evil_packages: true,
            ..Default::default()
        };
        let gh = fake_github("unsigned", sc).await;
        let result = run_update(&gh, false).await;
        expect_refused(&gh, result, "not signed");
    }

    #[tokio::test]
    async fn pipeline_refuses_an_old_manifest_under_a_new_tag() {
        if !has_package_for_this_system() {
            return;
        }
        let sc = Scenario {
            manifest_version: "0.0.6",
            ..Default::default()
        };
        let gh = fake_github("replayed", sc).await;
        let result = run_update(&gh, false).await;
        expect_refused(&gh, result, "for version 0.0.6, not 9.9.9");
    }

    #[tokio::test]
    async fn pipeline_ignores_releases_that_are_not_newer() {
        for tag in ["v0.0.5", "v0.0.4", "nightly"] {
            let sc = Scenario {
                tag,
                manifest_version: "0.0.5",
                ..Default::default()
            };
            let gh = fake_github("not-newer", sc).await;
            assert!(run_update(&gh, false).await.unwrap().is_none(), "{tag}");
            let _ = std::fs::remove_dir_all(&gh.dir);
        }
    }

    #[tokio::test]
    async fn pipeline_reports_a_failed_update_check() {
        let sc = Scenario {
            api_status: 500,
            ..Default::default()
        };
        let gh = fake_github("api-error", sc).await;
        let result = run_update(&gh, false).await;
        expect_refused(&gh, result, "update check failed");
    }

    #[test]
    fn accepts_a_manifest_signed_by_the_release_workflow() {
        let manifest = "{\n  \"name\": \"multron-download-manager\",\n  \"version\": \"0.0.5\",\n  \"files\": {\n    \"Multron-Download-Manager-0.0.5-x64-Setup.exe\": \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n  }\n}\n";
        let signature = "Tn7knJVEUgniy5P8qXN/kdhV6vzFNuPM7tgti1mHfz9/eAwm0S6zhigm4CXp/A2l1OGme1uutJAdjGD5ukdPBQ==";
        let key = signing_key(7).verifying_key().to_bytes();
        let m = verify_manifest(manifest.as_bytes(), signature, &key, "0.0.5", "0.0.4").unwrap();
        assert_eq!(m.files.len(), 1);
        let edited = manifest.replace("0.0.5\"", "0.0.6\"");
        assert!(verify_manifest(edited.as_bytes(), signature, &key, "0.0.6", "0.0.4").is_err());
    }

    #[test]
    fn the_pinned_release_key_is_a_valid_ed25519_key() {
        if UPDATE_PUBLIC_KEY.trim().is_empty() {
            return;
        }
        let key = decode_key(UPDATE_PUBLIC_KEY).unwrap();
        assert!(ed25519_dalek::VerifyingKey::from_bytes(&key).is_ok());
    }

    #[test]
    fn refuses_updates_when_no_key_is_pinned() {
        assert!(decode_key("")
            .unwrap_err()
            .to_string()
            .contains("not set up"));
        assert!(decode_key("c2hvcnQ=").is_err());
        use base64::Engine as _;
        let key = base64::engine::general_purpose::STANDARD.encode([1u8; 32]);
        assert_eq!(decode_key(&format!("{key}\n")).unwrap(), [1u8; 32]);
    }

    #[test]
    fn accepts_only_official_github_addresses() {
        assert!(trusted_url(&format!(
            "https://github.com/{REPO}/releases/download/v1.0.0/{SETUP}"
        )));
        assert!(trusted_url(&format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        )));
        for bad in [
            "http://github.com/x/y/releases/download/v1/a.exe",
            "https://github.com.evil.example/a.exe",
            "https://evil.example/github.com/a.exe",
            "https://github.com@evil.example/a.exe",
            "file:///C:/Windows/Temp/a.exe",
            "not a url",
        ] {
            assert!(!trusted_url(bad), "{bad}");
        }
    }

    #[test]
    fn rejects_package_names_that_leave_the_update_folder() {
        assert!(safe_name(SETUP));
        for bad in [
            "",
            "../evil.exe",
            "..\\evil.exe",
            "a/b.deb",
            "C:evil.exe",
            ".hidden.deb",
            "x..y.exe",
        ] {
            assert!(!safe_name(bad), "{bad}");
        }
    }

    #[test]
    fn picks_the_package_for_this_system() {
        let assets = [
            asset("Multron-Download-Manager-0.0.2-x64-Setup.exe"),
            asset("Multron.Download.Manager_0.0.2_aarch64.dmg"),
            asset("Multron.Download.Manager_0.0.2_x64.dmg"),
            asset("Multron.Download.Manager_0.0.2_amd64.deb"),
            asset("Multron.Download.Manager-0.0.2-1.x86_64.rpm"),
            asset("multron-browser-extension-0.0.2.zip"),
            asset("update.json"),
            asset("update.json.sig"),
        ];
        let picked = pick_asset(&assets).map(|a| a.name.as_str());
        if cfg!(all(windows, target_arch = "x86_64")) {
            assert_eq!(picked, Some("Multron-Download-Manager-0.0.2-x64-Setup.exe"));
        }
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert_eq!(picked, Some("Multron.Download.Manager_0.0.2_aarch64.dmg"));
        }
        assert_eq!(
            asset_url(&assets, MANIFEST_NAME).as_deref(),
            Some("https://example.com/update.json")
        );
        assert_eq!(
            asset_url(&assets, SIGNATURE_NAME).as_deref(),
            Some("https://example.com/update.json.sig")
        );
    }

    #[test]
    fn picks_the_package_built_for_this_distribution() {
        let assets = [
            asset("Multron-Download-Manager_0.0.2_debian12_amd64.deb"),
            asset("Multron-Download-Manager_0.0.2_debian12_arm64.deb"),
            asset("Multron-Download-Manager_0.0.2_opensuse_x86_64.rpm"),
            asset("Multron-Download-Manager_0.0.2_opensuse_aarch64.rpm"),
            asset("Multron-Download-Manager_0.0.2_fedora_x86_64.rpm"),
            asset("Multron-Download-Manager_0.0.2_fedora_aarch64.rpm"),
            asset("Multron.Download.Manager_0.0.2_amd64.deb"),
            asset("Multron.Download.Manager_0.0.2_arm64.deb"),
            asset("Multron.Download.Manager_0.0.2_amd64.AppImage"),
        ];
        let x64 = arch_names_for("x86_64");
        let arm = arch_names_for("aarch64");
        let pick = |kind, arch, flavor| {
            pick_asset_for(&assets, kind, arch, flavor).map(|a| a.name.as_str())
        };
        assert_eq!(
            pick(PackageKind::Deb, x64, None),
            Some("Multron.Download.Manager_0.0.2_amd64.deb")
        );
        assert_eq!(
            pick(PackageKind::Deb, arm, None),
            Some("Multron.Download.Manager_0.0.2_arm64.deb")
        );
        assert_eq!(
            pick(PackageKind::Deb, x64, Some("debian12")),
            Some("Multron-Download-Manager_0.0.2_debian12_amd64.deb")
        );
        assert_eq!(
            pick(PackageKind::Deb, arm, Some("debian12")),
            Some("Multron-Download-Manager_0.0.2_debian12_arm64.deb")
        );
        assert_eq!(
            pick(PackageKind::Rpm, x64, Some("fedora")),
            Some("Multron-Download-Manager_0.0.2_fedora_x86_64.rpm")
        );
        assert_eq!(
            pick(PackageKind::Rpm, arm, Some("fedora")),
            Some("Multron-Download-Manager_0.0.2_fedora_aarch64.rpm")
        );
        assert_eq!(
            pick(PackageKind::Rpm, x64, Some("opensuse")),
            Some("Multron-Download-Manager_0.0.2_opensuse_x86_64.rpm")
        );
        assert_eq!(
            pick(PackageKind::Rpm, arm, Some("opensuse")),
            Some("Multron-Download-Manager_0.0.2_opensuse_aarch64.rpm")
        );
        assert_eq!(pick(PackageKind::Rpm, x64, None), None);
        assert_eq!(
            pick(PackageKind::AppImage, x64, None),
            Some("Multron.Download.Manager_0.0.2_amd64.AppImage")
        );
    }

    #[test]
    fn stops_retrying_an_update_that_does_not_install() {
        let first = next_attempt(None, "0.0.7", 1000);
        assert!(!paused(Some(&first), "0.0.7", 1001));
        let second = next_attempt(Some(first), "0.0.7", 2000);
        assert_eq!(second.count, 2);
        assert!(paused(Some(&second), "0.0.7", 2001));
        assert!(!paused(Some(&second), "0.0.8", 2001));
        assert!(!paused(Some(&second), "0.0.7", 2000 + ATTEMPT_PAUSE_SECS));
        let other = next_attempt(Some(second), "0.0.8", 3000);
        assert_eq!(other.count, 1);
    }

    #[test]
    fn an_update_that_never_installs_stops_after_two_restarts() {
        let dir = std::env::temp_dir().join(format!("mdm-attempts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("update-attempt.json");
        let mut installs = 0;
        let mut now = 1_000_000;
        for _start in 0..10 {
            forget_settled(&file, "0.0.6");
            if paused_at(&file, "0.0.7", now) {
                break;
            }
            record_at(&file, "0.0.7", now);
            installs += 1;
            now += 30;
        }
        assert_eq!(installs, 2);
        assert!(paused_at(&file, "0.0.7", now + 3600));
        assert!(!paused_at(&file, "0.0.7", now + ATTEMPT_PAUSE_SECS));
        assert!(!paused_at(&file, "0.0.8", now));
        forget_settled(&file, "0.0.7");
        assert!(!file.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn forgets_attempts_once_the_version_is_installed() {
        let a = Attempt {
            version: "0.0.7".into(),
            count: 2,
            at: 0,
        };
        assert!(settled(&a, "0.0.7"));
        assert!(settled(&a, "0.0.8"));
        assert!(!settled(&a, "0.0.6"));
    }

    #[test]
    fn every_version_file_matches_the_app_version() {
        let version = env!("CARGO_PKG_VERSION");
        let read = |text: &str| -> String {
            serde_json::from_str::<serde_json::Value>(text).unwrap()["version"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert_eq!(read(include_str!("../tauri.conf.json")), version);
        assert_eq!(read(include_str!("../../package.json")), version);
        assert_eq!(read(include_str!("../../frontend/package.json")), version);
        assert_eq!(read(include_str!("../../extension/manifest.json")), version);
    }

    #[test]
    fn relaunch_waits_for_the_old_process() {
        let script = relaunch_script(
            4242,
            Path::new(r"\\?\C:\Users\o'neil\Desktop\Multron\multron-download-manager.exe"),
        );
        assert_eq!(
            script,
            r"Wait-Process -Id 4242 -Timeout 120 -ErrorAction SilentlyContinue; Start-Process -FilePath 'C:\Users\o''neil\Desktop\Multron\multron-download-manager.exe'"
        );
    }

    #[test]
    fn portable_copies_pick_the_portable_zip() {
        let assets = [
            asset("multron-browser-extension-chromium-v0.0.6.zip"),
            asset("Multron-Download-Manager-0.0.6-x64-Setup.exe"),
            asset("Multron-Download-Manager-0.0.6-x64-portable.zip"),
            asset("multron-browser-extension-firefox-v0.0.6.zip"),
        ];
        let x64 = arch_names_for("x86_64");
        assert_eq!(
            pick_asset_for(&assets, PackageKind::WindowsPortable, x64, None)
                .map(|a| a.name.as_str()),
            Some("Multron-Download-Manager-0.0.6-x64-portable.zip")
        );
        assert_eq!(
            pick_asset_for(&assets, PackageKind::WindowsSetup, x64, None).map(|a| a.name.as_str()),
            Some("Multron-Download-Manager-0.0.6-x64-Setup.exe")
        );
    }

    #[test]
    fn tells_opensuse_from_fedora() {
        assert_eq!(
            rpm_flavor("NAME=\"openSUSE Leap\"\nID=\"opensuse-leap\"\nID_LIKE=\"suse opensuse\"\n"),
            "opensuse"
        );
        assert_eq!(
            rpm_flavor("NAME=\"openSUSE Tumbleweed\"\nID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n"),
            "opensuse"
        );
        assert_eq!(rpm_flavor("NAME=\"Fedora Linux\"\nID=fedora\n"), "fedora");
        assert_eq!(
            rpm_flavor("NAME=\"Nobara Linux\"\nID=nobara\nID_LIKE=\"rhel centos fedora\"\n"),
            "fedora"
        );
        assert_eq!(rpm_flavor(""), "fedora");
    }
}
