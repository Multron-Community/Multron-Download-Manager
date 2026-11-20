use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use percent_encoding::percent_decode_str;

use crate::model::Category;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

const RESERVED_WINDOWS_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub fn sanitize_filename(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .filter(|c| !is_spoofing_char(*c))
        .collect();
    let replaced = collapse_spaces(&replaced);
    let trimmed = replaced.trim().trim_end_matches(['.', ' ']).to_string();
    let stem = trimmed.split('.').next().unwrap_or("").to_ascii_uppercase();
    let mut out = if RESERVED_WINDOWS_NAMES.contains(&stem.as_str()) {
        format!("_{trimmed}")
    } else {
        trimmed
    };
    if out.chars().count() > 200 {
        let (stem, ext) = split_ext(&out);
        let keep: String = stem
            .chars()
            .take(200 - ext.chars().count().min(40))
            .collect();
        out = format!("{keep}{ext}");
    }
    if out.is_empty() {
        "download".into()
    } else {
        out
    }
}

fn is_spoofing_char(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2069}'
            | '\u{061C}'
            | '\u{FEFF}'
    )
}

fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for c in s.chars() {
        let space = c.is_whitespace();
        if !(space && last_space) {
            out.push(if space { ' ' } else { c });
        }
        last_space = space;
    }
    out
}

pub fn split_ext(name: &str) -> (String, String) {
    match name.rfind('.') {
        Some(idx) if idx > 0 && idx < name.len() - 1 => {
            (name[..idx].to_string(), name[idx..].to_string())
        }
        _ => (name.to_string(), String::new()),
    }
}

pub fn extension(name: &str) -> String {
    split_ext(name)
        .1
        .trim_start_matches('.')
        .to_ascii_lowercase()
}

pub fn part_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let (stem, ext) = split_ext(name);
    for i in 0..10_000 {
        let candidate = if i == 0 {
            name.to_string()
        } else {
            format!("{stem} ({i}){ext}")
        };
        let p = dir.join(&candidate);
        if !p.exists() && !part_path(&p).exists() {
            return p;
        }
    }
    dir.join(format!("{stem} ({}){ext}", now_ms()))
}

pub fn category_for(name: &str) -> Category {
    match extension(name).as_str() {
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" | "ts" | "3gp" | "mpg"
        | "mpeg" => Category::Video,
        "mp3" | "flac" | "wav" | "aac" | "m4a" | "ogg" | "opus" | "wma" | "alac" => Category::Music,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "epub" | "odt"
        | "ods" | "rtf" | "csv" | "md" => Category::Documents,
        "zip" | "rar" | "7z" | "tar" | "gz" | "xz" | "bz2" | "zst" | "iso" | "dmg" | "img"
        | "tgz" => Category::Archives,
        "exe" | "msi" | "apk" | "deb" | "rpm" | "appimage" | "pkg" | "msix" | "flatpak" => {
            Category::Programs
        }
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "heic" | "avif" => {
            Category::Images
        }
        _ => Category::Other,
    }
}

pub fn filename_from_disposition(value: &str) -> Option<String> {
    let parts: Vec<&str> = value.split(';').map(str::trim).collect();
    for part in &parts {
        if let Some(rest) = strip_prefix_ci(part, "filename*=") {
            let rest = rest.trim_matches('"');
            let encoded = rest.splitn(3, '\'').nth(2).unwrap_or(rest);
            let decoded = percent_decode_str(encoded).decode_utf8_lossy().into_owned();
            if !decoded.is_empty() {
                return Some(decoded);
            }
        }
    }
    for part in &parts {
        if let Some(rest) = strip_prefix_ci(part, "filename=") {
            let name = rest.trim_matches('"').trim();
            if !name.is_empty() {
                return Some(percent_decode_str(name).decode_utf8_lossy().into_owned());
            }
        }
    }
    None
}

pub fn read_json_or_keep<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice::<T>(&bytes) {
        Ok(v) => Some(v),
        Err(e) => {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let mut keep = path.as_os_str().to_owned();
            keep.push(format!(".broken-{stamp}"));
            let kept = std::fs::copy(path, &keep).is_ok();
            tracing::warn!(
                "could not read {} ({e}); {}",
                path.display(),
                if kept {
                    format!("a copy was kept as {}", Path::new(&keep).display())
                } else {
                    "no copy could be kept".to_string()
                }
            );
            None
        }
    }
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    match (s.get(..prefix.len()), s.get(prefix.len()..)) {
        (Some(head), Some(rest)) if head.eq_ignore_ascii_case(prefix) => Some(rest),
        _ => None,
    }
}

pub fn filename_from_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let segment = parsed.path_segments()?.rev().find(|s| !s.is_empty())?;
    let decoded = percent_decode_str(segment).decode_utf8_lossy().into_owned();
    if decoded.is_empty() {
        None
    } else {
        Some(decoded)
    }
}

#[cfg(unix)]
pub fn write_all_at(file: &File, buf: &[u8], offset: u64) -> io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.write_all_at(buf, offset)
}

#[cfg(windows)]
pub fn write_all_at(file: &File, mut buf: &[u8], mut offset: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        let n = file.seek_write(buf, offset)?;
        if n == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        buf = &buf[n..];
        offset += n as u64;
    }
    Ok(())
}

#[cfg(windows)]
pub fn make_sparse(file: &File) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;
    use windows_sys::Win32::System::IO::DeviceIoControl;
    let mut returned = 0u32;
    unsafe {
        DeviceIoControl(
            file.as_raw_handle() as _,
            FSCTL_SET_SPARSE,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        );
    }
}

pub fn which(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        dirs.push("/opt/homebrew/bin".into());
        dirs.push("/usr/local/bin".into());
    }
    dirs.into_iter().map(|d| d.join(&exe)).find(|p| p.is_file())
}

pub fn reddit_video_url(url: &str) -> String {
    let Ok(u) = url::Url::parse(url) else {
        return url.to_string();
    };
    if u.host_str() != Some("v.redd.it") {
        return url.to_string();
    }
    let parts: Vec<&str> = u.path().split('/').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        [id] if id.chars().all(|c| c.is_ascii_alphanumeric()) => {
            format!("https://v.redd.it/{id}/HLSPlaylist.m3u8")
        }
        _ => url.to_string(),
    }
}

pub fn media_title(title: Option<&str>, url: &str) -> Option<String> {
    let squash = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase()
    };
    let site = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .map(|h| {
            let labels: Vec<&str> = h.split('.').collect();
            let n = labels.len();
            if n >= 2 {
                labels[n - 2].to_string()
            } else {
                h
            }
        })
        .map(|s| squash(&s))
        .unwrap_or_default();
    let mut t = title?.trim().to_string();
    if t.starts_with('(') {
        if let Some(end) = t.find(')') {
            if t[1..end].chars().all(|c| c.is_ascii_digit() || c == '+') {
                t = t[end + 1..].trim_start().to_string();
            }
        }
    }
    for sep in [" - ", " | ", " • ", " · ", " – ", " — "] {
        if let Some(idx) = t.rfind(sep) {
            let suffix = squash(&t[idx + sep.len()..]);
            if !site.is_empty() && suffix.contains(&site) {
                t = t[..idx].trim().to_string();
                break;
            }
        }
    }
    let flat = squash(&t);
    if flat.is_empty() || flat == site || (!site.is_empty() && flat == format!("{site}com")) {
        return None;
    }
    Some(t)
}

pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn same_dir(a: &str, b: &str) -> bool {
    let norm = |p: &str| {
        p.trim()
            .trim_end_matches(['/', '\\'])
            .replace('\\', "/")
            .to_lowercase()
    };
    norm(a) == norm(b)
}

pub fn web_link(url: &str) -> bool {
    url::Url::parse(url.trim()).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
}

pub fn private_network_link(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url.trim()) else {
        return false;
    };
    match u.host() {
        Some(url::Host::Ipv4(ip)) => {
            ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified()
        }
        Some(url::Host::Ipv6(ip)) => {
            let seg = ip.segments()[0];
            ip.is_loopback()
                || ip.is_unspecified()
                || (seg & 0xfe00) == 0xfc00
                || (seg & 0xffc0) == 0xfe80
                || ip
                    .to_ipv4_mapped()
                    .is_some_and(|v4| v4.is_private() || v4.is_loopback() || v4.is_link_local())
        }
        Some(url::Host::Domain(d)) => {
            let d = d.trim_end_matches('.').to_ascii_lowercase();
            d == "localhost"
                || d.ends_with(".localhost")
                || d.ends_with(".local")
                || d.ends_with(".lan")
                || d.ends_with(".home.arpa")
                || d.ends_with(".internal")
                || !d.contains('.')
        }
        None => false,
    }
}

pub fn insecure_link(url: &str) -> bool {
    url.trim()
        .get(..7)
        .is_some_and(|p| p.eq_ignore_ascii_case("http://"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_a_copy_of_files_it_cannot_read() {
        let dir = std::env::temp_dir().join(format!("mdm-keep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("good.json");
        std::fs::write(&good, b"[1, 2]").unwrap();
        assert_eq!(read_json_or_keep::<Vec<u8>>(&good), Some(vec![1, 2]));
        let bad = dir.join("bad.json");
        std::fs::write(&bad, b"{ not json").unwrap();
        assert_eq!(read_json_or_keep::<Vec<u8>>(&bad), None);
        let kept: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("bad.json.broken-")
            })
            .collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(std::fs::read(kept[0].path()).unwrap(), b"{ not json");
        assert_eq!(
            read_json_or_keep::<Vec<u8>>(&dir.join("missing.json")),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_file_names_from_headers_with_any_characters() {
        assert_eq!(
            strip_prefix_ci("FILENAME=a.zip", "filename="),
            Some("a.zip")
        );
        assert_eq!(strip_prefix_ci("ğğğğğğğğğğ", "filename="), None);
        assert_eq!(strip_prefix_ci("fileğ", "filename="), None);
        assert_eq!(strip_prefix_ci("", "filename="), None);
    }

    #[test]
    fn spots_links_to_the_local_network() {
        for url in [
            "http://192.168.1.1/admin",
            "http://10.0.0.5/file.zip",
            "http://172.20.1.1/",
            "http://127.0.0.1:8080/x",
            "http://localhost/x",
            "http://router/",
            "http://nas.local/share/a.iso",
            "http://[::1]/x",
            "http://[fd00::1]/x",
            "http://169.254.1.1/",
        ] {
            assert!(private_network_link(url), "{url}");
        }
        for url in [
            "https://example.com/file.zip",
            "http://8.8.8.8/x",
            "https://172.32.0.1/",
            "https://[2001:db8::1]/",
            "magnet:?xt=urn:btih:abc",
        ] {
            assert!(!private_network_link(url), "{url}");
        }
    }

    #[test]
    fn compares_folders_loosely() {
        assert!(same_dir(
            "C:\\Users\\a\\Downloads\\",
            "c:/users/a/downloads"
        ));
        assert!(!same_dir(
            "C:\\Users\\a\\Downloads",
            "C:\\Users\\a\\Torrents"
        ));
    }

    #[test]
    fn only_web_links_count() {
        assert!(web_link("https://a.example/x"));
        assert!(web_link("http://a.example"));
        assert!(!web_link("file:///C:/Windows/system32/calc.exe"));
        assert!(!web_link("javascript:alert(1)"));
        assert!(!web_link("ms-settings:"));
    }

    #[test]
    fn spots_unencrypted_links() {
        assert!(insecure_link("http://example.com/a.zip"));
        assert!(insecure_link(" HTTP://example.com"));
        assert!(!insecure_link("https://example.com/a.zip"));
        assert!(!insecure_link("magnet:?xt=urn:btih:abc"));
        assert!(!insecure_link("ftp"));
    }

    #[test]
    fn disposition_parsing() {
        assert_eq!(
            filename_from_disposition("attachment; filename=\"a b.zip\"").as_deref(),
            Some("a b.zip")
        );
        assert_eq!(
            filename_from_disposition("attachment; filename=x.bin; filename*=UTF-8''%C3%A7.zip")
                .as_deref(),
            Some("ç.zip")
        );
    }

    #[test]
    fn sanitizing() {
        assert_eq!(sanitize_filename("a/b:c?.txt"), "a_b_c_.txt");
        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_filename("  "), "download");
        assert_eq!(
            sanitize_filename("invoice\u{202E}fdp.exe"),
            "invoicefdp.exe"
        );
        assert_eq!(
            sanitize_filename("report.pdf                    .exe"),
            "report.pdf .exe"
        );
        assert_eq!(sanitize_filename("a\u{200B}b.zip"), "ab.zip");
    }

    #[test]
    fn reddit_links_become_playlists() {
        assert_eq!(
            reddit_video_url("https://v.redd.it/2kmciqy1xxuh1"),
            "https://v.redd.it/2kmciqy1xxuh1/HLSPlaylist.m3u8"
        );
        assert_eq!(
            reddit_video_url("https://v.redd.it/abc/HLSPlaylist.m3u8?a=1"),
            "https://v.redd.it/abc/HLSPlaylist.m3u8?a=1"
        );
        assert_eq!(
            reddit_video_url("https://www.youtube.com/watch?v=1"),
            "https://www.youtube.com/watch?v=1"
        );
    }

    #[test]
    fn media_titles() {
        let yt = "https://www.youtube.com/watch?v=abc";
        assert_eq!(media_title(Some("YouTube"), yt), None);
        assert_eq!(media_title(Some("(3) YouTube"), yt), None);
        assert_eq!(
            media_title(Some("(12) Big Buck Bunny 4K - YouTube"), yt).as_deref(),
            Some("Big Buck Bunny 4K")
        );
        assert_eq!(
            media_title(Some("Song | Instagram"), "https://www.instagram.com/p/x/").as_deref(),
            Some("Song")
        );
        assert_eq!(
            media_title(Some("Live - Twitch"), "https://m.youtube.com/watch?v=1").as_deref(),
            Some("Live - Twitch")
        );
        assert_eq!(media_title(Some("  "), yt), None);
        assert_eq!(media_title(None, yt), None);
    }

    #[test]
    fn url_names() {
        assert_eq!(
            filename_from_url("https://x.org/dir/file%20name.iso?x=1").as_deref(),
            Some("file name.iso")
        );
        assert_eq!(filename_from_url("https://x.org/"), None);
    }
}
