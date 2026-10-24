use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;

pub fn smartscreen_status() -> &'static str {
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        if !root.join("System32").join("smartscreen.exe").is_file() {
            return "missing";
        }
        let policy = reg_value(
            r"HKLM\SOFTWARE\Policies\Microsoft\Windows\System",
            "EnableSmartScreen",
        );
        if policy.as_deref() == Some("0x0") {
            return "off";
        }
        let explorer = reg_value(
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer",
            "SmartScreenEnabled",
        );
        if explorer.is_some_and(|v| v.eq_ignore_ascii_case("off")) {
            return "off";
        }
        "on"
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("spctl").arg("--status").output();
        match out {
            Ok(o) if String::from_utf8_lossy(&o.stdout).contains("disabled") => "off",
            Ok(_) => "on",
            Err(_) => "missing",
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "unsupported"
    }
}

#[cfg(windows)]
fn reg_value(key: &str, name: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("reg")
        .args(["query", key, "/v", name])
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.trim_start().starts_with(name))
        .and_then(|l| l.split_whitespace().last().map(str::to_string))
}

pub fn is_program(path: &Path) -> bool {
    const PROGRAMS: &[&str] = &[
        "exe",
        "msi",
        "msix",
        "msixbundle",
        "appx",
        "appxbundle",
        "bat",
        "cmd",
        "com",
        "scr",
        "pif",
        "ps1",
        "vbs",
        "vbe",
        "js",
        "jse",
        "wsf",
        "hta",
        "cpl",
        "lnk",
        "reg",
        "jar",
        "appimage",
        "run",
        "sh",
        "deb",
        "rpm",
        "pkg",
        "dmg",
        "command",
    ];
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| PROGRAMS.iter().any(|p| p.eq_ignore_ascii_case(e)))
}

pub fn mark_from_internet(path: &Path, url: &str, referrer: Option<&str>) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(windows)]
    {
        let web = |u: &str| u.starts_with("http://") || u.starts_with("https://");
        let mut content = String::from("[ZoneTransfer]\r\nZoneId=3\r\n");
        if let Some(r) = referrer.filter(|r| web(r)) {
            content.push_str(&format!("ReferrerUrl={r}\r\n"));
        }
        if web(url) {
            content.push_str(&format!("HostUrl={url}\r\n"));
        }
        let mut ads = path.as_os_str().to_os_string();
        ads.push(":Zone.Identifier");
        match std::fs::write(PathBuf::from(ads), content) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("could not mark {} as downloaded: {e}", path.display());
                false
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (url, referrer);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let value = format!("0081;{stamp:x};Multron Download Manager;");
        std::process::Command::new("xattr")
            .args(["-w", "com.apple.quarantine", &value])
            .arg(path)
            .status()
            .is_ok_and(|s| s.success())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = (url, referrer);
        false
    }
}

#[cfg(all(windows, test))]
pub fn read_mark(path: &Path) -> Option<String> {
    let mut ads = path.as_os_str().to_os_string();
    ads.push(":Zone.Identifier");
    std::fs::read_to_string(PathBuf::from(ads)).ok()
}

pub fn access_denied(err: &anyhow::Error) -> bool {
    cfg!(windows)
        && err.chain().any(|cause| {
            cause
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.raw_os_error() == Some(5))
        })
}

pub fn antivirus_error(err: &anyhow::Error) -> Option<&'static str> {
    for cause in err.chain() {
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            match io.raw_os_error() {
                Some(225) | Some(226) => {
                    return Some("blocked by your antivirus: it found a threat in this file")
                }
                _ => {}
            }
        }
    }
    let text = format!("{err:#}").to_lowercase();
    if text.contains("contains a virus") || text.contains("potentially unwanted software") {
        return Some("blocked by your antivirus: it found a threat in this file");
    }
    if text.contains("removed right after it was downloaded") {
        return Some(
            "the file was removed right after it was downloaded, most likely by your antivirus",
        );
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn marks_files_as_downloaded_from_the_internet() {
        let dir = std::env::temp_dir().join(format!("mdm-motw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("setup.exe");
        std::fs::write(&file, b"not really an exe").unwrap();
        mark_from_internet(
            &file,
            "https://example.org/setup.exe",
            Some("https://example.org/"),
        );
        let mark = read_mark(&file).expect("Zone.Identifier stream");
        assert!(mark.contains("ZoneId=3"));
        assert!(mark.contains("HostUrl=https://example.org/setup.exe"));
        assert!(mark.contains("ReferrerUrl=https://example.org/"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recognizes_programs() {
        assert!(is_program(Path::new("C:/x/setup.EXE")));
        assert!(is_program(Path::new("/tmp/app.AppImage")));
        assert!(!is_program(Path::new("C:/x/report.pdf")));
        assert!(!is_program(Path::new("C:/x/folder")));
    }

    #[test]
    fn recognizes_antivirus_errors() {
        let blocked =
            anyhow::Error::new(std::io::Error::from_raw_os_error(225)).context("write failed");
        assert!(antivirus_error(&blocked).is_some());
        let other = anyhow::anyhow!("HTTP 404");
        assert!(antivirus_error(&other).is_none());
        let denied =
            anyhow::Error::new(std::io::Error::from_raw_os_error(5)).context("write failed");
        assert!(antivirus_error(&denied).is_none());
        assert_eq!(access_denied(&denied), cfg!(windows));
    }

    #[test]
    fn reports_smartscreen_status() {
        let status = smartscreen_status();
        eprintln!("SmartScreen: {status}");
        assert!(["on", "off", "missing", "unsupported"].contains(&status));
    }
}
