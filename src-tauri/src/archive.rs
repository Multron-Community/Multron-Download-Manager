use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

pub const MAX_ENTRY_BYTES: u64 = 100 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 1000;

const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
const SEVEN_ZIP_MAGIC: &[u8] = b"7z\xBC\xAF\x27\x1C";
const RAR_MAGIC: &[u8] = b"Rar!\x1A\x07";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Zip,
    SevenZip,
    Rar,
}

#[derive(Debug)]
pub struct WrongPassword;

impl std::fmt::Display for WrongPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the password is wrong")
    }
}

impl std::error::Error for WrongPassword {}

#[derive(Debug, Default)]
pub struct Extracted {
    pub files: Vec<PathBuf>,
    pub skipped: usize,
}

fn kind_of(path: &Path) -> Option<Kind> {
    let mut head = [0u8; 8];
    let mut file = File::open(path).ok()?;
    let n = file.read(&mut head).ok()?;
    let head = &head[..n];
    if head.starts_with(ZIP_MAGIC) {
        Some(Kind::Zip)
    } else if head.starts_with(SEVEN_ZIP_MAGIC) {
        Some(Kind::SevenZip)
    } else if head.starts_with(RAR_MAGIC) {
        Some(Kind::Rar)
    } else {
        None
    }
}

pub fn locked(path: &Path) -> Option<Kind> {
    match kind_of(path)? {
        Kind::Zip => zip_locked(path).then_some(Kind::Zip),
        Kind::SevenZip => seven_zip_locked(path).then_some(Kind::SevenZip),
        Kind::Rar => rar_locked(path).then_some(Kind::Rar),
    }
}

fn zip_locked(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return false;
    };
    (0..zip.len()).any(|i| zip.by_index_raw(i).is_ok_and(|f| f.encrypted()))
}

fn seven_zip_locked(path: &Path) -> bool {
    let mut reader = match sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())
    {
        Ok(r) => r,
        Err(e) => return is_password_error(&e),
    };
    let mut locked = false;
    let result = reader.for_each_entries(|entry, data| {
        if !entry.has_stream || entry.is_directory || entry.size == 0 {
            return Ok(true);
        }
        let mut probe = [0u8; 1];
        match data.read_exact(&mut probe) {
            Ok(()) => Ok(false),
            Err(e) => {
                locked = true;
                Err(sevenz_rust2::Error::from(e))
            }
        }
    });
    locked || matches!(result, Err(ref e) if is_password_error(e))
}

fn is_password_error(e: &sevenz_rust2::Error) -> bool {
    matches!(
        e,
        sevenz_rust2::Error::PasswordRequired | sevenz_rust2::Error::MaybeBadPassword(_)
    )
}

fn read_vint(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *data.get(*pos)?;
        *pos += 1;
        value |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn rar_locked(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut data = vec![0u8; 64 * 1024];
    let Ok(n) = file.read(&mut data) else {
        return false;
    };
    data.truncate(n);
    if data.starts_with(b"Rar!\x1A\x07\x01\x00") {
        let mut pos = 8 + 4;
        let Some(_size) = read_vint(&data, &mut pos) else {
            return false;
        };
        return read_vint(&data, &mut pos) == Some(4);
    }
    if data.starts_with(b"Rar!\x1A\x07\x00") {
        let mut pos = 7;
        while pos + 7 <= data.len() {
            let kind = data[pos + 2];
            let flags = u16::from_le_bytes([data[pos + 3], data[pos + 4]]);
            let size = u16::from_le_bytes([data[pos + 5], data[pos + 6]]) as usize;
            match kind {
                0x73 if flags & 0x0080 != 0 => return true,
                0x74 => return flags & 0x0004 != 0,
                _ => {}
            }
            if size < 7 {
                return false;
            }
            let mut next = pos + size;
            if flags & 0x8000 != 0 && pos + 11 <= data.len() {
                next += u32::from_le_bytes([
                    data[pos + 7],
                    data[pos + 8],
                    data[pos + 9],
                    data[pos + 10],
                ]) as usize;
            }
            pos = next;
        }
    }
    false
}

fn entry_path(dest: &Path, index: usize, name: &str) -> Result<PathBuf> {
    let base = name
        .rsplit(['/', '\\'])
        .find(|p| !p.is_empty())
        .unwrap_or("file");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(120)
        .collect();
    let cleaned = cleaned.trim_matches(['.', ' ']);
    let folder = dest.join(format!("{index:04}"));
    std::fs::create_dir_all(&folder)?;
    Ok(folder.join(if cleaned.is_empty() { "file" } else { cleaned }))
}

struct Budget {
    total: u64,
    out: Extracted,
}

impl Budget {
    fn allows(&self, size: u64) -> bool {
        size <= MAX_ENTRY_BYTES
            && self.total + size <= MAX_TOTAL_BYTES
            && self.out.files.len() < MAX_ENTRIES
    }
}

fn copy_limited(mut from: impl Read, to: &Path, limit: u64) -> Result<u64> {
    let mut file = File::create(to)?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut written = 0u64;
    loop {
        let n = from.read(&mut buf)?;
        if n == 0 {
            break;
        }
        written += n as u64;
        if written > limit {
            drop(file);
            let _ = std::fs::remove_file(to);
            bail!("an entry is larger than it claims");
        }
        file.write_all(&buf[..n])?;
    }
    Ok(written)
}

pub fn extract(path: &Path, password: &str, dest: &Path) -> Result<Extracted> {
    std::fs::create_dir_all(dest)?;
    match kind_of(path) {
        Some(Kind::Zip) => extract_zip(path, password, dest),
        Some(Kind::SevenZip) => extract_seven_zip(path, password, dest),
        Some(Kind::Rar) => bail!(
            "RAR archives with a password cannot be opened here; extract it with WinRAR or 7-Zip and scan the files"
        ),
        None => bail!("this file is not a ZIP or 7z archive"),
    }
}

fn extract_zip(path: &Path, password: &str, dest: &Path) -> Result<Extracted> {
    let mut zip = zip::ZipArchive::new(File::open(path)?).context("the ZIP archive is damaged")?;
    let mut budget = Budget {
        total: 0,
        out: Extracted::default(),
    };
    for i in 0..zip.len() {
        let (is_dir, size, encrypted, name) = {
            let raw = zip.by_index_raw(i)?;
            (
                raw.is_dir(),
                raw.size(),
                raw.encrypted(),
                raw.name().map(|n| n.into_owned()).unwrap_or_default(),
            )
        };
        if is_dir {
            continue;
        }
        if !budget.allows(size) {
            budget.out.skipped += 1;
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
        let out = entry_path(dest, i, &name)?;
        match copy_limited(entry, &out, size) {
            Ok(n) => {
                budget.total += n;
                budget.out.files.push(out);
            }
            Err(_) if encrypted => {
                let _ = std::fs::remove_file(&out);
                return Err(WrongPassword.into());
            }
            Err(e) => return Err(e),
        }
    }
    Ok(budget.out)
}

fn extract_seven_zip(path: &Path, password: &str, dest: &Path) -> Result<Extracted> {
    let mut reader =
        match sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::from(password)) {
            Ok(r) => r,
            Err(e) if is_password_error(&e) => return Err(WrongPassword.into()),
            Err(e) => return Err(anyhow::anyhow!("the 7z archive cannot be opened: {e}")),
        };
    let mut budget = Budget {
        total: 0,
        out: Extracted::default(),
    };
    let mut index = 0usize;
    let mut failure: Option<anyhow::Error> = None;
    let result = reader.for_each_entries(|entry, data| {
        if !entry.has_stream || entry.is_directory {
            return Ok(true);
        }
        index += 1;
        let size = entry.size;
        if !budget.allows(size) {
            budget.out.skipped += 1;
            std::io::copy(data, &mut std::io::sink())?;
            return Ok(true);
        }
        let copied = entry_path(dest, index, &entry.name)
            .and_then(|out| copy_limited(data, &out, size).map(|n| (out, n)));
        match copied {
            Ok((out, n)) => {
                budget.total += n;
                budget.out.files.push(out);
                Ok(true)
            }
            Err(e) => {
                failure = Some(e);
                Ok(false)
            }
        }
    });
    match result {
        Err(e) if is_password_error(&e) => return Err(WrongPassword.into()),
        Err(e) => return Err(anyhow::anyhow!("the 7z archive cannot be read: {e}")),
        Ok(()) => {}
    }
    if failure.is_some() {
        return Err(WrongPassword.into());
    }
    Ok(budget.out)
}

pub fn is_wrong_password(e: &anyhow::Error) -> bool {
    e.downcast_ref::<WrongPassword>().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mdm-archive-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_zip(path: &Path, mode: zip::AesMode) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        let options = SimpleFileOptions::default().with_aes_encryption(mode, "secret");
        zip.start_file("inner/setup.exe", options).unwrap();
        zip.write_all(b"MZ pretend program").unwrap();
        zip.start_file("../../readme.txt", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"hello").unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn finds_and_opens_password_protected_zips() {
        for (name, mode) in [
            ("aes256", zip::AesMode::Aes256),
            ("aes128", zip::AesMode::Aes128),
        ] {
            let dir = temp(name);
            let archive = dir.join("files.zip");
            write_zip(&archive, mode);
            assert_eq!(locked(&archive), Some(Kind::Zip));
            let err = extract(&archive, "wrong", &dir.join("bad")).unwrap_err();
            assert!(is_wrong_password(&err), "{err:#}");
            let out = extract(&archive, "secret", &dir.join("good")).unwrap();
            assert_eq!(out.files.len(), 2);
            let program = out
                .files
                .iter()
                .find(|f| f.to_string_lossy().ends_with("setup.exe"))
                .unwrap();
            assert_eq!(std::fs::read(program).unwrap(), b"MZ pretend program");
            assert!(out.files.iter().all(|f| f.starts_with(dir.join("good"))));
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn plain_files_are_not_locked() {
        let dir = temp("plain");
        let text = dir.join("a.txt");
        std::fs::write(&text, b"hello").unwrap();
        assert_eq!(locked(&text), None);
        let archive = dir.join("open.zip");
        let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
        zip.start_file("a.txt", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"hello").unwrap();
        zip.finish().unwrap();
        assert_eq!(locked(&archive), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_and_opens_password_protected_7z() {
        let dir = temp("7z");
        let src = dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("tool.exe"), b"MZ another pretend program").unwrap();
        let archive = dir.join("files.7z");
        sevenz_rust2::compress_to_path_encrypted(&src, &archive, "secret".into()).unwrap();
        assert_eq!(locked(&archive), Some(Kind::SevenZip));
        let err = extract(&archive, "wrong", &dir.join("bad")).unwrap_err();
        assert!(is_wrong_password(&err), "{err:#}");
        let out = extract(&archive, "secret", &dir.join("good")).unwrap();
        assert_eq!(out.files.len(), 1);
        assert_eq!(
            std::fs::read(&out.files[0]).unwrap(),
            b"MZ another pretend program"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_rar_encryption_flags() {
        let dir = temp("rar");
        let rar5 = dir.join("locked5.rar");
        std::fs::write(
            &rar5,
            b"Rar!\x1A\x07\x01\x00\x00\x00\x00\x00\x05\x04\x00\x00\x00",
        )
        .unwrap();
        assert_eq!(locked(&rar5), Some(Kind::Rar));
        let open5 = dir.join("open5.rar");
        std::fs::write(
            &open5,
            b"Rar!\x1A\x07\x01\x00\x00\x00\x00\x00\x05\x01\x00\x00\x00",
        )
        .unwrap();
        assert_eq!(locked(&open5), None);
        let err = extract(&rar5, "x", &dir.join("out")).unwrap_err();
        assert!(!is_wrong_password(&err));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_cannot_escape_the_folder() {
        let dir = temp("names");
        let name = |i, n| {
            entry_path(&dir, i, n)
                .unwrap()
                .strip_prefix(&dir)
                .unwrap()
                .to_path_buf()
        };
        assert_eq!(
            name(3, "../../evil.exe"),
            Path::new("0003").join("evil.exe")
        );
        assert_eq!(
            name(4, "C:\\Windows\\a:b.dll"),
            Path::new("0004").join("a_b.dll")
        );
        assert_eq!(name(5, "dir/"), Path::new("0005").join("dir"));
        assert_eq!(name(6, ".."), Path::new("0006").join("file"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
