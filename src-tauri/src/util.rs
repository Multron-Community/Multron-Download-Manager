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

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
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

#[cfg(not(windows))]
pub fn make_sparse(_file: &File) {}

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

#[cfg(test)]
mod tests {
    use super::*;

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
