use std::collections::HashMap;
use std::sync::OnceLock;

const FILES: &[(&str, &str)] = &[
    ("tr", include_str!("../../frontend/src/lang/tr.json")),
    ("de", include_str!("../../frontend/src/lang/de.json")),
    ("fr", include_str!("../../frontend/src/lang/fr.json")),
    ("es", include_str!("../../frontend/src/lang/es.json")),
    ("it", include_str!("../../frontend/src/lang/it.json")),
    ("pt-br", include_str!("../../frontend/src/lang/pt-br.json")),
    ("ru", include_str!("../../frontend/src/lang/ru.json")),
    ("ja", include_str!("../../frontend/src/lang/ja.json")),
    ("zh-cn", include_str!("../../frontend/src/lang/zh-cn.json")),
    ("zh-tw", include_str!("../../frontend/src/lang/zh-tw.json")),
    ("ko", include_str!("../../frontend/src/lang/ko.json")),
];

pub fn languages() -> impl Iterator<Item = &'static str> {
    FILES.iter().map(|(code, _)| *code)
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
        _ => "",
    }
}

fn dictionary(code: &str) -> Option<&'static HashMap<String, String>> {
    static DICTS: OnceLock<HashMap<&'static str, HashMap<String, String>>> = OnceLock::new();
    DICTS
        .get_or_init(|| {
            FILES
                .iter()
                .filter_map(|(code, json)| Some((*code, serde_json::from_str(json).ok()?)))
                .collect()
        })
        .get(code)
}

pub fn resolve(lang: &str) -> String {
    if lang != "auto" {
        return lang.to_string();
    }
    let system = sys_locale::get_locale()
        .unwrap_or_default()
        .to_ascii_lowercase();
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
    candidates.push(system.split(['-', '_']).next().unwrap_or("").to_string());
    candidates
        .into_iter()
        .find(|c| languages().any(|l| l == c))
        .unwrap_or_else(|| "en".into())
}

pub fn t(lang: &str, key: &str) -> String {
    let en = english(key);
    dictionary(&resolve(lang))
        .and_then(|d| d.get(en))
        .cloned()
        .unwrap_or_else(|| en.to_string())
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
    fn every_native_string_is_in_every_language_file() {
        for (code, _) in FILES {
            let dict = dictionary(code).expect("valid JSON");
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
            ] {
                assert!(
                    dict.contains_key(english(key)),
                    "{code}.json lacks {:?}",
                    english(key)
                );
            }
        }
    }
}
