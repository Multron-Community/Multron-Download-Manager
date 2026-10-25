use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    Active,
    Paused,
    Completed,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Http,
    Media,
    Torrent,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Video,
    Music,
    Documents,
    Archives,
    Programs,
    Images,
    Other,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum HashState {
    #[default]
    None,
    Pending,
    Verifying,
    Ok,
    Mismatch,
    Computed,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct SegmentState {
    pub start: u64,
    pub end: u64,
    pub pos: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaOptions {
    pub quality: String,
    #[serde(default)]
    pub playlist: bool,
    #[serde(default)]
    pub cookies_file: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partials: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TorrentFile {
    pub name: String,
    pub length: u64,
    pub included: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TorrentOptions {
    #[serde(default)]
    pub info_hash: Option<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub torrent_file: Option<String>,
    #[serde(default)]
    pub only_files: Option<Vec<usize>>,
    #[serde(default)]
    pub files: Vec<TorrentFile>,
    #[serde(default)]
    pub uploaded: u64,
    #[serde(default)]
    pub trackers: Vec<String>,
    #[serde(default)]
    pub user_paused: bool,
    #[serde(default)]
    pub queued: bool,
    #[serde(default)]
    pub seed_done: bool,
    #[serde(default)]
    pub seed_override: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRecord {
    pub id: String,
    pub kind: Kind,
    pub url: String,
    pub name: String,
    pub dir: String,
    pub category: Category,
    pub status: Status,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub downloaded: u64,
    pub threads: u32,
    #[serde(default)]
    pub resumable: bool,
    #[serde(default)]
    pub segments: Vec<SegmentState>,
    pub created_at: i64,
    #[serde(default)]
    pub completed_at: Option<i64>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub referrer: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub cookie_header: Option<String>,
    #[serde(default)]
    pub speed_limit: u64,
    #[serde(default)]
    pub scan_state: String,
    #[serde(default)]
    pub scan_detail: Option<String>,
    #[serde(default)]
    pub cloud_state: String,
    #[serde(default)]
    pub cloud_detail: Option<String>,
    #[serde(default)]
    pub queue_order: i64,
    #[serde(default)]
    pub media: Option<MediaOptions>,
    #[serde(default)]
    pub torrent: Option<TorrentOptions>,
    #[serde(default)]
    pub sha256_expected: Option<String>,
    #[serde(default)]
    pub sha256_actual: Option<String>,
    #[serde(default)]
    pub hash_state: HashState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validator: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Live {
    pub speed: u64,
    pub eta: Option<u64>,
    pub connections: u32,
    pub upload_speed: u64,
    pub peers: u32,
    pub seeding: bool,
    pub phase: Option<String>,
    pub item_index: Option<u32>,
    pub item_count: Option<u32>,
    pub tool_name: Option<String>,
    pub tool_downloaded: u64,
    pub tool_total: Option<u64>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DownloadView {
    #[serde(flatten)]
    pub record: DownloadRecord,
    #[serde(flatten)]
    pub live: Live,
    pub queue_pos: Option<u32>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    #[serde(default = "root_path")]
    pub path: String,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub http_only: bool,
    #[serde(default)]
    pub expiration_date: Option<f64>,
}

fn root_path() -> String {
    "/".into()
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AddRequest {
    pub url: String,
    pub kind: Option<String>,
    pub dir: Option<String>,
    pub filename: Option<String>,
    pub threads: Option<u32>,
    pub quality: Option<String>,
    pub playlist: bool,
    pub referrer: Option<String>,
    pub user_agent: Option<String>,
    pub cookies: Vec<BrowserCookie>,
    pub start_paused: bool,
    pub speed_limit: Option<u64>,
    pub sha256: Option<String>,
    pub torrent_file: Option<String>,
    pub only_files: Option<Vec<usize>>,
    pub page_title: Option<String>,
    pub thumbnail: Option<String>,
    pub total: Option<u64>,
    pub info_hash: Option<String>,
    pub trackers: Vec<String>,
    pub allow_duplicate: bool,
    pub audio_url: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Duplicate {
    pub id: String,
    pub name: String,
    pub status: Status,
}

pub fn same_link(a: &str, b: &str) -> bool {
    let clean = |u: &str| {
        u.trim()
            .split('#')
            .next()
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string()
    };
    let (a, b) = (clean(a), clean(b));
    !a.is_empty() && a.eq_ignore_ascii_case(&b)
}

pub fn detect_kind(url: &str) -> Kind {
    let lower = url.trim().to_ascii_lowercase();
    if lower.starts_with("magnet:") || lower.ends_with(".torrent") {
        return Kind::Torrent;
    }
    if is_media_site(&lower) {
        return Kind::Media;
    }
    Kind::Http
}

const MEDIA_HOSTS: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "music.youtube.com",
    "reddit.com",
    "redd.it",
    "v.redd.it",
    "instagram.com",
    "tiktok.com",
    "twitter.com",
    "x.com",
    "facebook.com",
    "fb.watch",
    "vimeo.com",
    "dailymotion.com",
    "twitch.tv",
    "soundcloud.com",
    "bilibili.com",
    "streamable.com",
    "bandcamp.com",
    "pinterest.com",
    "threads.net",
    "kick.com",
];

pub fn is_media_site(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.trim_start_matches("www.").trim_start_matches("m.");
    MEDIA_HOSTS
        .iter()
        .any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_the_same_link() {
        assert!(same_link(
            "https://example.org/setup.exe",
            " https://EXAMPLE.org/setup.exe#top "
        ));
        assert!(same_link(
            "https://example.org/files/",
            "https://example.org/files"
        ));
        assert!(!same_link(
            "https://example.org/setup.exe",
            "https://example.org/setup.exe?v=2"
        ));
        assert!(!same_link("", ""));
    }

    #[test]
    fn kinds() {
        assert_eq!(detect_kind("magnet:?xt=urn:btih:abc"), Kind::Torrent);
        assert_eq!(
            detect_kind("https://www.youtube.com/watch?v=x"),
            Kind::Media
        );
        assert_eq!(
            detect_kind("https://old.reddit.com/r/a/comments/x"),
            Kind::Media
        );
        assert_eq!(detect_kind("https://example.com/a.zip"), Kind::Http);
        assert_eq!(detect_kind("https://example.com/a.torrent"), Kind::Torrent);
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PageBatch {
    pub page_url: String,
    pub page_title: Option<String>,
    pub items: Vec<BatchItem>,
    pub user_agent: Option<String>,
    pub cookies: Vec<BrowserCookie>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BatchItem {
    pub url: String,
    pub kind: String,
    pub media_type: String,
    pub name: Option<String>,
    pub filename: Option<String>,
    pub thumbnail: Option<String>,
    pub referrer: Option<String>,
}
