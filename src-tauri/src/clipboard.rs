use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;

use crate::manager::Manager;

pub const EVT_CLIPBOARD: &str = "app://clipboard-links";
const MAX_LINKS: usize = 500;

pub fn links_in(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for word in text.split(|c: char| c.is_whitespace() || matches!(c, '"' | '<' | '>' | '`')) {
        let w = word.trim_matches(|c: char| matches!(c, ',' | ';' | '(' | ')' | '\'' | '.'));
        let lower = w.to_ascii_lowercase();
        let link = if lower.starts_with("magnet:?") {
            Some(w.to_string())
        } else if lower.starts_with("http://") || lower.starts_with("https://") {
            url::Url::parse(w)
                .ok()
                .filter(|u| u.host_str().is_some_and(|h| h.contains('.')))
                .map(|_| w.to_string())
        } else {
            None
        };
        if let Some(l) = link {
            if seen.insert(l.clone()) {
                out.push(l);
                if out.len() >= MAX_LINKS {
                    break;
                }
            }
        }
    }
    out
}

pub fn watch(mgr: Arc<Manager>) {
    std::thread::spawn(move || {
        let mut board: Option<arboard::Clipboard> = None;
        let mut last: Option<String> = None;
        loop {
            std::thread::sleep(Duration::from_millis(1000));
            if !mgr.settings().clipboard_monitor {
                board = None;
                last = None;
                continue;
            }
            if board.is_none() {
                board = arboard::Clipboard::new().ok();
            }
            let Some(b) = board.as_mut() else { continue };
            let text = match b.get_text() {
                Ok(t) => t,
                Err(arboard::Error::ContentNotAvailable) => String::new(),
                Err(_) => {
                    board = None;
                    continue;
                }
            };
            match &last {
                None => {
                    last = Some(text);
                    continue;
                }
                Some(prev) if *prev == text => continue,
                _ => {}
            }
            last = Some(text.clone());
            let known: HashSet<String> = mgr.views().into_iter().map(|v| v.record.url).collect();
            let links: Vec<String> = links_in(&text)
                .into_iter()
                .filter(|l| !known.contains(l))
                .collect();
            if !links.is_empty() {
                let _ = mgr.app.emit(EVT_CLIPBOARD, links);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_links_in_copied_text() {
        let text = "Parts:\nhttps://files.example.com/a.part1.rar, https://files.example.com/a.part2.rar.\n(magnet:?xt=urn:btih:abc) see https://files.example.com/a.part1.rar again\nnot a link: http://localhost or example.com";
        assert_eq!(
            links_in(text),
            vec![
                "https://files.example.com/a.part1.rar",
                "https://files.example.com/a.part2.rar",
                "magnet:?xt=urn:btih:abc",
            ]
        );
        assert!(links_in("just words").is_empty());
    }
}
