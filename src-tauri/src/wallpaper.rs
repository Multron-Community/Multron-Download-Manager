use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use base64::Engine;

const PREFIX: &str = "wallpaper-";
const MAX_BYTES: u64 = 30 * 1024 * 1024;
const TYPES: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
    ("bmp", "image/bmp"),
    ("avif", "image/avif"),
];

fn mime(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    TYPES.iter().find(|(e, _)| *e == ext).map(|(_, m)| *m)
}

pub fn store(data: &Path, source: &Path) -> Result<PathBuf> {
    if mime(source).is_none() {
        bail!("choose a PNG, JPEG, WebP, GIF, BMP or AVIF image");
    }
    let size = std::fs::metadata(source)
        .context("the image was not found")?
        .len();
    if size > MAX_BYTES {
        bail!("the image is larger than 30 MB");
    }
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    std::fs::create_dir_all(data)?;
    let target = data.join(format!("{PREFIX}{}.{ext}", uuid::Uuid::new_v4().simple()));
    std::fs::copy(source, &target).context("could not copy the image")?;
    Ok(target)
}

fn owned(data: &Path, path: &Path) -> bool {
    path.parent() == Some(data)
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with(PREFIX))
}

pub fn data_url(data: &Path, path: &Path) -> Option<String> {
    if !owned(data, path) {
        return None;
    }
    let m = mime(path)?;
    let bytes = std::fs::read(path).ok()?;
    Some(format!(
        "data:{m};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

pub fn cleanup(data: &Path, keep: &str) {
    let Ok(dir) = std::fs::read_dir(data) else {
        return;
    };
    for entry in dir.flatten() {
        let p = entry.path();
        if owned(data, &p) && p.to_string_lossy() != keep {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_reads_and_cleans_up_wallpapers() {
        let root = std::env::temp_dir().join(format!("mdm-wallpaper-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let data = root.join("data");
        std::fs::create_dir_all(&root).unwrap();
        let src = root.join("night.PNG");
        std::fs::write(&src, b"fake png").unwrap();
        assert!(store(&data, &root.join("notes.txt")).is_err());
        let first = store(&data, &src).unwrap();
        let second = store(&data, &src).unwrap();
        assert!(data_url(&data, &first)
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert!(data_url(&data, &src).is_none());
        cleanup(&data, &second.to_string_lossy());
        assert!(!first.exists());
        assert!(second.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
