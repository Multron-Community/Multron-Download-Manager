use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

pub const MARKER: &str = "portable.txt";
const OLD_SUFFIX: &str = ".mdm-old";
const DATA: &str = "data";

pub fn dir() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    dir.join(MARKER).is_file().then_some(dir)
}

pub fn data_dir() -> Option<PathBuf> {
    dir().map(|d| d.join(DATA))
}

pub fn extract(archive: &Path, staging: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(relative) = entry.enclosed_name() else {
            bail!("the update contains a file outside its folder and was not installed");
        };
        let out = staging.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut file)?;
    }
    Ok(())
}

pub fn package_root(staging: &Path, exe_name: &str) -> Option<PathBuf> {
    if staging.join(exe_name).is_file() {
        return Some(staging.to_path_buf());
    }
    std::fs::read_dir(staging)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.join(exe_name).is_file())
}

fn files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn old_name(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(OLD_SUFFIX);
    PathBuf::from(name)
}

pub fn replace_files(source: &Path, target: &Path) -> Result<()> {
    let mut done: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    let result = (|| -> Result<()> {
        for file in files(source) {
            let relative = file.strip_prefix(source)?;
            if relative
                .components()
                .next()
                .is_some_and(|c| c.as_os_str() == DATA)
            {
                continue;
            }
            let dest = target.join(relative);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let backup = if dest.exists() {
                let old = old_name(&dest);
                let _ = std::fs::remove_file(&old);
                std::fs::rename(&dest, &old)
                    .with_context(|| format!("could not replace {}", dest.display()))?;
                Some(old)
            } else {
                None
            };
            done.push((dest.clone(), backup));
            if std::fs::rename(&file, &dest).is_err() {
                std::fs::copy(&file, &dest)
                    .with_context(|| format!("could not write {}", dest.display()))?;
            }
        }
        Ok(())
    })();
    if result.is_err() {
        for (dest, backup) in done.into_iter().rev() {
            let _ = std::fs::remove_file(&dest);
            if let Some(old) = backup {
                let _ = std::fs::rename(&old, &dest);
            }
        }
    }
    result
}

pub fn cleanup(dir: &Path) {
    let entries = std::fs::read_dir(dir).into_iter().flatten().flatten();
    let candidates = entries
        .map(|e| e.path())
        .filter(|p| p.file_name().is_none_or(|n| n != DATA))
        .flat_map(|p| if p.is_dir() { files(&p) } else { vec![p] });
    for file in candidates {
        if file.to_string_lossy().ends_with(OLD_SUFFIX) {
            let _ = std::fs::remove_file(file);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mdm-portable-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn zip_with(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, data) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn replaces_program_files_and_keeps_data() {
        let base = temp("replace");
        let install = base.join("install");
        std::fs::create_dir_all(install.join("data")).unwrap();
        std::fs::create_dir_all(install.join("browser-extension")).unwrap();
        std::fs::write(install.join("multron-download-manager.exe"), b"old exe").unwrap();
        std::fs::write(install.join("browser-extension/manifest.json"), b"old").unwrap();
        std::fs::write(install.join("data/settings.json"), b"mine").unwrap();
        std::fs::write(install.join(MARKER), b"portable").unwrap();

        let archive = base.join("update.zip");
        zip_with(
            &archive,
            &[
                (
                    "Multron-Download-Manager/multron-download-manager.exe",
                    b"new exe",
                ),
                (
                    "Multron-Download-Manager/browser-extension/manifest.json",
                    b"new",
                ),
                ("Multron-Download-Manager/data/settings.json", b"theirs"),
                ("Multron-Download-Manager/portable.txt", b"portable"),
            ],
        );
        let staging = base.join("staging");
        extract(&archive, &staging).unwrap();
        let root = package_root(&staging, "multron-download-manager.exe").unwrap();
        replace_files(&root, &install).unwrap();

        assert_eq!(
            std::fs::read(install.join("multron-download-manager.exe")).unwrap(),
            b"new exe"
        );
        assert_eq!(
            std::fs::read(install.join("browser-extension/manifest.json")).unwrap(),
            b"new"
        );
        assert_eq!(
            std::fs::read(install.join("data/settings.json")).unwrap(),
            b"mine"
        );
        assert!(install
            .join("multron-download-manager.exe.mdm-old")
            .exists());
        cleanup(&install);
        assert!(!install
            .join("multron-download-manager.exe.mdm-old")
            .exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn refuses_files_outside_the_package() {
        let base = temp("slip");
        let archive = base.join("evil.zip");
        zip_with(&archive, &[("../../evil.exe", b"x")]);
        assert!(extract(&archive, &base.join("staging")).is_err());
        assert!(!base.parent().unwrap().join("evil.exe").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
