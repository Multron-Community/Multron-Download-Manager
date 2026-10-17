use std::path::Path;

use serde::{Deserialize, Serialize};

pub const DEFAULT_THREADS: u32 = 8;
pub const MAX_THREADS: u32 = 64;
pub const DEFAULT_BRIDGE_PORT: u16 = 6801;
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub language: String,
    pub theme: String,
    pub accent_color: String,
    pub transparency: bool,
    pub backdrop_material: String,
    pub transparency_level: u8,
    pub download_dir: String,
    pub organize_by_category: bool,
    pub close_to_tray: bool,
    pub start_minimized: bool,
    pub launch_at_startup: bool,
    pub notifications: bool,
    pub resume_on_startup: bool,
    pub check_updates_on_startup: bool,
    pub auto_update: bool,

    pub threads: u32,
    pub max_concurrent: u32,
    pub sequential: bool,
    pub speed_limit: u64,
    pub retries: u32,
    pub user_agent: String,

    pub ask_sha256: bool,
    pub auto_detect_sha256: bool,

    pub bridge_enabled: bool,
    pub bridge_port: u16,
    pub confirm_browser_downloads: bool,

    pub ytdlp_auto_update: bool,
    pub media_quality: String,
    pub media_container: String,
    pub cookies_from_browser: String,
    pub embed_metadata: bool,
    pub embed_thumbnail: bool,

    pub torrent_port: u16,
    pub torrent_dht: bool,
    pub torrent_upnp: bool,
    pub torrent_utp: bool,
    pub torrent_peer_limit: u32,
    pub torrent_download_limit: u64,
    pub torrent_upload_limit: u64,
    pub seed_after_complete: bool,
    pub seed_ratio: f64,
    pub seed_time_limit: u32,
    pub seed_limit_action: String,
    pub torrent_max_active_downloads: u32,
    pub torrent_max_active_seeds: u32,
    pub alt_speed_enabled: bool,
    pub alt_download_limit: u64,
    pub alt_upload_limit: u64,
    pub alt_schedule_enabled: bool,
    pub alt_schedule_from: String,
    pub alt_schedule_to: String,
    pub alt_schedule_days: Vec<u8>,
    pub watch_folder_enabled: bool,
    pub watch_folder: String,
    pub rss_enabled: bool,
    pub rss_interval: u32,
    pub rss_feeds: Vec<RssFeed>,
    pub rss_rules: Vec<RssRule>,
    pub on_complete_command: String,
    pub when_done: String,
    pub mark_downloads: bool,
    pub scan_downloads: bool,
    #[serde(default)]
    pub settings_version: u32,
    pub extra_trackers: Vec<String>,

    pub torrent_proxy: String,
    pub torrent_bind_interface: String,
    pub torrent_ipv4_only: bool,
    pub torrent_lsd: bool,
    pub torrent_disable_trackers: bool,
    pub torrent_hide_client: bool,
    pub torrent_scrape: bool,

    pub ip_filter_enabled: bool,
    pub ip_filter_allow_mode: bool,
    pub ip_filter_source: String,
    pub ip_filter_source_v6: String,
    pub ip_filter_manual: String,
    pub ip_filter_reload_days: u32,

    pub torrent_enabled: bool,
    pub media_enabled: bool,

    pub tls_accept_invalid_certs: bool,
    pub block_insecure_redirects: bool,
    pub bridge_require_pairing: bool,
    pub trusted_extensions: Vec<String>,
    pub extension_setup_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "auto".into(),
            theme: "system".into(),
            accent_color: "blue".into(),
            transparency: true,
            backdrop_material: "auto".into(),
            transparency_level: 1,
            download_dir: String::new(),
            organize_by_category: false,
            close_to_tray: true,
            start_minimized: false,
            launch_at_startup: false,
            notifications: true,
            resume_on_startup: true,
            check_updates_on_startup: true,
            auto_update: true,

            threads: DEFAULT_THREADS,
            max_concurrent: 4,
            sequential: false,
            speed_limit: 0,
            retries: 6,
            user_agent: DEFAULT_USER_AGENT.into(),

            ask_sha256: false,
            auto_detect_sha256: true,

            bridge_enabled: true,
            bridge_port: DEFAULT_BRIDGE_PORT,
            confirm_browser_downloads: false,

            ytdlp_auto_update: true,
            media_quality: "best".into(),
            media_container: "mp4".into(),
            cookies_from_browser: String::new(),
            embed_metadata: true,
            embed_thumbnail: false,

            torrent_port: 22155,
            torrent_dht: true,
            torrent_upnp: true,
            torrent_utp: true,
            torrent_peer_limit: 200,
            torrent_download_limit: 0,
            torrent_upload_limit: 0,
            seed_after_complete: true,
            seed_ratio: 2.0,
            seed_time_limit: 0,
            seed_limit_action: "pause".into(),
            torrent_max_active_downloads: 5,
            torrent_max_active_seeds: 0,
            alt_speed_enabled: false,
            alt_download_limit: 1024,
            alt_upload_limit: 256,
            alt_schedule_enabled: false,
            alt_schedule_from: "08:00".into(),
            alt_schedule_to: "20:00".into(),
            alt_schedule_days: vec![0, 1, 2, 3, 4, 5, 6],
            watch_folder_enabled: false,
            watch_folder: String::new(),
            rss_enabled: true,
            rss_interval: 30,
            rss_feeds: Vec::new(),
            rss_rules: Vec::new(),
            on_complete_command: String::new(),
            when_done: "nothing".into(),
            mark_downloads: true,
            scan_downloads: true,
            settings_version: SETTINGS_VERSION,
            extra_trackers: Vec::new(),

            torrent_proxy: String::new(),
            torrent_bind_interface: String::new(),
            torrent_ipv4_only: false,
            torrent_lsd: true,
            torrent_disable_trackers: false,
            torrent_hide_client: false,
            torrent_scrape: true,

            ip_filter_enabled: false,
            ip_filter_allow_mode: false,
            ip_filter_source: String::new(),
            ip_filter_source_v6: String::new(),
            ip_filter_manual: String::new(),
            ip_filter_reload_days: 7,

            torrent_enabled: true,
            media_enabled: true,

            tls_accept_invalid_certs: false,
            block_insecure_redirects: true,
            bridge_require_pairing: true,
            trusted_extensions: Vec::new(),
            extension_setup_done: false,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
            .unwrap_or_default()
            .normalized()
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn normalized(mut self) -> Self {
        if self.settings_version < 1 {
            self.confirm_browser_downloads = false;
        }
        self.settings_version = SETTINGS_VERSION;
        self.auto_update = true;
        self.check_updates_on_startup = true;
        self.ytdlp_auto_update = true;
        self.threads = self.threads.clamp(1, MAX_THREADS);
        self.max_concurrent = self.max_concurrent.clamp(1, 32);
        self.retries = self.retries.clamp(0, 50);
        if self.language != "en"
            && self.language != "auto"
            && !crate::i18n::languages().any(|l| l == self.language)
        {
            self.language = "en".into();
        }
        if !["system", "dark", "light"].contains(&self.theme.as_str()) {
            self.theme = "system".into();
        }
        const ACCENTS: [&str; 9] = [
            "blue", "cyan", "green", "orange", "purple", "red", "pink", "teal", "gray",
        ];
        if !ACCENTS.contains(&self.accent_color.as_str()) {
            self.accent_color = "blue".into();
        }
        self.transparency_level = self.transparency_level.min(2);
        let materials = crate::appearance::support().materials;
        if !materials.contains(&self.backdrop_material.as_str()) {
            self.backdrop_material = materials.first().copied().unwrap_or("none").into();
        }
        if self.bridge_port == 0 {
            self.bridge_port = DEFAULT_BRIDGE_PORT;
        }
        if !["mp4", "mkv", "webm"].contains(&self.media_container.as_str()) {
            self.media_container = "mp4".into();
        }
        if !["pause", "remove", "delete"].contains(&self.seed_limit_action.as_str()) {
            self.seed_limit_action = "pause".into();
        }
        if !["nothing", "sleep", "shutdown", "quit"].contains(&self.when_done.as_str()) {
            self.when_done = "nothing".into();
        }
        self.rss_interval = self.rss_interval.clamp(5, 1440);
        self.alt_schedule_days.retain(|d| *d < 7);
        self.alt_schedule_days.sort_unstable();
        self.alt_schedule_days.dedup();
        for t in [&mut self.alt_schedule_from, &mut self.alt_schedule_to] {
            if parse_hhmm(t).is_none() {
                *t = "00:00".into();
            }
        }
        self.rss_feeds.retain(|f| !f.url.trim().is_empty());
        if self.seed_ratio < 0.0 || !self.seed_ratio.is_finite() {
            self.seed_ratio = 0.0;
        }
        self.extra_trackers = self
            .extra_trackers
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        self.extra_trackers.dedup();
        self
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RssFeed {
    pub url: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RssRule {
    pub name: String,
    pub enabled: bool,
    pub must_contain: String,
    pub must_not_contain: String,
    pub use_regex: bool,
    pub feeds: Vec<String>,
    pub save_dir: String,
    pub start_paused: bool,
}

pub const SETTINGS_VERSION: u32 = 1;

pub fn parse_hhmm(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}
