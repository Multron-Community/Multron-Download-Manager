use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use bytes::Bytes;
use librqbit::api::{Api, TorrentIdOrHash};
use librqbit::limits::LimitsConfig;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ConnectionOptions, DhtSessionConfig,
    ListenerMode, ListenerOptions, ManagedTorrent, PeerConnectionOptions, Session, SessionOptions,
    SessionPersistenceConfig, TorrentStats,
};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::Value;

use crate::model::TorrentFile;
use crate::trackers::{self, ScrapeStats};
use crate::util::now_ms;

const SCRAPE_INTERVAL_SECS: u64 = 300;
const PEER_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const PUBLIC_TRACKERS_FILE: &str = "public-trackers.txt";
const PUBLIC_TRACKERS_MAX_AGE: Duration = Duration::from_secs(24 * 3600);

#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub default_dir: PathBuf,
    pub state_dir: PathBuf,
    pub port: u16,
    pub dht: bool,
    pub upnp: bool,
    pub utp: bool,
    pub lsd: bool,
    pub ipv4_only: bool,
    pub peer_limit: u32,
    pub download_kbps: u64,
    pub upload_kbps: u64,
    pub trackers: Vec<String>,
    pub proxy: String,
    pub bind_interface: String,
    pub blocklist: Option<PathBuf>,
    pub allowlist: Option<PathBuf>,
    pub disable_trackers: bool,
    pub hide_client: bool,
    pub scrape: bool,
    pub public_trackers: bool,
}

pub enum TorrentSource {
    Url(String),
    File(PathBuf),
}

impl TorrentSource {
    fn to_add(&self) -> Result<AddTorrent<'static>> {
        Ok(match self {
            TorrentSource::Url(u) => AddTorrent::from_url(u.clone()),
            TorrentSource::File(p) => AddTorrent::from_bytes(
                std::fs::read(p).with_context(|| format!("could not read {}", p.display()))?,
            ),
        })
    }

    fn magnet(&self) -> Option<&str> {
        match self {
            TorrentSource::Url(u) if u.starts_with("magnet:") => Some(u),
            _ => None,
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TorrentPreview {
    pub info_hash: String,
    pub name: String,
    pub total: u64,
    pub files: Vec<TorrentFile>,
    pub trackers: Vec<String>,
    pub private: bool,
    pub piece_length: Option<u64>,
    pub comment: Option<String>,
}

pub struct AddedTorrent {
    pub info_hash: String,
    pub name: String,
    pub files: Vec<TorrentFile>,
    pub total: u64,
    pub output_folder: PathBuf,
    pub torrent_bytes: Option<Bytes>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SourceRow {
    pub kind: String,
    pub details: String,
    pub status: String,
    pub peers: Option<u32>,
    pub seeds: Option<u32>,
    pub leechers: Option<u32>,
    pub completed: Option<u32>,
    pub next: Option<u64>,
    pub interval: Option<u64>,
    #[serde(skip)]
    next_at: i64,
    #[serde(skip)]
    in_flight: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PeerRow {
    pub addr: String,
    pub client: Option<String>,
    pub state: String,
    pub connection: Option<String>,
    pub downloaded: u64,
    pub uploaded: u64,
    pub errors: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    pub index: usize,
    pub name: String,
    pub length: u64,
    pub included: bool,
    pub progress: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PiecesInfo {
    pub count: u32,
    pub have: String,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct InfoTab {
    pub info_hash: String,
    pub name: String,
    pub total: u64,
    pub piece_length: Option<u64>,
    pub pieces: u32,
    pub private: bool,
    pub comment: Option<String>,
    pub created_by: Option<String>,
    pub creation_date: Option<i64>,
    pub save_path: String,
    pub magnet: String,
    pub trackers: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TorrentDetails {
    pub info: InfoTab,
    pub files: Vec<FileRow>,
    pub peers: Vec<PeerRow>,
    pub pieces: Option<PiecesInfo>,
    pub sources: Vec<SourceRow>,
    pub state: String,
    pub error: Option<String>,
    pub download_speed: u64,
    pub upload_speed: u64,
    pub uploaded: u64,
    pub progress: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub running: bool,
    pub download_speed: u64,
    pub upload_speed: u64,
    pub peers: u64,
    pub dht_enabled: bool,
    pub dht_nodes: Option<usize>,
    pub listen_port: Option<u16>,
    pub warning: Option<String>,
}

pub struct Snapshot {
    pub state: String,
    pub error: Option<String>,
    pub progress: u64,
    pub total: u64,
    pub uploaded: u64,
    pub finished: bool,
    pub download_speed: u64,
    pub upload_speed: u64,
    pub peers: u32,
}

pub trait ReadSeek: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin + Send> ReadSeek for T {}

pub struct CreatedTorrent {
    pub bytes: Bytes,
    pub info_hash: String,
    pub name: String,
}

pub async fn create_torrent_file(
    path: &Path,
    trackers: Vec<String>,
    piece_kib: Option<u32>,
) -> Result<CreatedTorrent> {
    let spawner = librqbit::spawn_utils::BlockingSpawner::new(4);
    let result = librqbit::create_torrent(
        path,
        librqbit::CreateTorrentOptions {
            name: None,
            trackers,
            piece_length: piece_kib.filter(|k| *k > 0).map(|k| k * 1024),
        },
        &spawner,
    )
    .await?;
    let bytes = result.as_bytes()?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| result.info_hash().as_string());
    Ok(CreatedTorrent {
        bytes,
        info_hash: result.info_hash().as_string(),
        name,
    })
}

pub struct TorrentEngine {
    pub session: Arc<Session>,
    api: Api,
    pub config: EngineConfig,
    pub warning: Option<String>,
    client: reqwest::Client,
    previews: Mutex<HashMap<String, Bytes>>,
    sources: Mutex<HashMap<String, HashMap<String, SourceRow>>>,
    public: Arc<Mutex<Vec<String>>>,
}

fn file_url(p: &Path) -> Option<String> {
    url::Url::from_file_path(p).ok().map(|u| u.to_string())
}

fn kbps(v: u64) -> Option<NonZeroU32> {
    NonZeroU32::new(v.saturating_mul(1024).min(u32::MAX as u64) as u32)
}

async fn new_session(cfg: &EngineConfig, port: u16) -> Result<Arc<Session>> {
    let listen_ip: SocketAddr = if cfg.ipv4_only {
        (Ipv4Addr::UNSPECIFIED, port).into()
    } else {
        (Ipv6Addr::UNSPECIFIED, port).into()
    };
    let trackers: HashSet<url::Url> = cfg
        .trackers
        .iter()
        .filter_map(|t| url::Url::parse(t.trim()).ok())
        .collect();
    let opts = SessionOptions {
        dht: cfg.dht.then(|| DhtSessionConfig {
            persistence: Some(librqbit::dht::DhtPersistenceConfig {
                config_filename: Some(cfg.state_dir.join("dht.json")),
                ..Default::default()
            }),
            ..Default::default()
        }),
        fastresume: true,
        persistence: Some(SessionPersistenceConfig::Json {
            folder: Some(cfg.state_dir.clone()),
        }),
        listen: Some(ListenerOptions {
            mode: if cfg.utp {
                ListenerMode::TcpAndUtp
            } else {
                ListenerMode::TcpOnly
            },
            listen_addr: listen_ip,
            enable_upnp_port_forwarding: cfg.upnp,
            ipv4_only: cfg.ipv4_only,
            ..Default::default()
        }),
        connect: Some(ConnectionOptions {
            proxy_url: (!cfg.proxy.trim().is_empty()).then(|| cfg.proxy.trim().to_string()),
            peer_opts: Some(PeerConnectionOptions {
                connect_timeout: Some(PEER_CONNECT_TIMEOUT),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ratelimits: LimitsConfig {
            download_bps: kbps(cfg.download_kbps),
            upload_bps: kbps(cfg.upload_kbps),
        },
        blocklist_url: cfg.blocklist.as_deref().and_then(file_url),
        allowlist_url: cfg.allowlist.as_deref().and_then(file_url),
        trackers,
        peer_limit: Some(cfg.peer_limit.max(10) as usize),
        disable_local_service_discovery: !cfg.lsd,
        disable_trackers: cfg.disable_trackers,
        ipv4_only: cfg.ipv4_only,
        bind_device_name: (!cfg.bind_interface.trim().is_empty())
            .then(|| cfg.bind_interface.trim().to_string()),
        client_name_and_version: Some(if cfg.hide_client {
            "BitTorrent".to_string()
        } else {
            format!("Multron Download Manager {}", env!("CARGO_PKG_VERSION"))
        }),
        ..Default::default()
    };
    Session::new_with_opts(cfg.default_dir.clone(), opts).await
}

fn read_tracker_list(text: &str) -> Vec<String> {
    let mut v: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.contains("://"))
        .map(str::to_string)
        .collect();
    trackers::dedup(&mut v);
    v
}

fn load_public_trackers(state_dir: &Path, client: &reqwest::Client, into: Arc<Mutex<Vec<String>>>) {
    let file = state_dir.join(PUBLIC_TRACKERS_FILE);
    let fresh = std::fs::metadata(&file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < PUBLIC_TRACKERS_MAX_AGE);
    if let Ok(text) = std::fs::read_to_string(&file) {
        *into.lock() = read_tracker_list(&text);
    }
    if fresh {
        return;
    }
    let client = client.clone();
    tokio::spawn(async move {
        match trackers::fetch_public_trackers(&client).await {
            Ok(list) if !list.is_empty() => {
                let _ = std::fs::write(&file, list.join("\n"));
                *into.lock() = list;
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("could not refresh the public tracker list: {e:#}"),
        }
    });
}

impl TorrentEngine {
    pub async fn start(cfg: EngineConfig, client: reqwest::Client) -> Result<Self> {
        std::fs::create_dir_all(&cfg.state_dir)?;
        #[cfg(unix)]
        let _ = librqbit::try_increase_nofile_limit();
        let (session, warning) = match new_session(&cfg, cfg.port).await {
            Ok(s) => (s, None),
            Err(e) if cfg.port != 0 => {
                tracing::warn!("torrent port {} unavailable: {e:#}", cfg.port);
                let s = new_session(&cfg, 0).await?;
                (
                    s,
                    Some(format!(
                        "Port {} is in use by another program; a random port is used instead.",
                        cfg.port
                    )),
                )
            }
            Err(e) => return Err(e),
        };
        let api = Api::new(session.clone(), None);
        let public = Arc::new(Mutex::new(Vec::new()));
        if cfg.public_trackers && !cfg.disable_trackers {
            load_public_trackers(&cfg.state_dir, &client, public.clone());
        }
        Ok(Self {
            session,
            api,
            config: cfg,
            warning,
            client,
            previews: Mutex::new(HashMap::new()),
            sources: Mutex::new(HashMap::new()),
            public,
        })
    }

    pub async fn stop(&self) {
        self.session.stop().await;
    }

    fn public_for(&self, private: bool) -> Vec<String> {
        if private || !self.config.public_trackers || self.config.disable_trackers {
            return Vec::new();
        }
        self.public.lock().clone()
    }

    pub fn handle(&self, hash: &str) -> Option<Arc<ManagedTorrent>> {
        let id = TorrentIdOrHash::parse(hash).ok()?;
        self.session.get(id)
    }

    pub async fn inspect(&self, src: &TorrentSource) -> Result<TorrentPreview> {
        let add = src.to_add()?;
        let fut = self.session.add_torrent(
            add,
            Some(AddTorrentOptions {
                list_only: true,
                trackers: src
                    .magnet()
                    .map(|_| self.public_for(false))
                    .filter(|t| !t.is_empty()),
                ..Default::default()
            }),
        );
        let resp = tokio::time::timeout(Duration::from_secs(180), fut)
            .await
            .map_err(|_| anyhow!("timed out while fetching torrent metadata from peers"))??;
        match resp {
            AddTorrentResponse::ListOnly(r) => {
                let info_hash = r.info_hash.as_string();
                let files: Vec<TorrentFile> = r
                    .info
                    .iter_file_details()
                    .map(|d| TorrentFile {
                        name: d.filename.to_string(),
                        length: d.len,
                        included: true,
                    })
                    .collect();
                let total = files.iter().map(|f| f.length).sum();
                let name = r
                    .info
                    .name()
                    .map(|n| n.into_owned())
                    .unwrap_or_else(|| info_hash.clone());
                let meta = trackers::read_metainfo(&r.torrent_bytes);
                let mut tr = meta.trackers.clone();
                if let Some(m) = src.magnet() {
                    tr.extend(trackers::magnet_trackers(m));
                }
                trackers::dedup(&mut tr);
                self.previews
                    .lock()
                    .insert(info_hash.clone(), r.torrent_bytes.clone());
                Ok(TorrentPreview {
                    info_hash,
                    name,
                    total,
                    files,
                    trackers: tr,
                    private: meta.private,
                    piece_length: meta.piece_length,
                    comment: meta.comment,
                })
            }
            AddTorrentResponse::AlreadyManaged(..) => bail!("this torrent is already in the list"),
            AddTorrentResponse::Added(..) => bail!("unexpected answer from the torrent engine"),
        }
    }

    pub async fn add(
        &self,
        src: &TorrentSource,
        preview_hash: Option<&str>,
        dir: &Path,
        only_files: Option<Vec<usize>>,
        paused: bool,
        extra_trackers: Vec<String>,
    ) -> Result<AddedTorrent> {
        let known = preview_hash
            .map(str::to_ascii_lowercase)
            .or_else(|| src.magnet().and_then(trackers::magnet_info_hash))
            .filter(|h| self.previews.lock().contains_key(h));
        let hash = match known {
            Some(h) => h,
            None => self.inspect(src).await?.info_hash,
        };
        let bytes = self
            .previews
            .lock()
            .get(&hash)
            .cloned()
            .ok_or_else(|| anyhow!("torrent metadata is missing"))?;
        let file_count = Bytes::clone(&bytes);
        let multi_file = trackers::Bencode::parse(&file_count)
            .and_then(|b| b.get("info").and_then(|i| i.get("files")).map(|_| true))
            .unwrap_or(false);
        let name = trackers::Bencode::parse(&bytes)
            .and_then(|b| {
                b.get("info")
                    .and_then(|i| i.get("name"))
                    .and_then(|n| n.as_str())
            })
            .map(|n| crate::util::sanitize_filename(&n))
            .unwrap_or_else(|| hash.clone());
        let output = if multi_file {
            dir.join(&name)
        } else {
            dir.to_path_buf()
        };
        let mut extra_trackers = extra_trackers;
        extra_trackers.extend(self.public_for(trackers::read_metainfo(&bytes).private));
        trackers::dedup(&mut extra_trackers);
        let opts = AddTorrentOptions {
            paused,
            only_files,
            overwrite: true,
            output_folder: Some(output.to_string_lossy().into_owned()),
            trackers: (!extra_trackers.is_empty()).then_some(extra_trackers),
            ..Default::default()
        };
        let resp = self
            .session
            .add_torrent(AddTorrent::from_bytes(bytes.clone()), Some(opts))
            .await?;
        let handle = resp
            .into_handle()
            .ok_or_else(|| anyhow!("torrent was not added"))?;
        let files = self.files_of(&handle);
        let total = files.iter().map(|f| f.length).sum();
        Ok(AddedTorrent {
            info_hash: handle.info_hash().as_string(),
            name: handle.name().unwrap_or(name),
            files,
            total,
            output_folder: output,
            torrent_bytes: Some(bytes),
        })
    }

    pub async fn restore(
        &self,
        torrent_file: Option<&Path>,
        source: &str,
        dir: &Path,
        only_files: Option<Vec<usize>>,
        paused: bool,
    ) -> Result<String> {
        let (add, public) = match torrent_file.filter(|p| p.is_file()) {
            Some(p) => {
                let bytes = std::fs::read(p)?;
                let public = self.public_for(trackers::read_metainfo(&bytes).private);
                (AddTorrent::from_bytes(bytes), public)
            }
            None => (AddTorrent::from_url(source.to_string()), Vec::new()),
        };
        let resp = self
            .session
            .add_torrent(
                add,
                Some(AddTorrentOptions {
                    paused,
                    only_files,
                    overwrite: true,
                    output_folder: Some(dir.to_string_lossy().into_owned()),
                    trackers: (!public.is_empty()).then_some(public),
                    ..Default::default()
                }),
            )
            .await?;
        let handle = resp
            .into_handle()
            .ok_or_else(|| anyhow!("torrent was not restored"))?;
        Ok(handle.info_hash().as_string())
    }

    fn files_of(&self, handle: &ManagedTorrent) -> Vec<TorrentFile> {
        let only = handle.only_files();
        handle
            .metadata
            .load_full()
            .map(|m| {
                m.info
                    .iter_file_details()
                    .enumerate()
                    .map(|(i, d)| TorrentFile {
                        name: d.filename.to_string(),
                        length: d.len,
                        included: only.as_ref().map(|o| o.contains(&i)).unwrap_or(true),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn pause(&self, hash: &str) -> Result<()> {
        let h = self
            .handle(hash)
            .ok_or_else(|| anyhow!("torrent not found"))?;
        if !h.is_paused() {
            self.session.pause(&h).await?;
        }
        Ok(())
    }

    pub async fn resume(&self, hash: &str) -> Result<()> {
        let h = self
            .handle(hash)
            .ok_or_else(|| anyhow!("torrent not found"))?;
        if h.is_paused() {
            self.session.unpause(&h).await?;
        }
        Ok(())
    }

    pub async fn remove(&self, hash: &str, delete_files: bool) -> Result<()> {
        let id = TorrentIdOrHash::parse(hash)?;
        self.sources.lock().remove(hash);
        self.session.delete(id, delete_files).await
    }

    pub async fn set_only_files(&self, hash: &str, files: &[usize]) -> Result<()> {
        let h = self
            .handle(hash)
            .ok_or_else(|| anyhow!("torrent not found"))?;
        let set: HashSet<usize> = files.iter().copied().collect();
        if set.is_empty() {
            bail!("select at least one file");
        }
        self.session.update_only_files(&h, &set).await
    }

    pub async fn stream(&self, hash: &str, file: usize) -> Result<(Box<dyn ReadSeek>, u64)> {
        let id = TorrentIdOrHash::parse(hash)?;
        let stream = self.api.api_stream(id, file).await?;
        let len = stream.len();
        Ok((Box::new(stream), len))
    }

    pub fn apply_limits(&self, download_kbps: u64, upload_kbps: u64) {
        self.session
            .ratelimits
            .set_download_bps(kbps(download_kbps));
        self.session.ratelimits.set_upload_bps(kbps(upload_kbps));
    }

    pub fn files(&self, hash: &str) -> Vec<TorrentFile> {
        self.handle(hash)
            .map(|h| self.files_of(&h))
            .unwrap_or_default()
    }

    pub fn snapshot(&self, hash: &str) -> Option<Snapshot> {
        let h = self.handle(hash)?;
        let stats = h.stats();
        Some(snapshot_of(&stats))
    }

    pub fn status(&self) -> SessionStatus {
        let snap = serde_json::to_value(self.session.stats_snapshot()).unwrap_or(Value::Null);
        let speed = |k: &str| {
            snap.pointer(&format!("/{k}/mbps"))
                .and_then(Value::as_f64)
                .map(|m| (m * 1024.0 * 1024.0) as u64)
                .unwrap_or(0)
        };
        let dht = self.session.get_dht();
        SessionStatus {
            running: true,
            download_speed: speed("download_speed"),
            upload_speed: speed("upload_speed"),
            peers: snap
                .pointer("/peers/live")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            dht_enabled: dht.is_some(),
            dht_nodes: dht.map(|d| {
                let s = d.stats();
                s.routing_table_size + s.routing_table_size_v6
            }),
            listen_port: self.session.announce_port(),
            warning: self.warning.clone(),
        }
    }

    pub fn details(
        self: &Arc<Self>,
        hash: &str,
        extra_trackers: &[String],
    ) -> Result<TorrentDetails> {
        let h = self
            .handle(hash)
            .ok_or_else(|| anyhow!("torrent not found"))?;
        let id = TorrentIdOrHash::parse(hash)?;
        let stats = h.stats();
        let snap = snapshot_of(&stats);

        let meta = h.metadata.load_full();
        let tmeta = meta
            .as_ref()
            .map(|m| trackers::read_metainfo(&m.torrent_bytes))
            .unwrap_or_default();
        let mut tr = tmeta.trackers.clone();
        tr.extend(extra_trackers.iter().cloned());
        tr.extend(self.config.trackers.iter().cloned());
        trackers::dedup(&mut tr);

        let only = h.only_files();
        let files: Vec<FileRow> = meta
            .as_ref()
            .map(|m| {
                m.info
                    .iter_file_details()
                    .enumerate()
                    .map(|(i, d)| FileRow {
                        index: i,
                        name: d.filename.to_string(),
                        length: d.len,
                        included: only.as_ref().map(|o| o.contains(&i)).unwrap_or(true),
                        progress: stats.file_progress.get(i).copied().unwrap_or(0),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let peers = self
            .api
            .api_peer_stats(
                id,
                serde_json::from_value(serde_json::json!({ "state": "live" }))?,
            )
            .ok()
            .and_then(|p| serde_json::to_value(p).ok())
            .map(|v| parse_peers(&v))
            .unwrap_or_default();

        let pieces = self
            .api
            .api_dump_haves(id)
            .ok()
            .map(|(bf, count)| PiecesInfo {
                count,
                have: base64::engine::general_purpose::STANDARD.encode(bf.as_raw_slice()),
            });

        let name = h.name().unwrap_or_else(|| hash.to_string());
        let mut magnet = format!("magnet:?xt=urn:btih:{hash}&dn={}", urlencode(&name));
        for t in &tr {
            magnet.push_str("&tr=");
            magnet.push_str(&urlencode(t));
        }
        let info = InfoTab {
            info_hash: hash.to_string(),
            name,
            total: stats.total_bytes,
            piece_length: tmeta.piece_length,
            pieces: meta
                .as_ref()
                .map(|m| m.info.lengths().total_pieces())
                .unwrap_or(0),
            private: tmeta.private,
            comment: tmeta.comment.clone(),
            created_by: tmeta.created_by.clone(),
            creation_date: tmeta.creation_date,
            save_path: self
                .api
                .api_torrent_details(id)
                .map(|d| d.output_folder)
                .unwrap_or_default(),
            magnet,
            trackers: tr.clone(),
        };

        Ok(TorrentDetails {
            sources: self.sources(hash, &tr, tmeta.private, snap.peers),
            info,
            files,
            peers,
            pieces,
            state: snap.state,
            error: snap.error,
            download_speed: snap.download_speed,
            upload_speed: snap.upload_speed,
            uploaded: snap.uploaded,
            progress: snap.progress,
        })
    }

    fn sources(
        self: &Arc<Self>,
        hash: &str,
        tracker_urls: &[String],
        private: bool,
        peers: u32,
    ) -> Vec<SourceRow> {
        let now = now_ms();
        let mut to_scrape = Vec::new();
        let rows: Vec<SourceRow> = {
            let mut all = self.sources.lock();
            let map = all.entry(hash.to_string()).or_default();
            map.retain(|k, _| tracker_urls.contains(k));
            tracker_urls
                .iter()
                .map(|t| {
                    let row = map.entry(t.clone()).or_insert_with(|| SourceRow {
                        kind: "tracker".into(),
                        details: t.clone(),
                        status: if self.config.disable_trackers {
                            "Disabled".into()
                        } else if self.config.scrape {
                            "Scraping…".into()
                        } else {
                            "Scrape disabled".into()
                        },
                        interval: Some(SCRAPE_INTERVAL_SECS),
                        ..Default::default()
                    });
                    if self.config.scrape
                        && !self.config.disable_trackers
                        && !row.in_flight
                        && row.next_at <= now
                    {
                        row.in_flight = true;
                        to_scrape.push(t.clone());
                    }
                    let mut out = row.clone();
                    out.next = (row.next_at > now).then(|| ((row.next_at - now) / 1000) as u64);
                    out
                })
                .collect()
        };
        for t in to_scrape {
            let this = self.clone();
            let hash = hash.to_string();
            tokio::spawn(async move {
                let result = match hex_hash(&hash) {
                    Some(raw) => trackers::scrape(&this.client, &t, &raw).await,
                    None => Err(anyhow!("invalid info hash")),
                };
                this.store_scrape(&hash, &t, result);
            });
        }
        let mut rows = rows;
        let dht = self.session.get_dht();
        rows.push(SourceRow {
            kind: "dht".into(),
            details: "Mainline DHT".into(),
            status: if private {
                "Disabled (private torrent)".into()
            } else {
                match dht {
                    Some(d) => {
                        let s = d.stats();
                        format!(
                            "Online · {} nodes",
                            s.routing_table_size + s.routing_table_size_v6
                        )
                    }
                    None => "Disabled".into(),
                }
            },
            ..Default::default()
        });
        rows.push(SourceRow {
            kind: "lsd".into(),
            details: "Local Service Discovery".into(),
            status: if self.config.lsd && !private {
                "Enabled".into()
            } else {
                "Disabled".into()
            },
            ..Default::default()
        });
        rows.push(SourceRow {
            kind: "incoming".into(),
            details: match self.session.announce_port() {
                Some(p) => format!("Listening on port {p}"),
                None => "Not listening".into(),
            },
            status: if self.config.upnp {
                "UPnP enabled".into()
            } else {
                "UPnP disabled".into()
            },
            peers: Some(peers),
            ..Default::default()
        });
        rows
    }

    fn store_scrape(&self, hash: &str, tracker: &str, result: Result<ScrapeStats>) {
        let mut all = self.sources.lock();
        let Some(row) = all.get_mut(hash).and_then(|m| m.get_mut(tracker)) else {
            return;
        };
        row.in_flight = false;
        match result {
            Ok(s) => {
                row.status = "OK".into();
                row.seeds = Some(s.seeds);
                row.leechers = Some(s.leechers);
                row.completed = Some(s.completed);
                row.peers = Some(s.seeds + s.leechers);
                row.next_at = now_ms() + (SCRAPE_INTERVAL_SECS * 1000) as i64;
            }
            Err(e) => {
                row.status = format!("Error: {e}");
                row.next_at = now_ms() + (SCRAPE_INTERVAL_SECS * 2000) as i64;
            }
        }
    }
}

fn snapshot_of(stats: &TorrentStats) -> Snapshot {
    let v = serde_json::to_value(stats).unwrap_or(Value::Null);
    let speed = |k: &str| {
        v.pointer(&format!("/live/{k}/mbps"))
            .and_then(Value::as_f64)
            .map(|m| (m * 1024.0 * 1024.0) as u64)
            .unwrap_or(0)
    };
    Snapshot {
        state: v
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("initializing")
            .to_string(),
        error: stats.error.clone(),
        progress: stats.progress_bytes,
        total: stats.total_bytes,
        uploaded: stats.uploaded_bytes,
        finished: stats.finished,
        download_speed: speed("download_speed"),
        upload_speed: speed("upload_speed"),
        peers: v
            .pointer("/live/snapshot/peer_stats/live")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
    }
}

fn parse_peers(v: &Value) -> Vec<PeerRow> {
    let Some(map) = v.get("peers").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut rows: Vec<PeerRow> = map
        .iter()
        .map(|(addr, p)| {
            let c = |k: &str| {
                p.pointer(&format!("/counters/{k}"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            };
            PeerRow {
                addr: addr.clone(),
                client: p
                    .get("client_name")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                state: p
                    .get("state")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                connection: p.get("conn_kind").and_then(|k| match k {
                    Value::String(s) => Some(s.clone()),
                    Value::Null => None,
                    other => Some(other.to_string()),
                }),
                downloaded: c("fetched_bytes"),
                uploaded: c("uploaded_bytes"),
                errors: c("errors"),
            }
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.downloaded));
    rows
}

fn hex_hash(hash: &str) -> Option<[u8; 20]> {
    let raw = hex::decode(hash).ok()?;
    raw.try_into().ok()
}

fn urlencode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_public_tracker_list() {
        let list = read_tracker_list(
            "udp://a.example:1337/announce

not a tracker
udp://a.example:1337/announce
https://b.example/announce
",
        );
        assert_eq!(
            list,
            vec![
                "udp://a.example:1337/announce".to_string(),
                "https://b.example/announce".to_string()
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn creates_a_valid_torrent_file() {
        let dir = std::env::temp_dir().join(format!("mdm-create-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("sample.bin");
        std::fs::write(&file, vec![7u8; 600_000]).unwrap();
        let created = create_torrent_file(
            &file,
            vec!["udp://tracker.opentrackr.org:1337/announce".into()],
            Some(256),
        )
        .await
        .unwrap();
        let meta = crate::trackers::read_metainfo(&created.bytes);
        assert_eq!(created.info_hash.len(), 40);
        assert_eq!(created.name, "sample.bin");
        assert_eq!(meta.piece_length, Some(256 * 1024));
        assert!(meta.trackers.iter().any(|t| t.contains("opentrackr")));
        std::fs::remove_dir_all(&dir).ok();
    }
}
