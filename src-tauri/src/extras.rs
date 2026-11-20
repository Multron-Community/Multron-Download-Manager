use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::model::DownloadRecord;
use crate::settings::{FileRule, Settings};

const MAX_HISTORY: usize = 20_000;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryItem {
    pub id: String,
    pub name: String,
    pub url: String,
    pub size: Option<u64>,
    pub kind: String,
    pub host: String,
    pub dir: String,
    pub at: i64,
}

pub struct History {
    file: PathBuf,
    items: Mutex<Vec<HistoryItem>>,
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.trim_start_matches("www.").to_string())
        })
        .unwrap_or_default()
}

impl History {
    pub fn open(data: &Path) -> Self {
        let file = data.join("download-history.json");
        let items = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        History {
            file,
            items: Mutex::new(items),
        }
    }

    fn save(&self, items: &[HistoryItem]) {
        let tmp = self.file.with_extension("json.tmp");
        let res = serde_json::to_vec(items)
            .map_err(anyhow::Error::from)
            .and_then(|b| std::fs::write(&tmp, b).map_err(Into::into))
            .and_then(|_| std::fs::rename(&tmp, &self.file).map_err(Into::into));
        if let Err(e) = res {
            tracing::warn!("could not save the download history: {e:#}");
        }
    }

    pub fn record(&self, rec: &DownloadRecord, at: i64) {
        let mut items = self.items.lock();
        items.retain(|i| i.id != rec.id);
        items.push(HistoryItem {
            id: rec.id.clone(),
            name: rec.name.clone(),
            url: rec.url.clone(),
            size: rec.total,
            kind: format!("{:?}", rec.kind).to_lowercase(),
            host: host_of(&rec.url),
            dir: rec.dir.clone(),
            at,
        });
        let extra = items.len().saturating_sub(MAX_HISTORY);
        if extra > 0 {
            items.drain(..extra);
        }
        self.save(&items);
    }

    pub fn list(&self) -> Vec<HistoryItem> {
        let mut v = self.items.lock().clone();
        v.reverse();
        v
    }

    pub fn find(&self, url: &str) -> Option<HistoryItem> {
        if url.trim().is_empty() {
            return None;
        }
        self.items
            .lock()
            .iter()
            .rev()
            .find(|i| crate::model::same_link(&i.url, url))
            .cloned()
    }

    pub fn remove(&self, id: &str) {
        let mut items = self.items.lock();
        items.retain(|i| i.id != id);
        self.save(&items);
    }

    pub fn clear(&self) {
        let mut items = self.items.lock();
        items.clear();
        self.save(&items);
    }
}

fn wildcard(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    if !p.contains(&'*') && !p.contains(&'?') {
        return text.to_lowercase().contains(&pattern.to_lowercase());
    }
    let (mut pi, mut ti, mut star, mut mark) = (0usize, 0usize, None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn rule_matches(rule: &FileRule, url: &str, name: &str) -> bool {
    if !rule.enabled || rule.pattern.trim().is_empty() {
        return false;
    }
    match rule.field.as_str() {
        "host" => {
            let host = host_of(url);
            rule.pattern
                .split([',', ' '])
                .map(|p| p.trim().trim_start_matches("www.").to_lowercase())
                .filter(|p| !p.is_empty())
                .any(|p| host == p || host.ends_with(&format!(".{p}")))
        }
        "ext" => {
            let ext = crate::util::extension(name);
            rule.pattern
                .split([',', ' '])
                .map(|p| p.trim().trim_start_matches('.').to_lowercase())
                .any(|p| !p.is_empty() && p == ext)
        }
        _ => wildcard(rule.pattern.trim(), name),
    }
}

pub fn render_name(template: &str, name: &str, url: &str, category: &str) -> String {
    let ext = crate::util::extension(name);
    let stem = if ext.is_empty() {
        name.to_string()
    } else {
        name[..name.len() - ext.len() - 1].to_string()
    };
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut out = template
        .replace("{name}", &stem)
        .replace("{ext}", &ext)
        .replace("{date}", &date)
        .replace("{host}", &host_of(url))
        .replace("{category}", category);
    if !ext.is_empty()
        && !template.contains("{ext}")
        && !out.to_lowercase().ends_with(&format!(".{ext}"))
    {
        out = format!("{out}.{ext}");
    }
    crate::util::sanitize_filename(out.trim())
}

fn free_path(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_string);
    for i in 1..1000 {
        let n = match &ext {
            Some(e) => format!("{stem} ({i}).{e}"),
            None => format!("{stem} ({i})"),
        };
        let c = path.with_file_name(n);
        if !c.exists() {
            return c;
        }
    }
    path
}

pub fn apply_rule(rule: &FileRule, file: &Path, url: &str, category: &str) -> Result<PathBuf> {
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let target = match rule.action.as_str() {
        "rename" => {
            let new = render_name(&rule.target, name, url, category);
            if new.is_empty() {
                bail!("the new name is empty");
            }
            file.with_file_name(new)
        }
        _ => {
            let dir = PathBuf::from(rule.target.trim());
            if rule.target.trim().is_empty() {
                bail!("the rule has no folder");
            }
            std::fs::create_dir_all(&dir).context("could not create the folder")?;
            dir.join(name)
        }
    };
    if target == file {
        return Ok(target);
    }
    let target = free_path(target);
    if std::fs::rename(file, &target).is_err() {
        std::fs::copy(file, &target).context("could not copy the file")?;
        std::fs::remove_file(file).context("could not remove the original file")?;
    }
    Ok(target)
}

#[cfg(windows)]
pub fn signature(path: &Path) -> String {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$s = Get-AuthenticodeSignature -LiteralPath $env:MDM_FILE; $p = if ($s.SignerCertificate) { $s.SignerCertificate.GetNameInfo('SimpleName', $false) } else { '' }; Write-Output \"$($s.Status)|$p\"",
        ])
        .env("MDM_FILE", path)
        .stdin(std::process::Stdio::null())
        .creation_flags(0x0800_0000)
        .output();
    let Ok(out) = out else {
        return String::new();
    };
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let (status, publisher) = text.split_once('|').unwrap_or((text.as_str(), ""));
    match status {
        "Valid" => format!("valid:{}", publisher.trim()),
        "NotSigned" => "unsigned".into(),
        "" => String::new(),
        _ => "invalid".into(),
    }
}

#[cfg(not(windows))]
pub fn signature(_path: &Path) -> String {
    String::new()
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Backup {
    pub app: String,
    pub version: String,
    pub created: i64,
    pub settings: Option<serde_json::Value>,
    pub downloads: Vec<DownloadRecord>,
    pub history: Vec<HistoryItem>,
}

pub fn strip_secrets(s: &mut Settings) {
    for l in &mut s.site_logins {
        l.password.clear();
    }
    s.virustotal_key.clear();
    s.virustotal_key_protected.clear();
    s.extract_passwords.clear();
}

pub fn keep_secrets(imported: &mut Settings, current: &Settings) {
    for l in &mut imported.site_logins {
        if l.password.is_empty() {
            if let Some(c) = current.site_logins.iter().find(|c| c.host == l.host) {
                l.password = c.password.clone();
            }
        }
    }
    if imported.virustotal_key.is_empty() {
        imported.virustotal_key = current.virustotal_key.clone();
    }
    if imported.extract_passwords.is_empty() {
        imported.extract_passwords = current.extract_passwords.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(field: &str, pattern: &str, action: &str, target: &str) -> FileRule {
        FileRule {
            enabled: true,
            field: field.into(),
            pattern: pattern.into(),
            action: action.into(),
            target: target.into(),
        }
    }

    #[test]
    fn matches_rules_by_host_type_and_name() {
        assert!(rule_matches(
            &rule("host", "example.com", "move", "x"),
            "https://cdn.example.com/a.zip",
            "a.zip"
        ));
        assert!(!rule_matches(
            &rule("host", "example.com", "move", "x"),
            "https://badexample.com/a.zip",
            "a.zip"
        ));
        assert!(rule_matches(
            &rule("ext", "pdf, .epub", "move", "x"),
            "https://a/b",
            "Book.EPUB"
        ));
        assert!(rule_matches(
            &rule("name", "*s01e??*", "move", "x"),
            "",
            "Show.S01E05.1080p.mkv"
        ));
        assert!(rule_matches(
            &rule("name", "invoice", "move", "x"),
            "",
            "my-Invoice-2024.pdf"
        ));
        assert!(!rule_matches(
            &FileRule {
                enabled: false,
                ..rule("ext", "pdf", "move", "x")
            },
            "",
            "a.pdf"
        ));
    }

    #[test]
    fn renders_names_from_templates() {
        let n = render_name(
            "{host} - {name}",
            "Report.PDF",
            "https://www.files.example/x",
            "documents",
        );
        assert_eq!(n, "files.example - Report.pdf");
        assert_eq!(render_name("{name}.{ext}", "a.tar", "", ""), "a.tar");
    }

    #[test]
    fn moves_and_renames_files() {
        let root = std::env::temp_dir().join(format!("mdm-rules-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let f = root.join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        let moved = apply_rule(
            &rule("ext", "txt", "move", &root.join("texts").to_string_lossy()),
            &f,
            "",
            "",
        )
        .unwrap();
        assert_eq!(moved, root.join("texts").join("a.txt"));
        let renamed =
            apply_rule(&rule("ext", "txt", "rename", "note-{name}"), &moved, "", "").unwrap();
        assert_eq!(renamed.file_name().unwrap(), "note-a.txt");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn keeps_a_short_history_and_finds_links() {
        let root = std::env::temp_dir().join(format!("mdm-hist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let h = History::open(&root);
        let rec = DownloadRecord {
            id: "x".into(),
            url: "https://a.example/f.zip".into(),
            name: "f.zip".into(),
            ..crate::manager::blank_record_for_tests("https://a.example/f.zip")
        };
        h.record(&rec, 5);
        assert_eq!(h.find("https://a.example/f.zip").map(|i| i.at), Some(5));
        assert!(History::open(&root)
            .find("https://a.example/f.zip")
            .is_some());
        h.remove("x");
        assert!(h.list().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}

pub const AUTO_BACKUP_PREFIX: &str = "multron-backup-";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupFile {
    pub path: String,
    pub name: String,
    pub created: i64,
    pub size: u64,
}

pub fn backup_interval_days(count: u32, unit: &str) -> i64 {
    let per = match unit {
        "day" => 1,
        "week" => 7,
        "year" => 365,
        _ => 30,
    };
    i64::from(count.max(1)) * per
}

pub fn auto_backups(dir: &Path) -> Vec<BackupFile> {
    let mut out: Vec<BackupFile> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with(AUTO_BACKUP_PREFIX) || !name.ends_with(".json") {
                return None;
            }
            let meta = e.metadata().ok()?;
            let created = meta
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_millis() as i64;
            Some(BackupFile {
                path: e.path().to_string_lossy().into_owned(),
                name,
                created,
                size: meta.len(),
            })
        })
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.created));
    out
}

pub fn backup_due(dir: &Path, interval_days: i64, now_ms: i64) -> bool {
    match auto_backups(dir).first() {
        Some(last) => now_ms - last.created >= interval_days * 24 * 3600 * 1000 - 3600 * 1000,
        None => true,
    }
}

pub fn prune_backups(dir: &Path, keep: u32) -> usize {
    let mut removed = 0;
    for old in auto_backups(dir).into_iter().skip(keep.max(1) as usize) {
        if std::fs::remove_file(&old.path).is_ok() {
            removed += 1;
        }
    }
    removed
}

pub fn auto_backup_name(now: chrono::DateTime<chrono::Local>) -> String {
    format!("{AUTO_BACKUP_PREFIX}{}.json", now.format("%Y-%m-%d_%H-%M"))
}

#[cfg(test)]
mod auto_backup_tests {
    use super::*;

    #[test]
    fn plans_and_prunes_automatic_backups() {
        assert_eq!(backup_interval_days(1, "week"), 7);
        assert_eq!(backup_interval_days(3, "month"), 90);
        assert_eq!(backup_interval_days(6, "month"), 180);
        assert_eq!(backup_interval_days(2, "year"), 730);
        let dir = std::env::temp_dir().join(format!("mdm-autobackup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        assert!(backup_due(&dir, 7, now));
        for i in 0..4 {
            std::fs::write(dir.join(format!("{AUTO_BACKUP_PREFIX}{i}.json")), b"{}").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::fs::write(dir.join("other.json"), b"{}").unwrap();
        assert!(!backup_due(&dir, 7, now));
        assert!(backup_due(&dir, 7, now + 8 * 24 * 3600 * 1000));
        assert_eq!(auto_backups(&dir).len(), 4);
        assert_eq!(prune_backups(&dir, 2), 2);
        let left: Vec<String> = auto_backups(&dir).into_iter().map(|b| b.name).collect();
        assert_eq!(
            left,
            vec![
                format!("{AUTO_BACKUP_PREFIX}3.json"),
                format!("{AUTO_BACKUP_PREFIX}2.json")
            ]
        );
        assert!(dir.join("other.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
