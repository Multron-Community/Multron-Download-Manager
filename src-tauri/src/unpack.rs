use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

const MIN_BUDGET: u64 = 4 * 1024 * 1024 * 1024;
const BUDGET_FACTOR: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub set: String,
    pub first: bool,
}

pub fn volume(name: &str) -> Option<Volume> {
    let lower = name.to_ascii_lowercase();
    let numbered = |stem: &str, num: &str| -> Option<Volume> {
        let n: u32 = num.parse().ok()?;
        Some(Volume {
            set: stem.to_string(),
            first: n == 1,
        })
    };
    if let Some(stem) = lower.strip_suffix(".rar") {
        if let Some((base, part)) = stem.rsplit_once(".part") {
            if !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()) {
                return numbered(&format!("{base}.rar"), part);
            }
        }
        return Some(Volume {
            set: lower.clone(),
            first: true,
        });
    }
    if let Some((stem, ext)) = lower.rsplit_once('.') {
        if ext.len() == 3 && ext.starts_with('r') && ext[1..].chars().all(|c| c.is_ascii_digit()) {
            return Some(Volume {
                set: format!("{stem}.rar"),
                first: false,
            });
        }
        if ext.len() == 3
            && ext.chars().all(|c| c.is_ascii_digit())
            && (stem.ends_with(".7z") || stem.ends_with(".zip"))
        {
            return numbered(stem, ext);
        }
        if ext == "zip" || ext == "7z" {
            return Some(Volume {
                set: lower.clone(),
                first: true,
            });
        }
    }
    None
}

pub fn parts_of(first: &Path) -> Vec<PathBuf> {
    let Some(name) = first.file_name().and_then(|n| n.to_str()) else {
        return vec![first.to_path_buf()];
    };
    let Some(v) = volume(name) else {
        return vec![first.to_path_buf()];
    };
    let dir = first.parent().unwrap_or(Path::new("."));
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .and_then(volume)
                        .is_some_and(|o| o.set == v.set)
                })
                .collect()
        })
        .unwrap_or_default();
    if out.is_empty() {
        out.push(first.to_path_buf());
    }
    out.sort();
    out
}

pub fn target_dir(archive: &Path, subfolder: bool) -> PathBuf {
    let dir = archive.parent().unwrap_or(Path::new(".")).to_path_buf();
    if !subfolder {
        return dir;
    }
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("archive");
    let mut stem = name.to_string();
    let lower = name.to_ascii_lowercase();
    if let Some((base, last)) = lower.rsplit_once('.') {
        let numeric = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
        let archive_ext = numeric(last)
            || (last.len() == 3 && last.starts_with('r') && numeric(&last[1..]))
            || matches!(last, "rar" | "zip" | "7z");
        if archive_ext {
            let cut = base.len();
            stem.truncate(cut);
            let l = stem.to_ascii_lowercase();
            if let Some((b, p)) = l.rsplit_once(".part") {
                if numeric(p) {
                    stem.truncate(b.len());
                }
            }
            for ext in [".rar", ".zip", ".7z"] {
                if stem.to_ascii_lowercase().ends_with(ext) {
                    stem.truncate(stem.len() - ext.len());
                    break;
                }
            }
        }
    }
    let stem = crate::util::sanitize_filename(stem.trim());
    dir.join(if stem.is_empty() {
        "extracted".into()
    } else {
        stem
    })
}

#[derive(Debug)]
pub struct WrongPassword;

impl std::fmt::Display for WrongPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the password is wrong")
    }
}

impl std::error::Error for WrongPassword {}

#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    pub files: usize,
    pub password: Option<String>,
}

fn safe_join(dest: &Path, name: &str) -> Option<PathBuf> {
    let mut out = dest.to_path_buf();
    let mut any = false;
    for c in Path::new(&name.replace('\\', "/")).components() {
        match c {
            Component::Normal(p) => {
                let s = p.to_string_lossy();
                let cleaned: String = s
                    .chars()
                    .map(|c| {
                        if c.is_control() || "<>:\"|?*".contains(c) {
                            '_'
                        } else {
                            c
                        }
                    })
                    .collect();
                let cleaned = cleaned.trim_end_matches(['.', ' ']);
                if cleaned.is_empty() {
                    continue;
                }
                out.push(cleaned);
                any = true;
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    any.then_some(out)
}

struct Budget {
    left: u64,
}

impl Budget {
    fn for_archive(parts: &[PathBuf]) -> Self {
        let size: u64 = parts
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();
        Budget {
            left: size.saturating_mul(BUDGET_FACTOR).max(MIN_BUDGET),
        }
    }

    fn copy(&mut self, mut from: impl Read, to: &Path) -> Result<()> {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(to)?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = from.read(&mut buf)?;
            if n == 0 {
                return Ok(());
            }
            if n as u64 > self.left {
                drop(file);
                let _ = std::fs::remove_file(to);
                bail!("the archive unpacks to far more than its own size, so it was stopped (possible archive bomb)");
            }
            self.left -= n as u64;
            file.write_all(&buf[..n])?;
        }
    }
}

fn zip_native(path: &Path, password: &str, dest: &Path) -> Result<usize> {
    let mut zip = zip::ZipArchive::new(File::open(path)?).context("the ZIP archive is damaged")?;
    let mut budget = Budget::for_archive(&[path.to_path_buf()]);
    let mut count = 0;
    for i in 0..zip.len() {
        let (is_dir, encrypted, name) = {
            let raw = zip.by_index_raw(i)?;
            (
                raw.is_dir(),
                raw.encrypted(),
                raw.name().map(|n| n.into_owned()).unwrap_or_default(),
            )
        };
        let Some(out) = safe_join(dest, &name) else {
            continue;
        };
        if is_dir {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        let entry = if encrypted {
            match zip.by_index_decrypt(i, password.as_bytes()) {
                Ok(e) => e,
                Err(zip::result::ZipError::InvalidPassword) => return Err(WrongPassword.into()),
                Err(e) => return Err(e.into()),
            }
        } else {
            zip.by_index(i)?
        };
        match budget.copy(entry, &out) {
            Ok(()) => count += 1,
            Err(_) if encrypted => {
                let _ = std::fs::remove_file(&out);
                return Err(WrongPassword.into());
            }
            Err(e) => return Err(e),
        }
    }
    Ok(count)
}

fn seven_zip_native(path: &Path, password: &str, dest: &Path) -> Result<usize> {
    let wrong = |e: &sevenz_rust2::Error| {
        let s = e.to_string().to_ascii_lowercase();
        matches!(
            e,
            sevenz_rust2::Error::PasswordRequired | sevenz_rust2::Error::MaybeBadPassword(_)
        ) || s.contains("password")
    };
    let mut reader =
        match sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::from(password)) {
            Ok(r) => r,
            Err(e) if wrong(&e) => return Err(WrongPassword.into()),
            Err(e) => return Err(anyhow!("the 7z archive cannot be opened: {e}")),
        };
    let mut budget = Budget::for_archive(&[path.to_path_buf()]);
    let mut count = 0;
    let mut failure: Option<anyhow::Error> = None;
    let result = reader.for_each_entries(|entry, data| {
        let Some(out) = safe_join(dest, &entry.name) else {
            std::io::copy(data, &mut std::io::sink())?;
            return Ok(true);
        };
        if entry.is_directory {
            let _ = std::fs::create_dir_all(&out);
            return Ok(true);
        }
        if !entry.has_stream {
            return Ok(true);
        }
        match budget.copy(data, &out) {
            Ok(()) => {
                count += 1;
                Ok(true)
            }
            Err(e) => {
                failure = Some(e);
                Ok(false)
            }
        }
    });
    match result {
        Err(e) if wrong(&e) => return Err(WrongPassword.into()),
        Err(e) => return Err(anyhow!("the 7z archive cannot be read: {e}")),
        Ok(()) => {}
    }
    if let Some(e) = failure {
        if !password.is_empty() || e.to_string().contains("archive bomb") {
            return Err(e);
        }
        return Err(WrongPassword.into());
    }
    Ok(count)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    SevenZip,
    Unrar,
}

fn find_tool() -> Option<(PathBuf, Tool)> {
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
            if let Some(dir) = std::env::var_os(var) {
                let p = PathBuf::from(dir).join("7-Zip").join("7z.exe");
                if p.is_file() {
                    return Some((p, Tool::SevenZip));
                }
            }
        }
    }
    for name in ["7zz", "7z", "7za"] {
        if let Some(p) = crate::util::which(name) {
            return Some((p, Tool::SevenZip));
        }
    }
    crate::util::which("unrar").map(|p| (p, Tool::Unrar))
}

pub fn tool_available() -> bool {
    find_tool().is_some()
}

const BOMB: &str =
    "the archive unpacks to far more than its own size, so it was stopped (possible archive bomb)";

fn tool_command(exe: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(exe);
    cmd.stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

fn password_arg(tool: Tool, password: &str) -> String {
    match tool {
        Tool::Unrar if password.is_empty() => "-p-".to_string(),
        _ => format!("-p{password}"),
    }
}

pub fn listed_size(text: &str, tool: Tool) -> Option<u64> {
    let body = match tool {
        Tool::SevenZip => text.split_once("\n----------").map(|(_, rest)| rest)?,
        Tool::Unrar => text,
    };
    let key = match tool {
        Tool::SevenZip => "Size = ",
        Tool::Unrar => "Size: ",
    };
    let mut total = 0u64;
    let mut any = false;
    for line in body.lines() {
        if let Some(v) = line.trim().strip_prefix(key) {
            if let Ok(n) = v.trim().parse::<u64>() {
                total = total.saturating_add(n);
                any = true;
            }
        }
    }
    any.then_some(total)
}

fn list_size(exe: &Path, tool: Tool, path: &Path, password: &str) -> Option<u64> {
    let mut cmd = tool_command(exe);
    match tool {
        Tool::SevenZip => cmd.arg("l").arg("-slt"),
        Tool::Unrar => cmd.arg("lt"),
    };
    cmd.arg(password_arg(tool, password)).arg("--").arg(path);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    listed_size(
        &String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        tool,
    )
}

fn snapshot(dir: &Path) -> (std::collections::HashSet<PathBuf>, u64) {
    let mut seen = std::collections::HashSet::new();
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            match e.metadata() {
                Ok(m) if m.is_dir() => stack.push(p),
                Ok(m) => {
                    total = total.saturating_add(m.len());
                    seen.insert(p);
                }
                Err(_) => {}
            }
            if seen.len() > 200_000 {
                return (seen, total);
            }
        }
    }
    (seen, total)
}

fn remove_new(dir: &Path, before: &std::collections::HashSet<PathBuf>) {
    let (now, _) = snapshot(dir);
    for p in now.difference(before) {
        let _ = std::fs::remove_file(p);
    }
}

fn external(path: &Path, password: &str, dest: &Path) -> Result<usize> {
    let Some((exe, tool)) = find_tool() else {
        bail!("RAR and split archives need 7-Zip (or unrar on Linux and macOS). Install it and the archive is extracted the next time");
    };
    std::fs::create_dir_all(dest)?;
    let budget = Budget::for_archive(&parts_of(path)).left;
    if list_size(&exe, tool, path, password).is_some_and(|n| n > budget) {
        bail!("{BOMB}");
    }
    let (before, before_size) = snapshot(dest);
    let mut cmd = tool_command(&exe);
    match tool {
        Tool::SevenZip => {
            cmd.arg("x")
                .arg("-y")
                .arg("-aoa")
                .arg(password_arg(tool, password));
            cmd.arg(format!("-o{}", dest.display())).arg("--").arg(path);
        }
        Tool::Unrar => {
            cmd.arg("x")
                .arg("-o+")
                .arg("-y")
                .arg(password_arg(tool, password));
            cmd.arg("--")
                .arg(path)
                .arg(format!("{}{}", dest.display(), std::path::MAIN_SEPARATOR));
        }
    }
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().context("could not start the extraction tool")?;
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut p) = pipe {
                let mut buf = Vec::new();
                let _ = p.read_to_end(&mut buf);
                text = String::from_utf8_lossy(&buf).into_owned();
            }
            text
        })
    };
    let out_thread = read(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err_thread = read(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(std::time::Duration::from_millis(700));
        let (_, now) = snapshot(dest);
        if now.saturating_sub(before_size) > budget {
            let _ = child.kill();
            let _ = child.wait();
            remove_new(dest, &before);
            bail!("{BOMB}");
        }
    };
    let text = format!(
        "{}\n{}",
        out_thread.join().unwrap_or_default(),
        err_thread.join().unwrap_or_default()
    )
    .to_ascii_lowercase();
    if !status.success() {
        if text.contains("wrong password")
            || text.contains("incorrect password")
            || text.contains("password is incorrect")
            || text.contains("encrypted") && text.contains("password")
        {
            remove_new(dest, &before);
            return Err(WrongPassword.into());
        }
        let line = text
            .lines()
            .rev()
            .find(|l| l.contains("error") || l.contains("missing") || l.contains("unexpected"))
            .unwrap_or("the extraction tool reported an error")
            .trim()
            .to_string();
        bail!("{line}");
    }
    let (after, after_size) = snapshot(dest);
    if after_size.saturating_sub(before_size) > budget {
        remove_new(dest, &before);
        bail!("{BOMB}");
    }
    Ok(after.difference(&before).count().max(1))
}

fn magic(path: &Path) -> [u8; 6] {
    let mut head = [0u8; 6];
    if let Ok(mut f) = File::open(path) {
        let _ = f.read(&mut head);
    }
    head
}

fn extract_with(path: &Path, password: &str, dest: &Path, split: bool) -> Result<usize> {
    let head = magic(path);
    if !split && head.starts_with(b"PK\x03\x04") {
        return zip_native(path, password, dest);
    }
    if !split && head.starts_with(b"7z\xBC\xAF\x27\x1C") {
        return seven_zip_native(path, password, dest);
    }
    external(path, password, dest)
}

pub fn extract(path: &Path, dest: &Path, passwords: &[String]) -> Result<Outcome> {
    let split = parts_of(path).len() > 1;
    let mut tried: Vec<&str> = vec![""];
    for p in passwords {
        let p = p.trim();
        if !p.is_empty() && !tried.contains(&p) {
            tried.push(p);
        }
    }
    let mut last: Option<anyhow::Error> = None;
    for pw in tried {
        match extract_with(path, pw, dest, split) {
            Ok(files) => {
                return Ok(Outcome {
                    files,
                    password: (!pw.is_empty()).then(|| pw.to_string()),
                })
            }
            Err(e) if e.downcast_ref::<WrongPassword>().is_some() => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| WrongPassword.into()))
}

pub fn is_wrong_password(e: &anyhow::Error) -> bool {
    e.downcast_ref::<WrongPassword>().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mdm-unpack-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn reads_the_unpacked_size_from_the_listing() {
        let seven = "Path = big.rar\nType = Rar5\nPhysical Size = 1000\n\n----------\nPath = a.bin\nSize = 5000000000\nPacked Size = 400\n\nPath = b.bin\nSize = 7\n";
        assert_eq!(listed_size(seven, Tool::SevenZip), Some(5_000_000_007));
        let unrar = "Archive: big.rar\n        Name: a.bin\n        Size: 300\n        Name: b.bin\n        Size: 12\n";
        assert_eq!(listed_size(unrar, Tool::Unrar), Some(312));
        assert_eq!(listed_size("nothing here", Tool::SevenZip), None);
    }

    #[test]
    fn recognizes_archive_volumes() {
        assert_eq!(
            volume("Movie.part1.rar"),
            Some(Volume {
                set: "movie.rar".into(),
                first: true
            })
        );
        assert_eq!(
            volume("Movie.part02.rar"),
            Some(Volume {
                set: "movie.rar".into(),
                first: false
            })
        );
        assert_eq!(
            volume("movie.r00"),
            Some(Volume {
                set: "movie.rar".into(),
                first: false
            })
        );
        assert_eq!(volume("a.7z.001").map(|v| v.first), Some(true));
        assert_eq!(volume("a.7z.002").map(|v| v.first), Some(false));
        assert_eq!(volume("a.zip").map(|v| v.first), Some(true));
        assert_eq!(volume("a.mp4"), None);
        assert_eq!(volume("a.001"), None);
    }

    #[test]
    fn picks_a_folder_named_after_the_archive() {
        let base = Path::new("downloads");
        assert_eq!(
            target_dir(&base.join("Holiday Photos.zip"), true),
            base.join("Holiday Photos")
        );
        assert_eq!(
            target_dir(&base.join("Big.Show.part1.rar"), true),
            base.join("Big.Show")
        );
        assert_eq!(target_dir(&base.join("x.7z.001"), true), base.join("x"));
        assert_eq!(target_dir(&base.join("x.zip"), false), base.to_path_buf());
    }

    #[test]
    fn refuses_paths_that_leave_the_folder() {
        let d = Path::new("out");
        assert_eq!(safe_join(d, "a/b.txt"), Some(d.join("a").join("b.txt")));
        assert_eq!(safe_join(d, "../evil.txt"), None);
        assert_eq!(safe_join(d, "/etc/passwd"), None);
        assert_eq!(safe_join(d, "a\\..\\..\\evil"), None);
    }

    #[test]
    fn extracts_a_zip_with_folders_and_a_password_from_the_list() {
        let dir = scratch("zip");
        let archive = dir.join("pack.zip");
        {
            let mut w = zip::ZipWriter::new(File::create(&archive).unwrap());
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .with_aes_encryption(zip::AesMode::Aes256, "secret");
            w.start_file("inner/hello.txt", opts).unwrap();
            w.write_all(b"hello").unwrap();
            w.finish().unwrap();
        }
        let dest = target_dir(&archive, true);
        let wrong = extract(&archive, &dest, &["nope".into()]).unwrap_err();
        assert!(is_wrong_password(&wrong));
        let out = extract(&archive, &dest, &["nope".into(), "secret".into()]).unwrap();
        assert_eq!(
            out,
            Outcome {
                files: 1,
                password: Some("secret".into())
            }
        );
        assert_eq!(
            std::fs::read(dest.join("inner").join("hello.txt")).unwrap(),
            b"hello"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
