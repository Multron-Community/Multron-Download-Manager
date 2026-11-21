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
    pub accent_custom: String,
    pub accent_contrast: String,
    pub accent_text: String,
    pub button_color: String,
    pub glass_effect: bool,
    pub hover_glow: bool,
    pub click_ripple: bool,
    pub state_glow: bool,
    pub motion: String,
    pub effects_off: Vec<String>,
    pub heading_color: String,
    pub light_brightness: i32,
    pub dark_brightness: i32,
    pub background_image: String,
    pub background_opacity: u8,
    pub background_blur: u8,
    pub sidebar_shortcuts: String,
    pub rounded_corners: bool,
    pub delete_files_by_default: bool,
    pub clipboard_monitor: bool,
    pub transparency: bool,
    pub backdrop_material: String,
    pub transparency_level: u8,
    pub download_dir: String,
    pub organize_by_category: bool,
    pub category_folders: std::collections::BTreeMap<String, String>,
    pub custom_categories: Vec<CustomCategory>,
    pub category_order: Vec<String>,
    pub category_folder_language: String,
    pub category_language_asked: String,
    pub close_to_tray: bool,
    pub start_minimized: bool,
    pub launch_at_startup: bool,
    pub notifications: bool,
    pub download_notices: bool,
    pub scan_notices: bool,
    pub in_app_notices: bool,
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
    pub progress_window: bool,
    pub site_options: bool,
    pub fullscreen_video_button: bool,

    pub ytdlp_auto_update: bool,
    pub media_quality: String,
    pub media_available_qualities: bool,
    pub click_n_load: bool,
    pub click_n_load_known_only: bool,
    pub tor_enabled: bool,
    pub tor_proxy: String,
    pub tor_block_torrents: bool,
    pub privacy_no_referrer: bool,
    pub privacy_clear_finished_on_exit: bool,
    pub privacy_clear_cookies_on_exit: bool,
    pub privacy_forget_linkgrabber: bool,
    pub check_real_type: bool,
    pub ask_for_programs: bool,
    pub captcha_open_browser: bool,
    pub cloud_quarantine: bool,
    pub virustotal_key: String,
    pub virustotal_key_protected: String,
    pub scheduler_enabled: bool,
    pub sound_on_complete: bool,
    pub sound_on_fail: bool,
    pub scheduler_start: String,
    pub scheduler_stop_enabled: bool,
    pub scheduler_stop: String,
    pub scheduler_days: Vec<u8>,
    pub scheduler_scope: String,
    pub site_logins: Vec<SiteLogin>,
    pub site_limits: Vec<SiteLimit>,
    pub file_rules: Vec<FileRule>,
    pub history_enabled: bool,
    pub history_warn: bool,
    pub show_stats: bool,
    pub show_history: bool,
    pub show_tags: bool,
    pub site_rules: Vec<SiteRule>,
    pub auto_backup: bool,
    pub auto_backup_count: u32,
    pub auto_backup_unit: String,
    pub auto_backup_dir: String,
    pub auto_backup_keep: u32,
    pub sidebar_layout: Vec<SidebarGroup>,
    pub shortcuts_enabled: bool,
    pub privacy_gpc: bool,
    pub privacy_hide_names: bool,
    pub privacy_clear_history_on_exit: bool,
    pub check_signatures: bool,
    pub logins_auto: bool,
    pub virustotal_button: bool,
    pub cloud_history: bool,
    pub cloud_history_days: u32,
    pub cloud_block_threats: bool,
    pub cloud_rescan_on_open: bool,
    pub cloud_rescan_days: u32,
    pub captcha_sound: bool,
    pub captcha_notice: bool,
    pub captcha_auto_retry: bool,
    pub captcha_retry_secs: u64,
    pub captcha_timeout_secs: u64,
    pub auto_extract: bool,
    pub extract_subfolder: bool,
    pub extract_delete_archives: bool,
    pub extract_passwords: Vec<String>,
    pub extract_ask_password: bool,
    pub https_only: bool,
    pub block_insecure_programs: bool,
    pub block_private_links: bool,
    pub ask_before_opening_files: bool,
    pub skip_torrent_programs: bool,
    pub warn_disguised_names: bool,
    pub media_container: String,
    pub cookies_from_browser: String,
    pub embed_metadata: bool,
    pub embed_thumbnail: bool,

    pub torrent_port: u16,
    pub torrent_dht: bool,
    pub torrent_upnp: bool,
    pub torrent_utp: bool,
    pub torrent_peer_limit: u32,
    pub torrent_public_trackers: bool,
    pub torrent_download_limit: u64,
    pub torrent_upload_limit: u64,
    pub seed_after_complete: bool,
    pub torrent_dir: String,
    pub torrent_start_paused: bool,
    pub torrent_skip_enabled: bool,
    pub torrent_skip_patterns: String,
    pub torrent_delete_watched: bool,
    pub prevent_sleep: bool,
    pub open_folder_when_done: bool,
    pub auto_retry_minutes: u32,
    pub auto_retry_max: u32,
    pub auto_clear_days: u32,
    pub min_free_gb: u32,
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
    pub search_sites: Vec<SearchSite>,
    pub search_disabled: Vec<String>,
    pub on_complete_command: String,
    pub when_done: String,
    pub mark_downloads: bool,
    pub cloud_scan: bool,
    pub cloud_scan_button: bool,
    pub cloud_scan_all_button: bool,
    pub ask_before_starting_programs: bool,
    pub cloud_scan_accepted: bool,
    pub cloud_scan_limit_mb: u32,
    pub cloud_scan_manual_limit_mb: u32,
    pub cloud_compress_uploads: bool,
    pub cloud_connections: u32,
    pub cloud_fingerprint_cache: bool,
    pub cloud_scan_extensions: Vec<String>,
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
            accent_custom: "#2f8cf0".into(),
            accent_contrast: "auto".into(),
            accent_text: String::new(),
            button_color: String::new(),
            glass_effect: true,
            hover_glow: true,
            click_ripple: true,
            state_glow: false,
            motion: "on".into(),
            effects_off: QUIET_EFFECTS.iter().map(|e| e.to_string()).collect(),
            heading_color: "accent".into(),
            light_brightness: 0,
            dark_brightness: 0,
            background_image: String::new(),
            background_opacity: 72,
            background_blur: 0,
            sidebar_shortcuts: "list".into(),
            rounded_corners: true,
            delete_files_by_default: false,
            clipboard_monitor: false,
            transparency: true,
            backdrop_material: "auto".into(),
            transparency_level: 1,
            download_dir: String::new(),
            organize_by_category: false,
            category_folders: std::collections::BTreeMap::new(),
            custom_categories: Vec::new(),
            category_order: Vec::new(),
            category_folder_language: String::new(),
            category_language_asked: String::new(),
            close_to_tray: true,
            start_minimized: false,
            launch_at_startup: false,
            notifications: true,
            download_notices: true,
            scan_notices: true,
            in_app_notices: false,
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
            confirm_browser_downloads: true,
            progress_window: true,
            site_options: true,
            fullscreen_video_button: true,

            ytdlp_auto_update: true,
            media_quality: "best".into(),
            media_available_qualities: true,
            click_n_load: true,
            click_n_load_known_only: false,
            tor_enabled: false,
            tor_proxy: crate::tor::DEFAULT_PROXY.into(),
            tor_block_torrents: true,
            privacy_no_referrer: false,
            privacy_clear_finished_on_exit: false,
            privacy_clear_cookies_on_exit: false,
            privacy_forget_linkgrabber: false,
            check_real_type: true,
            ask_for_programs: true,
            captcha_open_browser: true,
            cloud_quarantine: true,
            virustotal_key: String::new(),
            virustotal_key_protected: String::new(),
            scheduler_enabled: false,
            sound_on_complete: true,
            sound_on_fail: false,
            scheduler_start: "02:00".into(),
            scheduler_stop_enabled: false,
            scheduler_stop: "08:00".into(),
            scheduler_days: vec![0, 1, 2, 3, 4, 5, 6],
            scheduler_scope: "later".into(),
            site_logins: Vec::new(),
            site_limits: Vec::new(),
            file_rules: Vec::new(),
            history_enabled: true,
            history_warn: true,
            show_stats: true,
            show_history: true,
            show_tags: true,
            site_rules: Vec::new(),
            auto_backup: false,
            auto_backup_count: 1,
            auto_backup_unit: "month".into(),
            auto_backup_dir: String::new(),
            auto_backup_keep: 6,
            sidebar_layout: Vec::new(),
            shortcuts_enabled: true,
            privacy_gpc: false,
            privacy_hide_names: false,
            privacy_clear_history_on_exit: false,
            check_signatures: true,
            logins_auto: true,
            virustotal_button: true,
            cloud_history: true,
            cloud_history_days: 30,
            cloud_block_threats: true,
            cloud_rescan_on_open: true,
            cloud_rescan_days: 7,
            captcha_sound: true,
            captcha_notice: true,
            captcha_auto_retry: true,
            captcha_retry_secs: 30,
            captcha_timeout_secs: 300,
            auto_extract: true,
            extract_subfolder: true,
            extract_delete_archives: false,
            extract_passwords: Vec::new(),
            extract_ask_password: true,
            https_only: false,
            block_insecure_programs: false,
            block_private_links: false,
            ask_before_opening_files: false,
            skip_torrent_programs: true,
            warn_disguised_names: true,
            media_container: "mp4-h264".into(),
            cookies_from_browser: String::new(),
            embed_metadata: true,
            embed_thumbnail: false,

            torrent_port: 22155,
            torrent_dht: true,
            torrent_upnp: true,
            torrent_utp: true,
            torrent_peer_limit: DEFAULT_PEER_LIMIT,
            torrent_public_trackers: true,
            torrent_download_limit: 0,
            torrent_upload_limit: 0,
            torrent_dir: String::new(),
            torrent_start_paused: false,
            torrent_skip_enabled: false,
            torrent_skip_patterns: "sample, .nfo, .txt, .url, .lnk, .website, .jpg".into(),
            torrent_delete_watched: false,
            prevent_sleep: true,
            open_folder_when_done: false,
            auto_retry_minutes: 0,
            auto_retry_max: 3,
            auto_clear_days: 0,
            min_free_gb: 1,
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
            search_sites: Vec::new(),
            search_disabled: Vec::new(),
            on_complete_command: String::new(),
            when_done: "nothing".into(),
            mark_downloads: true,
            cloud_scan: true,
            cloud_scan_button: true,
            cloud_scan_all_button: true,
            ask_before_starting_programs: true,
            cloud_scan_accepted: false,
            cloud_scan_limit_mb: crate::cloudscan::DEFAULT_LIMIT_MB,
            cloud_scan_manual_limit_mb: crate::cloudscan::MAX_LIMIT_MB,
            cloud_compress_uploads: true,
            cloud_connections: crate::cloudscan::DEFAULT_CONNECTIONS,
            cloud_fingerprint_cache: true,
            cloud_scan_extensions: DEFAULT_CLOUD_EXTENSIONS
                .iter()
                .map(|e| e.to_string())
                .collect(),
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
        let mut s = crate::util::read_json_or_keep::<Settings>(path).unwrap_or_default();
        for l in &mut s.site_logins {
            if l.password.starts_with(crate::secret::PREFIX) {
                l.password = crate::secret::unprotect(&l.password).unwrap_or_default();
            }
        }
        if s.virustotal_key.is_empty() && !s.virustotal_key_protected.is_empty() {
            s.virustotal_key =
                crate::secret::unprotect(&s.virustotal_key_protected).unwrap_or_default();
        }
        s.virustotal_key_protected.clear();
        s.normalized()
    }

    fn for_disk(&self) -> Self {
        let mut s = self.clone();
        for l in &mut s.site_logins {
            if !l.password.is_empty() {
                if let Some(sealed) = crate::secret::protect(&l.password) {
                    l.password = sealed;
                }
            }
        }
        s.virustotal_key_protected.clear();
        if !s.virustotal_key.is_empty() {
            if let Some(sealed) = crate::secret::protect(&s.virustotal_key) {
                s.virustotal_key_protected = sealed;
                s.virustotal_key.clear();
            }
        }
        s
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&self.for_disk())?)?;
        crate::secret::restrict_file(&tmp);
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn normalized(mut self) -> Self {
        if !["auto", "exact"].contains(&self.accent_contrast.as_str()) {
            self.accent_contrast = "auto".into();
        }
        let button = self.button_color.trim().to_ascii_lowercase();
        self.button_color = if button.len() == 7
            && button.starts_with('#')
            && button[1..].chars().all(|c| c.is_ascii_hexdigit())
        {
            button
        } else {
            String::new()
        };
        let text = self.accent_text.trim().to_ascii_lowercase();
        self.accent_text = if text.len() == 7
            && text.starts_with('#')
            && text[1..].chars().all(|c| c.is_ascii_hexdigit())
        {
            text
        } else {
            String::new()
        };
        self.effects_off.retain(|e| {
            [
                "aurora",
                "spotlight",
                "cards",
                "progress",
                "live",
                "accents",
                "transitions",
                "state",
                "fill",
            ]
            .contains(&e.as_str())
        });
        self.effects_off.sort();
        self.effects_off.dedup();
        if !["on", "system", "off"].contains(&self.motion.as_str()) {
            self.motion = "on".into();
        }
        self.light_brightness = self.light_brightness.clamp(-30, 30);
        self.dark_brightness = self.dark_brightness.clamp(-30, 30);
        let custom = self.heading_color.len() == 7
            && self.heading_color.starts_with('#')
            && self.heading_color[1..]
                .chars()
                .all(|c| c.is_ascii_hexdigit());
        if !custom && !["accent", "gray", "white"].contains(&self.heading_color.as_str()) {
            self.heading_color = "accent".into();
        }
        self.site_rules.truncate(200);
        for r in &mut self.site_rules {
            r.name = r.name.trim().chars().take(80).collect();
            r.host = clean_host(&r.host);
            r.pattern = r.pattern.chars().take(1000).collect();
        }
        self.auto_backup_count = self.auto_backup_count.clamp(1, 120);
        self.auto_backup_keep = self.auto_backup_keep.clamp(1, 100);
        if !matches!(
            self.auto_backup_unit.as_str(),
            "day" | "week" | "month" | "year"
        ) {
            self.auto_backup_unit = "month".into();
        }
        self.sidebar_layout.truncate(40);
        for g in &mut self.sidebar_layout {
            g.id = g.id.chars().take(64).collect();
            g.title = g.title.trim().chars().take(40).collect();
            g.items.truncate(40);
        }
        if self.settings_version < 1 {
            self.confirm_browser_downloads = false;
        }
        if self.settings_version < 2 {
            if self.torrent_peer_limit == 200 {
                self.torrent_peer_limit = DEFAULT_PEER_LIMIT;
            }
            self.notifications = true;
        }
        if self.settings_version < 3 {
            self.ask_before_starting_programs = true;
        }
        if self.settings_version < 4 {
            self.cloud_scan = true;
        }
        if self.settings_version < 5 && self.media_container == "mp4" {
            self.media_container = "mp4-h264".into();
        }
        if self.settings_version < 6 && self.category_folder_language.is_empty() {
            self.category_folder_language = "en".into();
        }
        if self.settings_version < 7 {
            self.confirm_browser_downloads = true;
            self.progress_window = true;
        }
        if self.settings_version < 8 {
            for effect in QUIET_EFFECTS {
                if !self.effects_off.iter().any(|e| e == effect) {
                    self.effects_off.push(effect.to_string());
                }
            }
            self.effects_off.sort();
        }
        self.category_folders.retain(|_, v| !v.trim().is_empty());
        for c in &mut self.custom_categories {
            if c.icon.len() > 24 || !c.icon.chars().all(|ch| ch.is_ascii_alphanumeric()) {
                c.icon.clear();
            }
            c.name = c.name.trim().to_string();
            c.folder = c.folder.trim().to_string();
            c.extensions = c
                .extensions
                .iter()
                .map(|e| e.trim().trim_start_matches('.').to_ascii_lowercase())
                .filter(|e| !e.is_empty())
                .collect();
        }
        self.custom_categories.retain(|c| !c.name.is_empty());
        let mut seen_order = std::collections::HashSet::new();
        self.category_order
            .retain(|id| !id.trim().is_empty() && id.len() <= 200 && seen_order.insert(id.clone()));
        let mut seen = std::collections::HashSet::new();
        self.extract_passwords = self
            .extract_passwords
            .iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty() && seen.insert(p.clone()))
            .take(200)
            .collect();
        self.captcha_retry_secs = self.captcha_retry_secs.clamp(10, 600);
        self.cloud_rescan_days = self.cloud_rescan_days.clamp(1, 365);
        self.auto_retry_minutes = self.auto_retry_minutes.min(1440);
        self.auto_retry_max = self.auto_retry_max.clamp(1, 50);
        self.auto_clear_days = self.auto_clear_days.min(3650);
        self.min_free_gb = self.min_free_gb.min(10_000);
        self.torrent_dir = self.torrent_dir.trim().to_string();
        self.background_opacity = self.background_opacity.clamp(20, 98);
        self.background_blur = self.background_blur.min(30);
        self.virustotal_key = self.virustotal_key.trim().to_string();
        self.scheduler_days.retain(|d| *d < 7);
        self.scheduler_days.sort_unstable();
        self.scheduler_days.dedup();
        for (t, fallback) in [
            (&mut self.scheduler_start, "02:00"),
            (&mut self.scheduler_stop, "08:00"),
        ] {
            if parse_hhmm(t).is_none() {
                *t = fallback.into();
            }
        }
        if self.scheduler_scope != "all" {
            self.scheduler_scope = "later".into();
        }
        let mut hosts = std::collections::HashSet::new();
        for l in &mut self.site_logins {
            l.host = clean_host(&l.host);
            l.user = l.user.trim().to_string();
            if l.kind != "bearer" {
                l.kind = "basic".into();
            }
        }
        self.site_logins
            .retain(|l| !l.host.is_empty() && hosts.insert(l.host.clone()));
        for l in &mut self.site_limits {
            l.host = clean_host(&l.host);
            l.max_connections = l.max_connections.min(64);
        }
        self.site_limits.retain(|l| !l.host.is_empty());
        for r in &mut self.file_rules {
            if !matches!(r.field.as_str(), "host" | "ext" | "name") {
                r.field = "ext".into();
            }
            if r.action != "rename" {
                r.action = "move".into();
            }
        }
        self.file_rules.retain(|r| !r.pattern.trim().is_empty());
        if !crate::tor::valid_proxy(&self.tor_proxy) {
            self.tor_proxy = crate::tor::DEFAULT_PROXY.into();
        }
        self.captcha_timeout_secs = self.captcha_timeout_secs.clamp(30, 3600);
        if self.sidebar_shortcuts != "tiles" {
            self.sidebar_shortcuts = "list".into();
        }
        self.settings_version = SETTINGS_VERSION;
        self.auto_update = true;
        self.check_updates_on_startup = true;
        self.ytdlp_auto_update = true;
        self.threads = self.threads.clamp(1, MAX_THREADS);
        self.cloud_scan_limit_mb = self
            .cloud_scan_limit_mb
            .clamp(1, crate::cloudscan::MAX_LIMIT_MB);
        self.cloud_connections = self
            .cloud_connections
            .clamp(1, crate::cloudscan::MAX_CONNECTIONS as u32);
        self.cloud_scan_manual_limit_mb = self
            .cloud_scan_manual_limit_mb
            .clamp(1, crate::cloudscan::MAX_LIMIT_MB);
        let mut extensions: Vec<String> = Vec::new();
        for ext in &self.cloud_scan_extensions {
            let ext = ext.trim().trim_start_matches('.').to_lowercase();
            let valid = ext == ALL_FILES
                || (!ext.is_empty()
                    && ext.len() <= 16
                    && ext.chars().all(|c| c.is_ascii_alphanumeric()));
            if valid && !extensions.contains(&ext) {
                extensions.push(ext);
            }
        }
        self.cloud_scan_extensions = extensions;
        self.max_concurrent = self.max_concurrent.clamp(1, 32);
        self.retries = self.retries.clamp(0, 50);
        if self.language != "en"
            && self.language != "auto"
            && !crate::i18n::languages().contains(&self.language)
        {
            self.language = "en".into();
        }
        if !["system", "dark", "light"].contains(&self.theme.as_str()) {
            self.theme = "system".into();
        }
        const ACCENTS: [&str; 10] = [
            "blue", "cyan", "green", "orange", "purple", "red", "pink", "teal", "gray", "custom",
        ];
        let hex = self.accent_custom.trim().to_ascii_lowercase();
        self.accent_custom = if hex.len() == 7
            && hex.starts_with('#')
            && hex[1..].chars().all(|c| c.is_ascii_hexdigit())
        {
            hex
        } else {
            "#2f8cf0".into()
        };
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
        if !CONTAINERS.contains(&self.media_container.as_str()) {
            self.media_container = "mp4-h264".into();
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
        let mut seen_sites = std::collections::HashSet::new();
        self.search_sites = std::mem::take(&mut self.search_sites)
            .into_iter()
            .filter_map(|mut s| {
                s.url = s.url.trim().to_string();
                s.name = s.name.trim().to_string();
                if !crate::search::valid_template(&s.url) || !seen_sites.insert(s.url.clone()) {
                    return None;
                }
                if s.name.is_empty() {
                    s.name = url::Url::parse(&s.url.replace("{query}", "x"))
                        .ok()
                        .and_then(|u| u.host_str().map(str::to_string))
                        .unwrap_or_else(|| s.url.clone());
                }
                Some(s)
            })
            .collect();
        self.search_disabled.sort();
        self.search_disabled.dedup();
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
pub struct SearchSite {
    pub name: String,
    pub url: String,
    pub enabled: bool,
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

pub const SETTINGS_VERSION: u32 = 8;

const QUIET_EFFECTS: [&str; 7] = [
    "accents",
    "aurora",
    "fill",
    "live",
    "progress",
    "spotlight",
    "state",
];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SidebarGroup {
    pub id: String,
    pub title: String,
    pub items: Vec<String>,
    pub collapsed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SiteLimit {
    pub host: String,
    pub speed_kb: u64,
    pub max_connections: u32,
    pub enabled: bool,
}

impl Default for SiteLimit {
    fn default() -> Self {
        SiteLimit {
            host: String::new(),
            speed_kb: 0,
            max_connections: 0,
            enabled: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct FileRule {
    pub enabled: bool,
    pub field: String,
    pub pattern: String,
    pub action: String,
    pub target: String,
}

impl Default for FileRule {
    fn default() -> Self {
        FileRule {
            enabled: true,
            field: "ext".into(),
            pattern: String::new(),
            action: "move".into(),
            target: String::new(),
        }
    }
}

impl Settings {
    pub fn site_limit(&self, url: &str) -> Option<&SiteLimit> {
        let host = url::Url::parse(url).ok()?.host_str()?.to_string();
        let host = clean_host(&host);
        self.site_limits
            .iter()
            .find(|l| l.enabled && (host == l.host || host.ends_with(&format!(".{}", l.host))))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SiteLogin {
    pub host: String,
    pub user: String,
    pub password: String,
    pub kind: String,
    pub subdomains: bool,
    pub enabled: bool,
}

impl Default for SiteLogin {
    fn default() -> Self {
        SiteLogin {
            host: String::new(),
            user: String::new(),
            password: String::new(),
            kind: "basic".into(),
            subdomains: true,
            enabled: true,
        }
    }
}

pub fn clean_host(input: &str) -> String {
    let t = input.trim().to_ascii_lowercase();
    let t = t.split_once("://").map(|(_, r)| r).unwrap_or(&t);
    let t = t.split(['/', '?', '#']).next().unwrap_or_default();
    let t = t.rsplit_once('@').map(|(_, h)| h).unwrap_or(t);
    t.trim_start_matches("www.")
        .trim_end_matches('.')
        .to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SiteRule {
    pub enabled: bool,
    pub name: String,
    pub host: String,
    pub pattern: String,
    pub only_rule: bool,
    pub follow: bool,
    pub on_add: bool,
}

impl Default for SiteRule {
    fn default() -> Self {
        SiteRule {
            enabled: true,
            name: String::new(),
            host: String::new(),
            pattern: String::new(),
            only_rule: true,
            follow: false,
            on_add: false,
        }
    }
}

impl SiteRule {
    pub fn matches(&self, url: &str) -> bool {
        let host = clean_host(url);
        let want = self.host.trim_start_matches("*.");
        self.enabled
            && !want.is_empty()
            && !self.pattern.trim().is_empty()
            && (host == want || host.ends_with(&format!(".{want}")))
    }
}

impl Settings {
    pub fn site_rule(&self, url: &str) -> Option<&SiteRule> {
        self.site_rules.iter().find(|r| r.matches(url))
    }
}

impl SiteLogin {
    pub fn matches(&self, host: &str) -> bool {
        let host = clean_host(host);
        self.enabled
            && !self.host.is_empty()
            && (host == self.host
                || (self.subdomains && host.ends_with(&format!(".{}", self.host))))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomCategory {
    pub name: String,
    pub extensions: Vec<String>,
    pub folder: String,
    pub icon: String,
}
pub const DEFAULT_PEER_LIMIT: u32 = 500;
pub const CONTAINERS: &[&str] = &["mp4", "mp4-h264", "mkv", "webm", "mov", "avi", "flv"];
pub const ALL_FILES: &str = "*";
#[cfg(windows)]
pub const DEFAULT_CLOUD_EXTENSIONS: &[&str] = &[
    "exe", "msi", "msix", "appx", "dll", "scr", "com", "bat", "cmd", "ps1", "vbs", "js", "jar",
    "lnk", "hta", "reg",
];
#[cfg(target_os = "macos")]
pub const DEFAULT_CLOUD_EXTENSIONS: &[&str] = &[
    "dmg", "pkg", "mpkg", "command", "tool", "sh", "zsh", "py", "jar", "scpt", "dylib",
];
#[cfg(all(unix, not(target_os = "macos")))]
pub const DEFAULT_CLOUD_EXTENSIONS: &[&str] = &[
    "appimage",
    "deb",
    "rpm",
    "run",
    "bin",
    "sh",
    "py",
    "pl",
    "jar",
    "flatpakref",
    "snap",
    "desktop",
];

pub fn parse_hhmm(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_site_logins_by_host() {
        let l = SiteLogin {
            host: clean_host("https://www.Files.example/dir?x=1"),
            ..Default::default()
        };
        assert_eq!(l.host, "files.example");
        assert!(l.matches("files.example"));
        assert!(l.matches("cdn.files.example"));
        assert!(!l.matches("evilfiles.example"));
        let strict = SiteLogin {
            subdomains: false,
            ..l.clone()
        };
        assert!(!strict.matches("cdn.files.example"));
        let off = SiteLogin {
            enabled: false,
            ..l
        };
        assert!(!off.matches("files.example"));
    }

    #[test]
    fn never_writes_the_virustotal_key_in_plain_text_on_windows() {
        let dir = std::env::temp_dir().join(format!("mdm-settings-{}", std::process::id()));
        let file = dir.join("settings.json");
        let s = Settings {
            virustotal_key: "secret-key-42".into(),
            ..Settings::default()
        };
        s.save(&file).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        if cfg!(windows) {
            assert!(!text.contains("secret-key-42"));
        }
        assert_eq!(Settings::load(&file).virustotal_key, "secret-key-42");
        assert!(Settings::load(&file).virustotal_key_protected.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn raises_the_old_default_peer_limit_once() {
        let old = Settings {
            settings_version: 1,
            torrent_peer_limit: 200,
            ..Default::default()
        };
        assert_eq!(old.normalized().torrent_peer_limit, DEFAULT_PEER_LIMIT);
        let chosen = Settings {
            settings_version: 1,
            torrent_peer_limit: 80,
            ..Default::default()
        };
        assert_eq!(chosen.normalized().torrent_peer_limit, 80);
        let current = Settings {
            settings_version: SETTINGS_VERSION,
            torrent_peer_limit: 200,
            ..Default::default()
        };
        assert_eq!(current.normalized().torrent_peer_limit, 200);
        let quiet = Settings {
            settings_version: 1,
            notifications: false,
            ..Default::default()
        };
        assert!(quiet.normalized().notifications);
        let chosen_quiet = Settings {
            settings_version: SETTINGS_VERSION,
            notifications: false,
            ..Default::default()
        };
        assert!(!chosen_quiet.normalized().notifications);
        assert!(Settings::default().notifications);
    }

    #[test]
    fn scans_downloads_in_the_cloud_by_default() {
        let fresh = Settings::default();
        assert!(fresh.cloud_scan && !fresh.cloud_scan_accepted);
        assert!(fresh.cloud_compress_uploads && fresh.cloud_fingerprint_cache);
        let old = Settings {
            settings_version: 3,
            cloud_scan: false,
            ..Default::default()
        };
        assert!(old.normalized().cloud_scan);
        let chosen = Settings {
            settings_version: SETTINGS_VERSION,
            cloud_scan: false,
            cloud_connections: 40,
            ..Default::default()
        };
        let chosen = chosen.normalized();
        assert!(!chosen.cloud_scan);
        assert_eq!(
            chosen.cloud_connections,
            crate::cloudscan::MAX_CONNECTIONS as u32
        );
    }

    #[test]
    fn accepts_a_custom_accent_color() {
        let custom = Settings {
            accent_color: "custom".into(),
            accent_custom: "#FF8800".into(),
            ..Default::default()
        }
        .normalized();
        assert_eq!(custom.accent_color, "custom");
        assert_eq!(custom.accent_custom, "#ff8800");
        let broken = Settings {
            accent_custom: "orange".into(),
            ..Default::default()
        }
        .normalized();
        assert_eq!(broken.accent_custom, "#2f8cf0");
        assert!(Settings::default().glass_effect);
        assert!(Settings::default().rounded_corners);
    }

    #[test]
    fn keeps_english_folder_names_for_existing_installs() {
        let old = Settings {
            settings_version: 5,
            category_folder_language: String::new(),
            ..Default::default()
        };
        assert_eq!(old.normalized().category_folder_language, "en");
        assert_eq!(
            Settings::default().normalized().category_folder_language,
            ""
        );
        let messy = Settings {
            custom_categories: vec![
                CustomCategory {
                    name: " E-books ".into(),
                    extensions: vec![".EPUB".into(), " mobi".into(), "".into()],
                    folder: " /books ".into(),
                    icon: "book; rm".into(),
                },
                CustomCategory {
                    name: "  ".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
        .normalized();
        assert_eq!(messy.custom_categories.len(), 1);
        assert_eq!(messy.custom_categories[0].icon, "");
        assert_eq!(messy.custom_categories[0].name, "E-books");
        assert_eq!(messy.custom_categories[0].extensions, vec!["epub", "mobi"]);
        assert_eq!(messy.custom_categories[0].folder, "/books");
    }

    #[test]
    fn saves_videos_that_play_everywhere_by_default() {
        assert_eq!(Settings::default().media_container, "mp4-h264");
        let old = Settings {
            settings_version: 4,
            media_container: "mp4".into(),
            ..Default::default()
        };
        assert_eq!(old.normalized().media_container, "mp4-h264");
        let chosen = Settings {
            settings_version: 4,
            media_container: "mkv".into(),
            ..Default::default()
        };
        assert_eq!(chosen.normalized().media_container, "mkv");
        let later = Settings {
            settings_version: SETTINGS_VERSION,
            media_container: "mp4".into(),
            ..Default::default()
        };
        assert_eq!(later.normalized().media_container, "mp4");
    }

    #[test]
    fn shows_the_fullscreen_video_button_by_default() {
        assert!(Settings::default().fullscreen_video_button);
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert!(old.fullscreen_video_button);
    }

    #[test]
    #[ignore = "reads a copied data folder"]
    fn live_loads_an_existing_data_folder() {
        let dir =
            std::path::PathBuf::from(std::env::var("MULTRON_TEST_DIR").expect("MULTRON_TEST_DIR"));
        let raw = std::fs::read(dir.join("settings.json")).unwrap();
        let parsed: Settings = serde_json::from_slice(&raw).expect("settings.json is readable");
        let before = parsed.settings_version;
        let s = parsed.normalized();
        assert_eq!(s.settings_version, SETTINGS_VERSION);
        let again: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(again.normalized().settings_version, SETTINGS_VERSION);
        let list: Vec<crate::model::DownloadRecord> =
            serde_json::from_slice(&std::fs::read(dir.join("downloads.json")).unwrap())
                .expect("downloads.json is readable");
        let back: Vec<crate::model::DownloadRecord> =
            serde_json::from_str(&serde_json::to_string(&list).unwrap()).unwrap();
        assert_eq!(back.len(), list.len());
        println!(
            "settings v{before} -> v{}, language {}, theme {}, download dir set: {}, {} downloads read and written back",
            s.settings_version,
            s.language,
            s.theme,
            !s.download_dir.is_empty(),
            list.len()
        );
    }

    #[test]
    fn brightness_defaults_to_zero_and_stays_in_range() {
        let s = Settings::default();
        assert_eq!((s.light_brightness, s.dark_brightness), (0, 0));
        let wild = Settings {
            light_brightness: -90,
            dark_brightness: 75,
            ..Settings::default()
        }
        .normalized();
        assert_eq!((wild.light_brightness, wild.dark_brightness), (-30, 30));
    }

    #[test]
    fn looping_effects_are_off_by_default() {
        let quiet: Vec<String> = QUIET_EFFECTS.iter().map(|e| e.to_string()).collect();
        assert_eq!(Settings::default().effects_off, quiet);
        let old = Settings {
            settings_version: 7,
            effects_off: vec!["cards".into()],
            ..Settings::default()
        }
        .normalized();
        assert!(old.effects_off.contains(&"cards".to_string()));
        assert!(quiet.iter().all(|e| old.effects_off.contains(e)));
        let chosen = Settings {
            effects_off: Vec::new(),
            ..Settings::default()
        }
        .normalized();
        assert!(chosen.effects_off.is_empty());
    }

    #[test]
    fn browser_windows_and_security_defaults() {
        let s = Settings::default();
        assert!(s.confirm_browser_downloads && s.progress_window && s.site_options);
        assert!(
            !s.block_insecure_programs && !s.block_private_links && !s.ask_before_opening_files
        );
        let old = Settings {
            settings_version: 6,
            confirm_browser_downloads: false,
            progress_window: false,
            ..Settings::default()
        }
        .normalized();
        assert!(old.confirm_browser_downloads && old.progress_window);
    }

    #[test]
    fn asks_before_starting_programs_by_default() {
        assert!(Settings::default().ask_before_starting_programs);
        let old = Settings {
            settings_version: 2,
            ask_before_starting_programs: false,
            ..Default::default()
        };
        assert!(old.normalized().ask_before_starting_programs);
        let chosen = Settings {
            settings_version: SETTINGS_VERSION,
            ask_before_starting_programs: false,
            ..Default::default()
        };
        assert!(!chosen.normalized().ask_before_starting_programs);
    }
}
