use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

pub const HOST_NAME: &str = "com.multron.downloadmanager";
pub const CHROMIUM_ID: &str = "ielbgieljlboakmkjbcclbbcdamibmed";
pub const FIREFOX_ID: &str = "multron-dm@multron.community";

pub fn is_invocation(args: &[String]) -> bool {
    let manifest = format!("{HOST_NAME}.json");
    args.iter()
        .skip(1)
        .any(|a| a.starts_with("chrome-extension://"))
        || args.get(1).is_some_and(|a| a.ends_with(&manifest))
        || args.get(2).is_some_and(|a| a == FIREFOX_ID)
}

pub fn serve() {
    let message = read_message(&mut std::io::stdin().lock());
    let action = message
        .as_ref()
        .and_then(|m| m.get("action"))
        .and_then(Value::as_str)
        .unwrap_or("launch");
    let reply = match action {
        "launch" => json!({ "ok": launch() }),
        _ => json!({ "ok": false, "error": "unknown action" }),
    };
    let _ = write_message(&mut std::io::stdout().lock(), &reply);
}

fn read_message(input: &mut impl Read) -> Option<Value> {
    let mut len = [0u8; 4];
    input.read_exact(&mut len).ok()?;
    let len = u32::from_ne_bytes(len) as usize;
    if len > 1 << 20 {
        return None;
    }
    let mut buf = vec![0u8; len];
    input.read_exact(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

fn write_message(output: &mut impl Write, value: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(value)?;
    output.write_all(&(body.len() as u32).to_ne_bytes())?;
    output.write_all(&body)?;
    output.flush()
}

fn app_path() -> Option<PathBuf> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Some(PathBuf::from(appimage));
    }
    std::env::current_exe().ok()
}

fn launch() -> bool {
    let Some(exe) = app_path() else {
        return false;
    };
    let mut cmd = Command::new(&exe);
    cmd.arg("--minimized")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB);
        if cmd.spawn().is_ok() {
            return true;
        }
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        cmd.spawn().is_ok()
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
        cmd.spawn().is_ok()
    }
}

fn chromium_origins(trusted: &[String]) -> Vec<String> {
    let mut out = vec![format!("chrome-extension://{CHROMIUM_ID}/")];
    for o in trusted {
        if let Some(id) = o.strip_prefix("chrome-extension://") {
            let id = id.trim_end_matches('/');
            if !id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase()) {
                let origin = format!("chrome-extension://{id}/");
                if !out.contains(&origin) {
                    out.push(origin);
                }
            }
        }
    }
    out
}

fn manifests(exe: &Path, trusted: &[String]) -> (String, String) {
    let path = exe.to_string_lossy();
    let chromium = json!({
        "name": HOST_NAME,
        "description": "Starts Multron Download Manager for its browser extension",
        "path": path,
        "type": "stdio",
        "allowed_origins": chromium_origins(trusted),
    });
    let firefox = json!({
        "name": HOST_NAME,
        "description": "Starts Multron Download Manager for its browser extension",
        "path": path,
        "type": "stdio",
        "allowed_extensions": [FIREFOX_ID],
    });
    (
        serde_json::to_string_pretty(&chromium).unwrap_or_default(),
        serde_json::to_string_pretty(&firefox).unwrap_or_default(),
    )
}

fn write_if_changed(path: &Path, content: &str) -> bool {
    if std::fs::read_to_string(path).ok().as_deref() == Some(content) {
        return false;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(path, content).is_ok()
}

pub fn sync(data_dir: &Path, trusted: &[String]) {
    if trusted.is_empty() {
        unregister(data_dir);
    } else {
        register(data_dir, trusted);
    }
}

#[cfg(windows)]
const VENDORS: [&str; 6] = [
    r"Google\Chrome",
    r"Chromium",
    r"Microsoft\Edge",
    r"BraveSoftware\Brave-Browser",
    r"Vivaldi",
    r"Mozilla",
];

#[cfg(unix)]
fn unix_manifest_paths() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let file = format!("{HOST_NAME}.json");
    let (chromium, firefox): (Vec<PathBuf>, PathBuf) = if cfg!(target_os = "macos") {
        let support = home.join("Library/Application Support");
        (
            [
                "Google/Chrome",
                "Chromium",
                "Microsoft Edge",
                "BraveSoftware/Brave-Browser",
                "Vivaldi",
            ]
            .iter()
            .map(|d| support.join(d).join("NativeMessagingHosts"))
            .collect(),
            support.join("Mozilla/NativeMessagingHosts"),
        )
    } else {
        let config = home.join(".config");
        (
            [
                "google-chrome",
                "chromium",
                "microsoft-edge",
                "BraveSoftware/Brave-Browser",
                "vivaldi",
            ]
            .iter()
            .map(|d| config.join(d).join("NativeMessagingHosts"))
            .collect(),
            home.join(".mozilla/native-messaging-hosts"),
        )
    };
    chromium
        .into_iter()
        .chain(std::iter::once(firefox))
        .map(|d| d.join(&file))
        .collect()
}

pub fn unregister(data_dir: &Path) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        for vendor in VENDORS {
            let _ = Command::new("reg")
                .args([
                    "delete",
                    &format!(r"HKCU\Software\{vendor}\NativeMessagingHosts\{HOST_NAME}"),
                    "/f",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(0x0800_0000)
                .status();
        }
        let _ = std::fs::remove_dir_all(data_dir.join("native-host"));
    }
    #[cfg(unix)]
    {
        let _ = data_dir;
        for path in unix_manifest_paths() {
            let _ = std::fs::remove_file(path);
        }
    }
    if cfg!(target_os = "linux") {
        if let Some(home) = crate::flatpak::home() {
            unregister_flatpaks(&home);
        }
    }
}

fn unregister_flatpaks(home: &Path) {
    let file = format!("{HOST_NAME}.json");
    for app in crate::flatpak::APPS {
        for path in crate::flatpak::manifest_paths(home, app, &file) {
            let _ = std::fs::remove_file(path);
        }
        let _ = std::fs::remove_file(crate::flatpak::wrapper_path(home, app.id));
    }
}

fn register_flatpaks(home: &Path, exe: &Path, trusted: &[String]) {
    use crate::flatpak::Hosts;
    let file = format!("{HOST_NAME}.json");
    let apps = crate::flatpak::installed(&crate::flatpak::roots(home));
    for app in &apps {
        let wrapper = crate::flatpak::wrapper_path(home, app.id);
        write_if_changed(&wrapper, &crate::flatpak::wrapper_script(exe));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755));
        }
        let (chromium, firefox) = manifests(&wrapper, trusted);
        let body = match app.hosts {
            Hosts::Chromium(_) => &chromium,
            Hosts::Firefox(_) => &firefox,
        };
        for path in crate::flatpak::manifest_paths(home, app, &file) {
            write_if_changed(&path, body);
        }
    }
    if apps.iter().any(|a| matches!(a.hosts, Hosts::Firefox(_))) {
        let (_, firefox) = manifests(exe, trusted);
        write_if_changed(
            &home.join(".mozilla/native-messaging-hosts").join(&file),
            &firefox,
        );
    }
}

pub fn register(data_dir: &Path, trusted: &[String]) {
    let Some(exe) = app_path() else { return };
    let (chromium, firefox) = manifests(&exe, trusted);
    let file = format!("{HOST_NAME}.json");
    #[cfg(windows)]
    {
        let base = data_dir.join("native-host");
        let chromium_file = base.join("chromium").join(&file);
        let firefox_file = base.join("firefox").join(&file);
        let changed =
            write_if_changed(&chromium_file, &chromium) | write_if_changed(&firefox_file, &firefox);
        let stamp = base.join("registered");
        if !changed && stamp.is_file() {
            return;
        }
        let mut ok = true;
        for vendor in [
            r"Google\Chrome",
            r"Chromium",
            r"Microsoft\Edge",
            r"BraveSoftware\Brave-Browser",
            r"Vivaldi",
        ] {
            ok &= reg_set(
                &format!(r"HKCU\Software\{vendor}\NativeMessagingHosts\{HOST_NAME}"),
                &chromium_file,
            );
        }
        ok &= reg_set(
            &format!(r"HKCU\Software\Mozilla\NativeMessagingHosts\{HOST_NAME}"),
            &firefox_file,
        );
        if ok {
            let _ = std::fs::write(stamp, b"");
        }
    }
    #[cfg(unix)]
    {
        let _ = data_dir;
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return;
        };
        let (chromium_dirs, firefox_dir): (Vec<PathBuf>, PathBuf) = if cfg!(target_os = "macos") {
            let support = home.join("Library/Application Support");
            (
                [
                    "Google/Chrome",
                    "Chromium",
                    "Microsoft Edge",
                    "BraveSoftware/Brave-Browser",
                    "Vivaldi",
                ]
                .iter()
                .map(|d| support.join(d))
                .collect(),
                support.join("Mozilla/NativeMessagingHosts"),
            )
        } else {
            let config = home.join(".config");
            (
                [
                    "google-chrome",
                    "chromium",
                    "microsoft-edge",
                    "BraveSoftware/Brave-Browser",
                    "vivaldi",
                ]
                .iter()
                .map(|d| config.join(d))
                .collect(),
                home.join(".mozilla/native-messaging-hosts"),
            )
        };
        for dir in chromium_dirs.into_iter().filter(|d| d.is_dir()) {
            write_if_changed(&dir.join("NativeMessagingHosts").join(&file), &chromium);
        }
        if firefox_dir.parent().is_some_and(Path::is_dir) {
            write_if_changed(&firefox_dir.join(&file), &firefox);
        }
    }
    if cfg!(target_os = "linux") {
        if let Some(home) = crate::flatpak::home() {
            register_flatpaks(&home, &exe, trusted);
        }
    }
}

#[cfg(windows)]
fn reg_set(key: &str, value: &Path) -> bool {
    use std::os::windows::process::CommandExt;
    Command::new("reg")
        .args(["add", key, "/ve", "/t", "REG_SZ", "/d"])
        .arg(value)
        .arg("/f")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_browser_invocations() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(is_invocation(&s(&[
            "mdm.exe",
            "chrome-extension://abc/",
            "--parent-window=1"
        ])));
        assert!(is_invocation(&s(&[
            "mdm",
            "/x/com.multron.downloadmanager.json",
            FIREFOX_ID
        ])));
        assert!(!is_invocation(&s(&["mdm.exe", "--minimized"])));
        assert!(!is_invocation(&s(&["mdm.exe", "magnet:?xt=urn:btih:1"])));
    }

    #[test]
    fn registers_flatpak_browsers_inside_their_sandbox() {
        let home = std::env::temp_dir().join(format!("mdm-nativehost-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        for id in ["org.mozilla.firefox", "com.google.Chrome"] {
            std::fs::create_dir_all(
                home.join(".local/share/flatpak/app")
                    .join(id)
                    .join("current"),
            )
            .unwrap();
        }
        let exe = Path::new("/opt/mdm/multron-download-manager");
        register_flatpaks(&home, exe, &[]);
        let wrapper = crate::flatpak::wrapper_path(&home, "com.google.Chrome");
        assert!(std::fs::read_to_string(&wrapper)
            .unwrap()
            .contains("flatpak-spawn --host '/opt/mdm/multron-download-manager'"));
        let chrome: Value = serde_json::from_str(
            &std::fs::read_to_string(home.join(".var/app/com.google.Chrome/config/google-chrome/NativeMessagingHosts/com.multron.downloadmanager.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(chrome["path"], wrapper.to_string_lossy().as_ref());
        assert_eq!(
            chrome["allowed_origins"][0],
            format!("chrome-extension://{CHROMIUM_ID}/")
        );
        let sandboxed: Value = serde_json::from_str(
            &std::fs::read_to_string(home.join(".var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts/com.multron.downloadmanager.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(sandboxed["allowed_extensions"][0], FIREFOX_ID);
        let host: Value = serde_json::from_str(
            &std::fs::read_to_string(
                home.join(".mozilla/native-messaging-hosts/com.multron.downloadmanager.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(host["path"], "/opt/mdm/multron-download-manager");
        unregister_flatpaks(&home);
        assert!(!wrapper.exists());
        assert!(!home.join(".var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts/com.multron.downloadmanager.json").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn frames_messages() {
        let mut buf = Vec::new();
        write_message(&mut buf, &json!({ "action": "launch" })).unwrap();
        let back = read_message(&mut buf.as_slice()).unwrap();
        assert_eq!(back["action"], "launch");
    }

    #[test]
    fn allows_the_bundled_and_paired_extensions() {
        let o = chromium_origins(&[
            "chrome-extension://abcdefgh".into(),
            "moz-extension://xyz".into(),
            format!("chrome-extension://{CHROMIUM_ID}"),
        ]);
        assert_eq!(
            o,
            vec![
                format!("chrome-extension://{CHROMIUM_ID}/"),
                "chrome-extension://abcdefgh/".to_string()
            ]
        );
    }
}
