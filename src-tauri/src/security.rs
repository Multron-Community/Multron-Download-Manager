use std::path::{Path, PathBuf};

pub const AMSI_MAX_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Clean(String),
    Threat(String),
    Unavailable(String),
}

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

pub fn mark_from_internet(path: &Path, url: &str, referrer: Option<&str>) {
    if !path.is_file() {
        return;
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
        if let Err(e) = std::fs::write(PathBuf::from(ads), content) {
            tracing::warn!("could not mark {} as downloaded: {e}", path.display());
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
        let _ = std::process::Command::new("xattr")
            .args(["-w", "com.apple.quarantine", &value])
            .arg(path)
            .status();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = (url, referrer);
    }
}

#[cfg(all(windows, test))]
pub fn read_mark(path: &Path) -> Option<String> {
    let mut ads = path.as_os_str().to_os_string();
    ads.push(":Zone.Identifier");
    std::fs::read_to_string(PathBuf::from(ads)).ok()
}

pub fn antivirus_error(err: &anyhow::Error) -> Option<&'static str> {
    for cause in err.chain() {
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            match io.raw_os_error() {
                Some(225) | Some(226) => return Some("blocked by your antivirus: it found a threat in this file"),
                Some(5) if cfg!(windows) => {
                    return Some("access denied: your antivirus most likely blocked this file because it found a threat")
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

pub fn decode_product_state(state: u32) -> bool {
    (state >> 12) & 0xF == 1
}

#[cfg(windows)]
pub fn antivirus_products() -> Option<Vec<(String, bool)>> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct | ForEach-Object { $_.displayName + '|' + $_.productState }",
        ])
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let list: Vec<(String, bool)> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let (name, state) = l.trim().rsplit_once('|')?;
            Some((
                name.trim().to_string(),
                decode_product_state(state.trim().parse().ok()?),
            ))
        })
        .collect();
    (!list.is_empty()).then_some(list)
}

pub fn scan(paths: &[PathBuf]) -> Verdict {
    #[cfg(windows)]
    {
        windows_scan(paths)
    }
    #[cfg(not(windows))]
    {
        let _ = paths;
        Verdict::Unavailable("not available on this system".into())
    }
}

#[cfg(windows)]
fn defender() -> Option<PathBuf> {
    let platform = std::env::var_os("ProgramData").map(|d| {
        PathBuf::from(d)
            .join("Microsoft")
            .join("Windows Defender")
            .join("Platform")
    });
    let mut candidates: Vec<PathBuf> = platform
        .and_then(|p| std::fs::read_dir(p).ok())
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path().join("MpCmdRun.exe"))
                .collect()
        })
        .unwrap_or_default();
    candidates.sort();
    candidates.reverse();
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        candidates.push(
            PathBuf::from(pf)
                .join("Windows Defender")
                .join("MpCmdRun.exe"),
        );
    }
    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(windows)]
fn windows_scan(paths: &[PathBuf]) -> Verdict {
    use std::os::windows::process::CommandExt;
    let products = antivirus_products();
    let mut engine = "antivirus".to_string();
    let mut defender_on = true;
    if let Some(list) = &products {
        let active: Vec<&str> = list
            .iter()
            .filter(|(_, on)| *on)
            .map(|(n, _)| n.as_str())
            .collect();
        if active.is_empty() {
            let off: Vec<&str> = list.iter().map(|(n, _)| n.as_str()).collect();
            return Verdict::Unavailable(format!(
                "no antivirus is turned on ({} is off)",
                off.join(", ")
            ));
        }
        engine = active.join(", ");
        defender_on = active
            .iter()
            .any(|n| n.to_ascii_lowercase().contains("defender"));
    }
    let missing: Vec<String> = paths
        .iter()
        .filter(|p| !p.exists())
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    if !missing.is_empty() {
        return Verdict::Threat(format!(
            "removed right after downloading, most likely by {engine}: {}",
            missing.join(", ")
        ));
    }
    if let Some(exe) = defender().filter(|_| defender_on) {
        let mut threats = Vec::new();
        let mut ran = false;
        for p in paths {
            let out = std::process::Command::new(&exe)
                .args(["-Scan", "-ScanType", "3", "-DisableRemediation", "-File"])
                .arg(p)
                .creation_flags(0x0800_0000)
                .output();
            match out.ok().and_then(|o| o.status.code().map(|c| (c, o))) {
                Some((0, _)) => ran = true,
                Some((2, o))
                    if String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .any(|l| l.trim_start().starts_with("Threat")) =>
                {
                    ran = true;
                    let text = String::from_utf8_lossy(&o.stdout);
                    let name = text
                        .lines()
                        .find_map(|l| {
                            l.trim()
                                .strip_prefix("Threat")
                                .map(|r| r.trim_start_matches([' ', ':']).trim().to_string())
                        })
                        .unwrap_or_else(|| "threat detected".into());
                    threats.push(name);
                }
                _ => {}
            }
        }
        if ran {
            return if threats.is_empty() {
                Verdict::Clean("Microsoft Defender".into())
            } else {
                Verdict::Threat(threats.join(", "))
            };
        }
    }
    amsi_scan(paths, &engine)
}

#[cfg(windows)]
fn amsi_scan(paths: &[PathBuf], engine: &str) -> Verdict {
    use windows_sys::Win32::System::Antimalware::{
        AmsiCloseSession, AmsiInitialize, AmsiOpenSession, AmsiScanBuffer, AmsiUninitialize,
        AMSI_RESULT_DETECTED, HAMSICONTEXT, HAMSISESSION,
    };
    let wide = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let mut blocked: Vec<String> = Vec::new();
    let small: Vec<&PathBuf> = paths
        .iter()
        .filter(|p| match std::fs::metadata(p) {
            Ok(m) => m.len() > 0 && m.len() <= AMSI_MAX_BYTES,
            Err(e) => {
                if matches!(e.raw_os_error(), Some(5) | Some(225) | Some(226)) {
                    blocked.push(p.to_string_lossy().into_owned());
                }
                false
            }
        })
        .collect();
    if !blocked.is_empty() {
        return Verdict::Threat(format!("blocked by {engine}: {}", blocked.join(", ")));
    }
    if small.is_empty() {
        return Verdict::Unavailable(
            "too large for an on-demand scan; your antivirus checks it when it is opened".into(),
        );
    }
    let app = wide("Multron Download Manager");
    let mut ctx: HAMSICONTEXT = std::ptr::null_mut();
    if unsafe { AmsiInitialize(app.as_ptr(), &mut ctx) } < 0 || ctx.is_null() {
        return Verdict::Unavailable("no antivirus is available for on-demand scans".into());
    }
    let mut session: HAMSISESSION = std::ptr::null_mut();
    unsafe { AmsiOpenSession(ctx, &mut session) };
    let mut threats = Vec::new();
    let mut scanned = 0;
    for p in small {
        let data = match std::fs::read(p) {
            Ok(d) => d,
            Err(e) => {
                if matches!(e.raw_os_error(), Some(5) | Some(225) | Some(226))
                    || e.kind() == std::io::ErrorKind::NotFound
                {
                    blocked.push(
                        p.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    );
                }
                continue;
            }
        };
        let name = wide(&p.to_string_lossy());
        let mut result = 0;
        let hr = unsafe {
            AmsiScanBuffer(
                ctx,
                data.as_ptr().cast(),
                data.len() as u32,
                name.as_ptr(),
                session,
                &mut result,
            )
        };
        if hr < 0 {
            continue;
        }
        scanned += 1;
        if result >= AMSI_RESULT_DETECTED {
            threats.push(
                p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            );
        }
    }
    unsafe {
        if !session.is_null() {
            AmsiCloseSession(ctx, session);
        }
        AmsiUninitialize(ctx);
    }
    if !blocked.is_empty() {
        return Verdict::Threat(format!("blocked by {engine}: {}", blocked.join(", ")));
    }
    if !threats.is_empty() {
        Verdict::Threat(format!("detected by {engine}: {}", threats.join(", ")))
    } else if scanned > 0 {
        Verdict::Clean(format!("{engine} (AMSI)"))
    } else {
        Verdict::Unavailable("the antivirus did not answer".into())
    }
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
    fn decodes_security_center_states() {
        assert!(decode_product_state(0x041000));
        assert!(decode_product_state(0x061100));
        assert!(!decode_product_state(0x060100));
        assert!(!decode_product_state(0x040000));
    }

    #[test]
    fn recognizes_antivirus_errors() {
        let blocked =
            anyhow::Error::new(std::io::Error::from_raw_os_error(225)).context("write failed");
        assert!(antivirus_error(&blocked).is_some());
        let other = anyhow::anyhow!("HTTP 404");
        assert!(antivirus_error(&other).is_none());
    }

    #[test]
    fn reports_smartscreen_status() {
        let status = smartscreen_status();
        eprintln!("SmartScreen: {status}");
        assert!(["on", "off", "missing", "unsupported"].contains(&status));
    }

    #[cfg(windows)]
    #[test]
    fn scans_a_clean_file() {
        let dir = std::env::temp_dir().join(format!("mdm-scan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("hello.txt");
        std::fs::write(&file, b"hello from multron").unwrap();
        let verdict = scan(&[file]);
        eprintln!("{verdict:?}");
        assert!(!matches!(verdict, Verdict::Threat(_)));
        std::fs::remove_dir_all(&dir).ok();
    }
}
