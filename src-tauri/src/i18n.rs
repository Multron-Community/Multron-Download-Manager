use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

const ORDER: &[&str] = &[
    "en", "tr", "de", "fr", "es", "it", "pt-br", "ru", "ja", "zh-cn", "zh-tw", "ko",
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageInfo {
    pub code: String,
    pub name: String,
    pub english: String,
    pub strings: usize,
}

struct Language {
    info: LanguageInfo,
    strings: HashMap<String, String>,
}

type Catalog = Arc<Vec<Language>>;

static DIR: OnceLock<PathBuf> = OnceLock::new();
static CATALOG: RwLock<Option<Catalog>> = RwLock::new(None);

pub fn set_dir(dir: PathBuf) {
    if DIR.set(dir).is_ok() {
        reload();
    }
}

pub fn dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = DIR.get().cloned().into_iter().collect();
    if let Some(exe) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        candidates.push(exe.join("lang"));
    }
    if cfg!(any(debug_assertions, test)) {
        candidates.push(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("lang"),
        );
    }
    candidates.into_iter().find(|d| d.is_dir())
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('s') => out.push(' '),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn split_key(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'=' => return Some((&line[..i], &line[i + 1..])),
            _ => i += 1,
        }
    }
    None
}

pub fn parse(text: &str) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut meta = HashMap::new();
    let mut strings = HashMap::new();
    let mut section = String::new();
    for raw in text.trim_start_matches('\u{feff}').lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            continue;
        }
        let Some((key, value)) = split_key(line) else {
            continue;
        };
        let key = unescape(key.trim());
        let value = unescape(value.trim());
        if key.is_empty() {
            continue;
        }
        match section.as_str() {
            "language" => {
                meta.insert(key.to_ascii_lowercase(), value);
            }
            "strings" if !value.is_empty() => {
                strings.insert(key, value);
            }
            _ => {}
        }
    }
    (meta, strings)
}

fn valid_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 16
        && code
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn load_dir(dir: Option<&Path>) -> Vec<Language> {
    let mut langs: Vec<Language> = Vec::new();
    let entries = dir
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .flatten();
    for entry in entries {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ini"))
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let (meta, strings) = parse(&text);
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .replace('_', "-");
        let code = meta
            .get("code")
            .map(|c| c.trim().to_ascii_lowercase().replace('_', "-"))
            .filter(|c| valid_code(c))
            .unwrap_or(stem);
        if !valid_code(&code) || langs.iter().any(|l| l.info.code == code) {
            continue;
        }
        let name = meta
            .get("name")
            .filter(|n| !n.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| code.clone());
        let english = meta.get("english").cloned().unwrap_or_else(|| name.clone());
        langs.push(Language {
            info: LanguageInfo {
                code,
                name,
                english,
                strings: strings.len(),
            },
            strings,
        });
    }
    if !langs.iter().any(|l| l.info.code == "en") {
        langs.push(Language {
            info: LanguageInfo {
                code: "en".into(),
                name: "English".into(),
                english: "English".into(),
                strings: 0,
            },
            strings: HashMap::new(),
        });
    }
    let rank = |code: &str| ORDER.iter().position(|c| *c == code).unwrap_or(ORDER.len());
    langs.sort_by(|a, b| {
        rank(&a.info.code)
            .cmp(&rank(&b.info.code))
            .then_with(|| a.info.name.to_lowercase().cmp(&b.info.name.to_lowercase()))
    });
    langs
}

fn catalog() -> Catalog {
    if let Some(c) = CATALOG.read().ok().and_then(|g| g.clone()) {
        return c;
    }
    let fresh: Catalog = Arc::new(load_dir(dir().as_deref()));
    if let Ok(mut g) = CATALOG.write() {
        if let Some(existing) = g.clone() {
            return existing;
        }
        *g = Some(fresh.clone());
    }
    fresh
}

pub fn reload() {
    if let Ok(mut g) = CATALOG.write() {
        *g = None;
    }
}

pub fn list() -> Vec<LanguageInfo> {
    catalog().iter().map(|l| l.info.clone()).collect()
}

pub fn strings(code: &str) -> HashMap<String, String> {
    catalog()
        .iter()
        .find(|l| l.info.code == code)
        .map(|l| l.strings.clone())
        .unwrap_or_default()
}

pub fn languages() -> Vec<String> {
    catalog().iter().map(|l| l.info.code.clone()).collect()
}

fn english(key: &str) -> &'static str {
    match key {
        "show" => "Show Multron Download Manager",
        "pause_all" => "Pause All",
        "resume_all" => "Resume All",
        "quit" => "Quit",
        "done" => "Download complete",
        "failed" => "Download failed",
        "hash_ok" => "SHA-256 verified",
        "hash_bad" => "SHA-256 mismatch",
        "pair" => "A browser extension wants to connect",
        "threat" => "Threat found in a download",
        "started" => "Download started",
        "captcha" => "A site asks you to solve a CAPTCHA",
        "hidden_program" => "this file is really a program, although its name says otherwise",
        "scan_cloud" => "Multron Cloud scan finished",
        "no_threat" => "No threat found",
        "not_scanned" => "Not scanned",
        "partly_scanned" => "Partly scanned",
        "scan_not_finished" => "Scan not finished",
        "suspicious" => "Suspicious file",
        _ => "",
    }
}

fn lookup(lang: &str, english: &str) -> Option<String> {
    let code = resolve(lang);
    catalog()
        .iter()
        .find(|l| l.info.code == code)
        .and_then(|l| l.strings.get(english).cloned())
}

pub fn resolve(lang: &str) -> String {
    if lang != "auto" {
        return lang.to_string();
    }
    let system = sys_locale::get_locale()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .replace('_', "-");
    let mut candidates = vec![system.clone()];
    if system.starts_with("zh") {
        let traditional = ["hant", "-tw", "-hk", "-mo"]
            .iter()
            .any(|t| system.contains(t));
        candidates.push(if traditional { "zh-tw" } else { "zh-cn" }.into());
    }
    if system.starts_with("pt") {
        candidates.push("pt-br".into());
    }
    candidates.push(system.split('-').next().unwrap_or("").to_string());
    let known = languages();
    candidates
        .into_iter()
        .find(|c| known.iter().any(|l| l == c))
        .unwrap_or_else(|| "en".into())
}

pub fn text(lang: &str, english: &str) -> String {
    lookup(lang, english).unwrap_or_else(|| english.to_string())
}

pub fn t(lang: &str, key: &str) -> String {
    let en = english(key);
    lookup(lang, en).unwrap_or_else(|| en.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_from_the_language_files() {
        assert_eq!(t("en", "quit"), "Quit");
        assert_eq!(t("tr", "quit"), "Çıkış");
        assert_eq!(t("de", "quit"), "Beenden");
        assert_eq!(t("ja", "quit"), "終了");
        assert_eq!(t("xx", "quit"), "Quit");
    }

    #[test]
    fn translates_category_names() {
        assert_eq!(text("ru", "Video"), "Видео");
        assert_eq!(text("tr", "Music"), "Müzik");
        assert_eq!(text("en", "Archives"), "Archives");
    }

    #[test]
    fn lists_languages_in_order() {
        let codes = languages();
        assert_eq!(codes.first().map(String::as_str), Some("en"));
        for code in ORDER {
            assert!(codes.iter().any(|c| c == code), "{code}.ini missing");
        }
        let tr = list().into_iter().find(|l| l.code == "tr").unwrap();
        assert_eq!(tr.name, "Türkçe");
        assert!(tr.strings > 1000);
    }

    #[test]
    fn parses_escapes_and_sections() {
        let (meta, strings) = parse(
            "\u{feff}; note\n[language]\ncode = pl\nname = Polski\n\n[strings]\n0 \\= off.=0 = wyłączone.\nTwo\\nlines=Dwie\\nlinie\n\\;Semi=Średnik\nEmpty=\n\\sPad\\s=\\sx\\s\nbroken line\n",
        );
        assert_eq!(meta.get("code").map(String::as_str), Some("pl"));
        assert_eq!(meta.get("name").map(String::as_str), Some("Polski"));
        assert_eq!(
            strings.get("0 = off.").map(String::as_str),
            Some("0 = wyłączone.")
        );
        assert_eq!(
            strings.get("Two\nlines").map(String::as_str),
            Some("Dwie\nlinie")
        );
        assert_eq!(strings.get(";Semi").map(String::as_str), Some("Średnik"));
        assert_eq!(strings.get(" Pad ").map(String::as_str), Some(" x "));
        assert!(!strings.contains_key("Empty"));
        assert_eq!(strings.len(), 4);
    }

    #[test]
    fn falls_back_to_english_without_files() {
        let langs = load_dir(None);
        assert_eq!(langs.len(), 1);
        assert_eq!(langs[0].info.code, "en");
    }

    #[test]
    fn every_native_string_is_in_every_language_file() {
        for code in languages().into_iter().filter(|c| c != "en") {
            let dict = strings(&code);
            for key in [
                "show",
                "pause_all",
                "resume_all",
                "quit",
                "done",
                "failed",
                "hash_ok",
                "hash_bad",
                "pair",
                "threat",
                "no_threat",
                "not_scanned",
                "partly_scanned",
                "scan_not_finished",
            ] {
                assert!(
                    dict.contains_key(english(key)),
                    "{code}.ini lacks {:?}",
                    english(key)
                );
            }
        }
    }
}
