use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use serde::Serialize;

pub const EVT_CHANGED: &str = "captcha://changed";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub id: u64,
    pub url: String,
    pub site: String,
    pub kind: String,
    pub download: Option<String>,
    pub created_at: i64,
}

static NEXT: AtomicU64 = AtomicU64::new(1);
static LIST: Mutex<Vec<Request>> = Mutex::new(Vec::new());

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn site_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.trim_start_matches("www.").to_string())
        })
        .unwrap_or_else(|| url.to_string())
}

fn prune(list: &mut Vec<Request>, timeout_secs: u64, at: i64) {
    let limit = (timeout_secs.max(30) * 1000) as i64;
    list.retain(|r| at - r.created_at < limit);
}

pub fn list(timeout_secs: u64) -> Vec<Request> {
    let mut list = LIST.lock();
    prune(&mut list, timeout_secs, now());
    list.clone()
}

pub fn add(url: &str, kind: &str, download: Option<&str>, timeout_secs: u64) -> Option<Request> {
    let mut list = LIST.lock();
    prune(&mut list, timeout_secs, now());
    let same = |r: &Request| match download {
        Some(d) => r.download.as_deref() == Some(d),
        None => r.download.is_none() && r.url == url,
    };
    if list.iter().any(same) {
        return None;
    }
    let req = Request {
        id: NEXT.fetch_add(1, Ordering::Relaxed),
        url: url.to_string(),
        site: site_of(url),
        kind: kind.to_string(),
        download: download.map(str::to_string),
        created_at: now(),
    };
    list.push(req.clone());
    Some(req)
}

pub fn get(id: u64) -> Option<Request> {
    LIST.lock().iter().find(|r| r.id == id).cloned()
}

pub fn remove(id: u64) -> bool {
    let mut list = LIST.lock();
    let before = list.len();
    list.retain(|r| r.id != id);
    list.len() != before
}

pub fn remove_download(download: &str) -> bool {
    let mut list = LIST.lock();
    let before = list.len();
    list.retain(|r| r.download.as_deref() != Some(download));
    list.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_one_request_per_download_until_it_expires() {
        let first = add(
            "https://www.host.example/f/1",
            "reCAPTCHA",
            Some("dl-test-1"),
            300,
        )
        .unwrap();
        assert_eq!(first.site, "host.example");
        assert!(add(
            "https://www.host.example/f/1",
            "reCAPTCHA",
            Some("dl-test-1"),
            300
        )
        .is_none());
        assert!(list(300)
            .iter()
            .any(|r| r.download.as_deref() == Some("dl-test-1")));
        let mut old = vec![Request {
            created_at: 0,
            ..first.clone()
        }];
        prune(&mut old, 300, 400_000);
        assert!(old.is_empty());
        assert!(remove_download("dl-test-1"));
        assert!(!list(300)
            .iter()
            .any(|r| r.download.as_deref() == Some("dl-test-1")));
    }
}
