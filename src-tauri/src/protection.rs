use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

const MAX_HISTORY: usize = 2000;
const QUARANTINE_EXT: &str = "quarantined";

const NATIVE_EXTENSIONS: &[&str] = &[
    "exe", "dll", "sys", "ocx", "cpl", "scr", "com", "efi", "mui", "drv", "ax", "node", "pyd",
    "msi", "msp", "mst", "so", "dylib", "bin", "elf", "out", "run", "appimage", "ko", "o", "a",
    "lib", "obj", "pdb", "winmd", "tlb", "rll", "acm", "tsp", "fon", "ime", "flt", "x64", "x86",
    "dat",
];

pub fn hidden_program(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if ext.is_empty() || NATIVE_EXTENSIONS.contains(&ext.as_str()) {
        return false;
    }
    let mut head = [0u8; 4];
    let read = std::fs::File::open(path)
        .and_then(|mut f| std::io::Read::read(&mut f, &mut head))
        .unwrap_or(0);
    if read < 4 {
        return false;
    }
    head.starts_with(b"MZ")
        || head == *b"\x7fELF"
        || matches!(
            head,
            [0xFE, 0xED, 0xFA, 0xCE]
                | [0xFE, 0xED, 0xFA, 0xCF]
                | [0xCE, 0xFA, 0xED, 0xFE]
                | [0xCF, 0xFA, 0xED, 0xFE]
        )
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryEntry {
    pub id: u64,
    pub download: String,
    pub name: String,
    pub state: String,
    pub detail: String,
    pub files: usize,
    pub threats: usize,
    pub at: i64,
    pub quarantined: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct QuarantineItem {
    pub id: u64,
    pub download: String,
    pub name: String,
    pub original: String,
    pub stored: String,
    pub detail: String,
    pub size: u64,
    pub at: i64,
}

pub struct Protection {
    dir: PathBuf,
    history_file: PathBuf,
    quarantine_file: PathBuf,
    history: Mutex<Vec<HistoryEntry>>,
    quarantine: Mutex<Vec<QuarantineItem>>,
    next: AtomicU64,
}

fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> Vec<T> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn write<T: Serialize>(path: &Path, value: &T) {
    let tmp = path.with_extension("json.tmp");
    let res = serde_json::to_vec_pretty(value)
        .map_err(anyhow::Error::from)
        .and_then(|b| std::fs::write(&tmp, b).map_err(Into::into))
        .and_then(|_| std::fs::rename(&tmp, path).map_err(Into::into));
    if let Err(e) = res {
        tracing::warn!("could not save {}: {e:#}", path.display());
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn move_file(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).context("could not copy the file")?;
    std::fs::remove_file(from).context("could not remove the original file")?;
    Ok(())
}

fn free_name(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|s| s.to_str());
    for i in 1..1000 {
        let name = match ext {
            Some(e) => format!("{stem} (restored {i}).{e}"),
            None => format!("{stem} (restored {i})"),
        };
        let candidate = path.with_file_name(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}

impl Protection {
    pub fn open(data: &Path) -> Self {
        let dir = data.join("quarantine");
        let history_file = data.join("scan-history.json");
        let quarantine_file = dir.join("index.json");
        let history: Vec<HistoryEntry> = read(&history_file);
        let quarantine: Vec<QuarantineItem> = read(&quarantine_file);
        let next = history
            .iter()
            .map(|h| h.id)
            .chain(quarantine.iter().map(|q| q.id))
            .max()
            .unwrap_or(0)
            + 1;
        Protection {
            dir,
            history_file,
            quarantine_file,
            history: Mutex::new(history),
            quarantine: Mutex::new(quarantine),
            next: AtomicU64::new(next),
        }
    }

    fn id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    pub fn record(&self, mut entry: HistoryEntry) {
        entry.id = self.id();
        if entry.at == 0 {
            entry.at = now();
        }
        let mut h = self.history.lock();
        h.push(entry);
        let extra = h.len().saturating_sub(MAX_HISTORY);
        if extra > 0 {
            h.drain(..extra);
        }
        write(&self.history_file, &*h);
    }

    pub fn prune(&self, days: u32) -> usize {
        if days == 0 {
            return 0;
        }
        let limit = now() - i64::from(days) * 24 * 3600 * 1000;
        let mut h = self.history.lock();
        let before = h.len();
        h.retain(|e| e.at >= limit);
        let removed = before - h.len();
        if removed > 0 {
            write(&self.history_file, &*h);
        }
        removed
    }

    pub fn history(&self) -> Vec<HistoryEntry> {
        let mut h = self.history.lock().clone();
        h.reverse();
        h
    }

    pub fn clear_history(&self) {
        let mut h = self.history.lock();
        h.clear();
        write(&self.history_file, &*h);
    }

    pub fn quarantine_files(&self, download: &str, files: &[(PathBuf, String)]) -> usize {
        let mut moved = 0;
        let mut list = self.quarantine.lock();
        for (path, detail) in files {
            if !path.is_file() {
                continue;
            }
            let id = self.id();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".into());
            let stored = self.dir.join(format!("{id}.{QUARANTINE_EXT}"));
            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            match move_file(path, &stored) {
                Ok(()) => {
                    list.push(QuarantineItem {
                        id,
                        download: download.to_string(),
                        name,
                        original: path.to_string_lossy().into_owned(),
                        stored: stored.to_string_lossy().into_owned(),
                        detail: detail.clone(),
                        size,
                        at: now(),
                    });
                    moved += 1;
                }
                Err(e) => tracing::warn!("could not quarantine {}: {e:#}", path.display()),
            }
        }
        if moved > 0 {
            write(&self.quarantine_file, &*list);
        }
        moved
    }

    pub fn quarantine(&self) -> Vec<QuarantineItem> {
        let mut q = self.quarantine.lock().clone();
        q.reverse();
        q
    }

    pub fn in_quarantine(&self, download: &str) -> bool {
        self.quarantine
            .lock()
            .iter()
            .any(|q| q.download == download)
    }

    pub fn restore(&self, id: u64) -> Result<QuarantineItem> {
        let mut list = self.quarantine.lock();
        let pos = list
            .iter()
            .position(|q| q.id == id)
            .ok_or_else(|| anyhow!("this file is no longer in quarantine"))?;
        let item = list[pos].clone();
        let target = free_name(Path::new(&item.original));
        move_file(Path::new(&item.stored), &target)?;
        list.remove(pos);
        write(&self.quarantine_file, &*list);
        Ok(item)
    }

    pub fn delete(&self, id: u64) -> Result<QuarantineItem> {
        let mut list = self.quarantine.lock();
        let pos = list
            .iter()
            .position(|q| q.id == id)
            .ok_or_else(|| anyhow!("this file is no longer in quarantine"))?;
        let item = list.remove(pos);
        let stored = Path::new(&item.stored);
        if stored.exists() {
            std::fs::remove_file(stored).context("could not delete the file")?;
        }
        write(&self.quarantine_file, &*list);
        Ok(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spots_programs_with_a_harmless_name() {
        let root = std::env::temp_dir().join(format!("mdm-hidden-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let fake = root.join("invoice.pdf");
        std::fs::write(&fake, b"MZ\x90\x00rest").unwrap();
        let real = root.join("setup.exe");
        std::fs::write(&real, b"MZ\x90\x00rest").unwrap();
        let pdf = root.join("paper.pdf");
        std::fs::write(&pdf, b"%PDF-1.7").unwrap();
        let elf = root.join("song.mp3");
        std::fs::write(&elf, b"\x7fELF\x02").unwrap();
        assert!(hidden_program(&fake));
        assert!(!hidden_program(&real));
        assert!(!hidden_program(&pdf));
        assert!(hidden_program(&elf));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn quarantines_restores_and_deletes_files() {
        let root = std::env::temp_dir().join(format!("mdm-protection-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let downloads = root.join("downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let bad = downloads.join("setup.exe");
        std::fs::write(&bad, b"not really a program").unwrap();
        let p = Protection::open(&root.join("data"));
        assert_eq!(
            p.quarantine_files("dl1", &[(bad.clone(), "Test.Threat".into())]),
            1
        );
        assert!(!bad.exists());
        assert!(p.in_quarantine("dl1"));
        let item = p.quarantine()[0].clone();
        assert!(item.stored.ends_with(".quarantined"));
        let again = Protection::open(&root.join("data"));
        assert_eq!(again.quarantine().len(), 1);
        p.restore(item.id).unwrap();
        assert!(bad.exists());
        assert!(!p.in_quarantine("dl1"));
        assert_eq!(
            p.quarantine_files("dl1", &[(bad.clone(), "Test.Threat".into())]),
            1
        );
        let id = p.quarantine()[0].id;
        p.delete(id).unwrap();
        assert!(p.quarantine().is_empty());
        assert!(!bad.exists());
        p.record(HistoryEntry {
            download: "dl1".into(),
            state: "threat".into(),
            ..Default::default()
        });
        p.record(HistoryEntry {
            download: "dl2".into(),
            state: "clean".into(),
            ..Default::default()
        });
        assert_eq!(p.history()[0].download, "dl2");
        p.record(HistoryEntry {
            download: "old".into(),
            at: 1,
            ..Default::default()
        });
        assert_eq!(p.prune(30), 1);
        assert_eq!(p.prune(0), 0);
        assert_eq!(p.history().len(), 2);
        p.clear_history();
        assert!(p.history().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
