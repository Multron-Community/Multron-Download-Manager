use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use futures_util::stream::FuturesUnordered;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::sync::{Notify, Semaphore, SemaphorePermit};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

pub const SERVER: &str = "wss://api.viruskov.com/ws";
pub const DEFAULT_LIMIT_MB: u32 = 15;
pub const MAX_LIMIT_MB: u32 = 100;
const PROTOCOL: u64 = 3;
const CLIENT: &str = "MultronDownloadManager";
const CHUNK: usize = 256 * 1024;
const MAX_MESSAGE: usize = 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const STOP_POLL: Duration = Duration::from_millis(150);
const MAX_CHECKS_PER_MESSAGE: usize = 50;
const MAX_PIPELINE: usize = 8;
const DEFAULT_CHECKS_PER_SECOND: f64 = 500.0;
const MAX_ATTEMPTS: u32 = 3;
pub const DEFAULT_CONNECTIONS: u32 = 4;
pub const MAX_CONNECTIONS: usize = 8;
const MAX_CACHED: usize = 50_000;
const COMPRESS_MIN: usize = 4096;
const STOPPED: &str = "the scan was stopped";
const TIMED_OUT: &str = "the cloud server did not answer in time";
const STILL_ANALYZING: &str =
    "Multron Cloud is still analyzing this file for another scan; scan it again later";
const INCOMPRESSIBLE: &[&str] = &[
    "zip", "7z", "rar", "gz", "tgz", "bz2", "xz", "zst", "lz4", "cab", "msi", "jar", "apk", "docx",
    "xlsx", "pptx", "odt", "epub", "pdf", "jpg", "jpeg", "png", "gif", "webp", "mp3", "mp4", "mkv",
    "avi", "mov", "webm", "ogg", "flac", "m4a",
];

static CONNECTIONS: Semaphore = Semaphore::const_new(MAX_CONNECTIONS);

#[derive(Debug, Clone)]
struct Tuning {
    keepalive: Duration,
    silent: Duration,
    check_idle: Duration,
    result_timeout: Duration,
    held_timeout: Duration,
    give_up: Duration,
    delays: Vec<Duration>,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            keepalive: Duration::from_secs(20),
            silent: Duration::from_secs(90),
            check_idle: Duration::from_secs(5),
            result_timeout: Duration::from_secs(180),
            held_timeout: Duration::from_secs(60),
            give_up: Duration::from_secs(240),
            delays: [2, 4, 8, 15, 30].map(Duration::from_secs).to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FileVerdict {
    pub verdict: String,
    pub detail: String,
    pub route: &'static str,
    pub size: u64,
    pub uploaded: u64,
}

impl FileVerdict {
    pub fn new(verdict: &str, detail: impl Into<String>) -> Self {
        Self {
            verdict: verdict.into(),
            detail: detail.into(),
            ..Default::default()
        }
    }

    fn by(mut self, route: &'static str, size: u64, uploaded: u64) -> Self {
        self.route = route;
        self.size = size;
        self.uploaded = uploaded;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stage {
    Hashing { done: usize, total: usize },
    Checking { count: usize },
    Waiting { count: usize },
    Uploading { sent: u64, total: u64 },
    Analyzing,
    Reconnecting,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub compress: bool,
    pub connections: usize,
    pub cache: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            compress: true,
            connections: DEFAULT_CONNECTIONS as usize,
            cache: None,
        }
    }
}

const ANALYZED_FOR: u64 = 24 * 3600;

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Cache {
    #[serde(default)]
    files: HashMap<String, (u64, u64, String)>,
    #[serde(default)]
    analyzed: HashMap<String, u64>,
}

static FINGERPRINTS: Mutex<Option<(PathBuf, Cache)>> = Mutex::new(None);

fn modified(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as u64)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn with_cache<T>(cache: &Path, f: impl FnOnce(&mut Cache) -> T) -> Option<T> {
    let mut guard = FINGERPRINTS.lock().ok()?;
    if guard.as_ref().is_none_or(|(p, _)| p != cache) {
        let loaded: Cache = std::fs::read(cache)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        *guard = Some((cache.to_path_buf(), loaded));
    }
    guard.as_mut().map(|(_, c)| f(c))
}

fn cached_fingerprint(cache: &Path, path: &Path) -> Option<(String, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let key = path.to_string_lossy().into_owned();
    with_cache(cache, |c| {
        let (size, stamp, sha) = c.files.get(&key)?;
        (*size == meta.len() && *stamp == modified(&meta)).then(|| (sha.clone(), *size))
    })
    .flatten()
}

fn remember_fingerprint(cache: &Path, path: &Path, sha256: &str, size: u64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    with_cache(cache, |c| {
        if c.files.len() >= MAX_CACHED {
            c.files.clear();
        }
        c.files.insert(
            path.to_string_lossy().into_owned(),
            (size, modified(&meta), sha256.to_string()),
        );
    });
}

fn remember_analyzed(cache: &Path, sha256: &str) {
    with_cache(cache, |c| {
        let now = now_secs();
        c.analyzed
            .retain(|_, at| now.saturating_sub(*at) < ANALYZED_FOR);
        c.analyzed.insert(sha256.to_string(), now);
    });
}

fn recently_analyzed(cache: &Path, sha256: &str) -> bool {
    with_cache(cache, |c| {
        c.analyzed
            .get(sha256)
            .is_some_and(|at| now_secs().saturating_sub(*at) < ANALYZED_FOR)
    })
    .unwrap_or(false)
}

fn save_fingerprints(cache: &Path) {
    let data = FINGERPRINTS.lock().ok().and_then(|g| {
        g.as_ref()
            .filter(|(p, _)| p == cache)
            .map(|(_, c)| serde_json::to_vec(c))
    });
    if let Some(Ok(bytes)) = data {
        let tmp = cache.with_extension("tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, cache);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub state: &'static str,
    pub detail: String,
}

pub async fn scan(
    files: &[PathBuf],
    limit_mb: u32,
    options: &Options,
    stop: &AtomicBool,
    on_stage: impl FnMut(usize, Stage) + Send,
    on_file: impl FnMut(usize, &FileVerdict) + Send,
) -> Summary {
    let verdicts = run_scan(
        &server(),
        files,
        limit_mb,
        options,
        stop,
        &Tuning::default(),
        on_stage,
        on_file,
    )
    .await;
    summarize(&verdicts, limit_mb)
}

trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

fn is_local(url: &url::Url) -> bool {
    matches!(
        url.host(),
        Some(url::Host::Ipv4(ip)) if ip.is_loopback()
    ) || matches!(url.host(), Some(url::Host::Ipv6(ip)) if ip.is_loopback())
        || url.host_str() == Some("localhost")
}

fn server() -> String {
    std::env::var("MULTRON_CLOUD_SERVER")
        .ok()
        .filter(|s| url::Url::parse(s).is_ok_and(|u| is_local(&u)))
        .unwrap_or_else(|| SERVER.to_string())
}

async fn connect(server: &str) -> Result<Box<dyn Io>> {
    let url = url::Url::parse(server)?;
    if url.scheme() == "ws" && is_local(&url) {
        let host = url
            .host_str()
            .unwrap_or("127.0.0.1")
            .trim_matches(['[', ']']);
        let port = url.port().unwrap_or(80);
        let tcp = tokio::time::timeout(
            CONNECT_TIMEOUT,
            tokio::net::TcpStream::connect((host, port)),
        )
        .await
        .map_err(|_| anyhow!(TIMED_OUT))??;
        return Ok(Box::new(tcp));
    }
    Ok(Box::new(connect_tls(server).await?))
}

async fn connect_tls(
    server: &str,
) -> Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>> {
    let url = url::Url::parse(server)?;
    if url.scheme() != "wss" {
        bail!("the cloud scan server must use wss:// (ws:// only on this computer)");
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("the cloud scan address has no host"))?
        .to_string();
    let port = url.port().unwrap_or(443);
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?;
    let config = rustls_platform_verifier::BuilderVerifierExt::with_platform_verifier(config)?
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let name = rustls::pki_types::ServerName::try_from(host.clone())?;
    tokio::time::timeout(CONNECT_TIMEOUT, async {
        let tcp = tokio::net::TcpStream::connect((host.as_str(), port)).await?;
        Ok::<_, anyhow::Error>(connector.connect(name, tcp).await?)
    })
    .await
    .map_err(|_| anyhow!(TIMED_OUT))?
}

fn stopped(stop: &AtomicBool) -> bool {
    stop.load(Ordering::Relaxed)
}

async fn until_stopped(stop: &AtomicBool) {
    while !stopped(stop) {
        tokio::time::sleep(STOP_POLL).await;
    }
}

async fn pause(stop: &AtomicBool, wait: Duration) -> Result<()> {
    tokio::select! {
        _ = tokio::time::sleep(wait) => Ok(()),
        _ = until_stopped(stop) => bail!(STOPPED),
    }
}

#[derive(Debug, Clone)]
struct Item {
    index: usize,
    id: u64,
    sha256: String,
    size: u64,
    name: String,
    folder: String,
    by_hash: Option<FileVerdict>,
    attempts: u32,
}

fn known_by_hash(answer: &Value) -> bool {
    answer["type"] == "result" && verdict_of(answer).verdict != "unknown"
}

enum Upload {
    Done(FileVerdict),
    Sent(u64),
}

struct Session {
    ws: WebSocketStream<Box<dyn Io>>,
    check: bool,
    brotli: bool,
    batch: usize,
    pipeline: usize,
    rate: f64,
    next_check: Option<Instant>,
    last_seen: Instant,
    last_sent: Instant,
    held: HashSet<u64>,
    late: HashMap<u64, Value>,
    _permit: SemaphorePermit<'static>,
}

impl Session {
    async fn open(server: &str, permit: SemaphorePermit<'static>) -> Result<Self> {
        let stream = connect(server).await?;
        let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE))
            .max_frame_size(Some(MAX_MESSAGE));
        let (ws, _) = tokio::time::timeout(
            CONNECT_TIMEOUT,
            tokio_tungstenite::client_async_with_config(server, stream, Some(config)),
        )
        .await
        .map_err(|_| anyhow!(TIMED_OUT))??;
        let mut session = Self {
            ws,
            check: false,
            brotli: false,
            batch: MAX_CHECKS_PER_MESSAGE,
            pipeline: 1,
            rate: DEFAULT_CHECKS_PER_SECOND,
            next_check: None,
            last_seen: Instant::now(),
            last_sent: Instant::now(),
            held: HashSet::new(),
            late: HashMap::new(),
            _permit: permit,
        };
        session
            .send(json!({ "type": "hello", "version": PROTOCOL, "client": CLIENT, "token": "" }))
            .await?;
        let reply = tokio::time::timeout(CONNECT_TIMEOUT, session.receive())
            .await
            .map_err(|_| anyhow!(TIMED_OUT))??;
        if reply["type"] != "hello_ok" {
            let why = reply["message"]
                .as_str()
                .or(reply["type"].as_str())
                .unwrap_or("no reason");
            return Err(Rejected(format!("the server rejected the connection: {why}")).into());
        }
        session.check = reply["version"].as_u64().unwrap_or(2) >= 3;
        session.brotli = reply["encodings"]
            .as_array()
            .is_some_and(|list| list.iter().any(|e| e == "br"));
        session.batch = reply["checkBatch"]
            .as_u64()
            .map_or(MAX_CHECKS_PER_MESSAGE, |n| {
                (n as usize).clamp(1, MAX_CHECKS_PER_MESSAGE)
            });
        session.pipeline = reply["pipeline"]
            .as_u64()
            .map_or(1, |n| (n as usize).clamp(1, MAX_PIPELINE));
        if let Some(rate) = reply["checksPerSec"].as_f64().filter(|r| *r > 0.0) {
            session.rate = (rate * 0.85).max(1.0);
        }
        Ok(session)
    }

    async fn send(&mut self, value: Value) -> Result<()> {
        self.ws.send(Message::text(value.to_string())).await?;
        self.last_sent = Instant::now();
        Ok(())
    }

    async fn receive(&mut self) -> Result<Value> {
        loop {
            let message = self
                .ws
                .next()
                .await
                .ok_or_else(|| anyhow!("the server closed the connection"))??;
            self.last_seen = Instant::now();
            match message {
                Message::Text(text) => return Ok(serde_json::from_str(text.as_str())?),
                Message::Close(_) => bail!("the server closed the connection"),
                Message::Binary(_) => bail!("unexpected binary message from the server"),
                _ => {}
            }
        }
    }

    async fn next_answer(&mut self, stop: &AtomicBool, tune: &Tuning) -> Result<Value> {
        loop {
            let ping_in = tune.keepalive.saturating_sub(self.last_sent.elapsed());
            let silent_left = tune.silent.saturating_sub(self.last_seen.elapsed());
            let message = tokio::select! {
                m = self.receive() => m?,
                _ = tokio::time::sleep(ping_in) => {
                    self.ws.send(Message::Ping(Vec::new().into())).await?;
                    self.last_sent = Instant::now();
                    continue;
                }
                _ = tokio::time::sleep(silent_left) => bail!("the connection to the cloud server was lost"),
                _ = until_stopped(stop) => bail!(STOPPED),
            };
            match message["id"].as_u64() {
                Some(n) if n != 0 => return Ok(message),
                _ if message["type"] == "error" => bail!(
                    "server error: {}",
                    message["message"].as_str().unwrap_or("unknown")
                ),
                _ => {}
            }
        }
    }

    async fn answer(&mut self, id: u64, stop: &AtomicBool, tune: &Tuning) -> Result<Value> {
        tokio::time::timeout(tune.result_timeout, async {
            loop {
                let message = self.next_answer(stop, tune).await?;
                if message["id"].as_u64() == Some(id) {
                    return Ok(message);
                }
                self.keep_late(message);
            }
        })
        .await
        .map_err(|_| anyhow!(TIMED_OUT))?
    }

    fn keep_late(&mut self, message: Value) {
        if let Some(id) = message["id"].as_u64().filter(|id| self.held.contains(id)) {
            self.late.insert(id, message);
        }
    }

    async fn send_check(&mut self, batch: &[Item], stop: &AtomicBool) -> Result<()> {
        if let Some(at) = self.next_check {
            pause(stop, at.saturating_duration_since(Instant::now())).await?;
        }
        let items: Vec<Value> = batch
            .iter()
            .map(|i| json!({ "id": i.id, "name": i.name, "size": i.size, "sha256": i.sha256, "folder": i.folder }))
            .collect();
        self.send(json!({ "type": "check", "items": items }))
            .await?;
        self.next_check =
            Some(Instant::now() + Duration::from_secs_f64(batch.len() as f64 / self.rate));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn upload(
        &mut self,
        path: &Path,
        item: &Item,
        limit: u64,
        compress_uploads: bool,
        stop: &AtomicBool,
        tune: &Tuning,
        stage: &mut (dyn FnMut(Stage) + Send),
    ) -> Result<Upload> {
        if let Some(kind) = crate::archive::locked(path) {
            return Ok(Upload::Done(
                FileVerdict::new(
                    "locked",
                    if kind == crate::archive::Kind::Rar {
                        "password-protected RAR archive; extract it with WinRAR or 7-Zip and scan the files"
                    } else {
                        "password-protected archive; enter its password to scan the files inside"
                    },
                )
                .by("local", item.size, 0),
            ));
        }
        let too_big = || {
            FileVerdict::new(
                "skipped",
                format!("larger than the {} MB upload limit", limit / (1024 * 1024)),
            )
        };
        if item.size > limit {
            if let Some(v) = item.by_hash.clone().filter(|v| v.verdict == "unknown") {
                return Ok(Upload::Done(v.by("hash", item.size, 0)));
            }
            return Ok(Upload::Done(too_big().by("local", item.size, 0)));
        }
        let data = tokio::fs::read(path)
            .await
            .with_context(|| format!("cannot read {}", path.display()))?;
        let size = data.len() as u64;
        if size > limit {
            return Ok(Upload::Done(too_big().by("local", size, 0)));
        }
        let sha256 = hex::encode_upper(Sha256::digest(&data));
        let payload = if compress_uploads && self.brotli && worth_compressing(path, data.len()) {
            tokio::task::spawn_blocking(move || compress(&data).ok_or(data))
                .await?
                .map_or_else(|plain| (plain, false), |packed| (packed, true))
        } else {
            (data, false)
        };
        let (payload, packed) = payload;
        let total = payload.len() as u64;
        let mut message = json!({
            "type": "scan", "id": item.id, "name": item.name, "size": size, "sha256": sha256, "folder": item.folder
        });
        if item.by_hash.is_some() {
            message["rescan"] = json!(true);
        }
        if packed {
            message["encoding"] = json!("br");
            message["csize"] = json!(total);
        }
        self.send(message).await?;
        let ack = self.answer(item.id, stop, tune).await?;
        if ack["type"] != "send_file" {
            return Ok(Upload::Done(verdict_of(&ack).by("hash", size, 0)));
        }
        let mut sent = 0u64;
        stage(Stage::Uploading { sent, total });
        for chunk in payload.chunks(CHUNK) {
            if stopped(stop) {
                bail!(STOPPED);
            }
            self.ws.send(Message::binary(chunk.to_vec())).await?;
            sent += chunk.len() as u64;
            stage(Stage::Uploading { sent, total });
        }
        drop(payload);
        self.last_sent = Instant::now();
        self.last_seen = Instant::now();
        stage(Stage::Analyzing);
        Ok(Upload::Sent(total))
    }

    async fn close(mut self) {
        let _ = tokio::time::timeout(Duration::from_secs(3), self.ws.close(None)).await;
    }
}

#[derive(Debug)]
struct Rejected(String);

impl std::fmt::Display for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Rejected {}

fn worth_compressing(path: &Path, len: usize) -> bool {
    len >= COMPRESS_MIN
        && !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| INCOMPRESSIBLE.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

fn compress(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() / 2);
    let params = brotli::enc::BrotliEncoderParams {
        quality: 5,
        lgwin: 22,
        ..Default::default()
    };
    brotli::BrotliCompress(&mut &data[..], &mut out, &params).ok()?;
    (out.len() < data.len() - data.len() / 10).then_some(out)
}

type Callback<'a, T> = Mutex<Box<dyn FnMut(usize, T) + Send + 'a>>;
type FileCallback<'a> = Mutex<Box<dyn FnMut(usize, &FileVerdict) + Send + 'a>>;

struct Shared<'a> {
    server: &'a str,
    files: &'a [PathBuf],
    limit: u64,
    options: &'a Options,
    stop: &'a AtomicBool,
    tune: &'a Tuning,
    on_stage: Callback<'a, Stage>,
    on_file: FileCallback<'a>,
    results: Mutex<Vec<(usize, FileVerdict)>>,
    queue: Mutex<Vec<Item>>,
    checking: AtomicBool,
    wake: Notify,
    next_id: AtomicU64,
    unreachable: Mutex<Option<Instant>>,
    gave_up: Mutex<Option<String>>,
}

impl Shared<'_> {
    fn stage(&self, index: usize, stage: Stage) {
        if let Ok(mut f) = self.on_stage.lock() {
            f(index, stage);
        }
    }

    fn done(&self, index: usize, verdict: FileVerdict) {
        if let Ok(mut f) = self.on_file.lock() {
            f(index, &verdict);
        }
        if let Ok(mut r) = self.results.lock() {
            r.push((index, verdict));
        }
    }

    fn fail(&self, item: &Item, why: &str) {
        self.done(
            item.index,
            FileVerdict::new("error", why).by("hash", item.size, 0),
        );
    }

    fn enqueue(&self, item: Item) {
        if let Ok(mut q) = self.queue.lock() {
            q.push(item);
        }
        self.wake.notify_waiters();
    }

    fn retry(&self, mut item: Item, why: &str) {
        item.attempts += 1;
        if item.attempts >= MAX_ATTEMPTS {
            self.fail(&item, why);
        } else {
            self.enqueue(item);
        }
    }

    fn take(&self) -> Option<Item> {
        let mut q = self.queue.lock().ok()?;
        let at = (0..q.len()).min_by_key(|&i| q[i].size)?;
        Some(q.swap_remove(at))
    }

    fn queued(&self) -> usize {
        self.queue.lock().map(|q| q.len()).unwrap_or(0)
    }

    fn gave_up(&self) -> Option<String> {
        self.gave_up.lock().ok()?.clone()
    }

    fn item(&self, index: usize, path: &Path, sha256: String, size: u64) -> Item {
        Item {
            index,
            id: self.next_id.fetch_add(1, Ordering::Relaxed) + 1,
            sha256,
            size,
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            folder: normalize_folder(path),
            by_hash: None,
            attempts: 0,
        }
    }

    async fn open(
        &self,
        index: usize,
        permit: Option<SemaphorePermit<'static>>,
    ) -> Result<Session> {
        let mut permit = permit;
        let mut failures = 0usize;
        loop {
            if stopped(self.stop) {
                bail!(STOPPED);
            }
            if let Some(why) = self.gave_up() {
                bail!(why);
            }
            let ticket = match permit.take() {
                Some(p) => p,
                None => tokio::select! {
                    p = CONNECTIONS.acquire() => p?,
                    _ = until_stopped(self.stop) => bail!(STOPPED),
                },
            };
            match Session::open(self.server, ticket).await {
                Ok(session) => {
                    if let Ok(mut u) = self.unreachable.lock() {
                        *u = None;
                    }
                    return Ok(session);
                }
                Err(e) if e.downcast_ref::<Rejected>().is_some() => {
                    let why = format!("{e:#}");
                    if let Ok(mut g) = self.gave_up.lock() {
                        *g = Some(why.clone());
                    }
                    bail!(why);
                }
                Err(e) => {
                    let since = self
                        .unreachable
                        .lock()
                        .map(|mut u| *u.get_or_insert_with(Instant::now))
                        .unwrap_or_else(|_| Instant::now());
                    if since.elapsed() >= self.tune.give_up {
                        let why = format!("could not reach the Multron Cloud server: {e:#}");
                        if let Ok(mut g) = self.gave_up.lock() {
                            *g = Some(why.clone());
                        }
                        bail!(why);
                    }
                    let wait = self.tune.delays[failures.min(self.tune.delays.len() - 1)];
                    failures += 1;
                    self.stage(index, Stage::Reconnecting);
                    pause(self.stop, wait).await?;
                }
            }
        }
    }
}

async fn checker(sh: &Shared<'_>) {
    let total = sh.files.len();
    let mut session: Option<Session> = None;
    let mut held: Vec<Item> = Vec::new();
    let mut start = 0;
    while start < total && !stopped(sh.stop) {
        if let Some(why) = sh.gave_up() {
            for index in start..total {
                sh.done(index, FileVerdict::new("error", why.clone()));
            }
            break;
        }
        let end = (start + session.as_ref().map_or(MAX_CHECKS_PER_MESSAGE, |s| s.batch)).min(total);
        let mut batch = Vec::new();
        for (index, path) in sh.files.iter().enumerate().take(end).skip(start) {
            if stopped(sh.stop) {
                break;
            }
            sh.stage(index, Stage::Hashing { done: index, total });
            let known = sh
                .options
                .cache
                .as_deref()
                .and_then(|c| cached_fingerprint(c, path));
            let hashed = match known {
                Some(hit) => Ok(hit),
                None => hash_file(path, sh.stop).await,
            };
            match hashed {
                Ok((sha256, size)) => {
                    if let Some(c) = sh.options.cache.as_deref() {
                        remember_fingerprint(c, path, &sha256, size);
                    }
                    batch.push(sh.item(index, path, sha256, size))
                }
                Err(_) if stopped(sh.stop) => break,
                Err(e) => sh.done(
                    index,
                    FileVerdict::new("error", format!("cannot read {}: {e:#}", path.display())),
                ),
            }
        }
        start = end;
        while !batch.is_empty() && !stopped(sh.stop) {
            if session.is_none() {
                match sh.open(batch[0].index, None).await {
                    Ok(s) => session = Some(s),
                    Err(e) => {
                        let why = format!("{e:#}");
                        if !stopped(sh.stop) {
                            for item in batch.drain(..).chain(held.drain(..)) {
                                sh.fail(&item, &why);
                            }
                        }
                        break;
                    }
                }
            }
            let Some(s) = session.as_mut() else { break };
            if !s.check {
                for item in batch.drain(..) {
                    sh.enqueue(item);
                }
                break;
            }
            sh.stage(batch[0].index, Stage::Checking { count: batch.len() });
            match check(sh, s, &mut batch).await {
                Ok(()) => {
                    for item in batch.drain(..) {
                        s.held.insert(item.id);
                        held.push(item);
                    }
                    settle_held(sh, s, &mut held);
                }
                Err(_) if stopped(sh.stop) => break,
                Err(e) => {
                    if let Some(old) = session.take() {
                        drop(old);
                    }
                    let why = format!("{e:#}");
                    let mut again = Vec::new();
                    for mut item in batch.drain(..).chain(held.drain(..)) {
                        item.attempts += 1;
                        if item.attempts >= MAX_ATTEMPTS {
                            sh.fail(&item, &why);
                        } else {
                            again.push(item);
                        }
                    }
                    batch = again;
                }
            }
        }
    }
    let held_since = Instant::now();
    while !held.is_empty() && !stopped(sh.stop) {
        let Some(s) = session.as_mut() else {
            let mut again = std::mem::take(&mut held);
            match sh.open(again[0].index, None).await {
                Ok(new) => {
                    session = Some(new);
                    let s = session.as_mut().expect("just opened");
                    sh.stage(again[0].index, Stage::Checking { count: again.len() });
                    match check(sh, s, &mut again).await {
                        Ok(()) => {
                            for item in again.drain(..) {
                                s.held.insert(item.id);
                                held.push(item);
                            }
                        }
                        Err(e) => {
                            session = None;
                            let why = format!("{e:#}");
                            for mut item in again {
                                item.attempts += 1;
                                if item.attempts >= MAX_ATTEMPTS {
                                    sh.fail(&item, &why);
                                } else {
                                    held.push(item);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    if !stopped(sh.stop) {
                        let why = format!("{e:#}");
                        for item in &again {
                            sh.fail(item, &why);
                        }
                    }
                }
            }
            continue;
        };
        settle_held(sh, s, &mut held);
        if held.is_empty() {
            break;
        }
        sh.stage(held[0].index, Stage::Waiting { count: held.len() });
        let left = sh.tune.held_timeout.saturating_sub(held_since.elapsed());
        match tokio::time::timeout(left, s.next_answer(sh.stop, sh.tune)).await {
            Ok(Ok(message)) => s.keep_late(message),
            Ok(Err(_)) if stopped(sh.stop) => break,
            Ok(Err(_)) => {
                session = None;
                for item in held.iter_mut() {
                    item.attempts += 1;
                }
                let (dead, live): (Vec<Item>, Vec<Item>) =
                    held.drain(..).partition(|i| i.attempts >= MAX_ATTEMPTS);
                for item in &dead {
                    sh.fail(item, "the connection to the cloud server was lost");
                }
                held = live;
            }
            Err(_) => {
                for item in held.drain(..) {
                    sh.fail(&item, STILL_ANALYZING);
                }
            }
        }
    }
    sh.checking.store(false, Ordering::Relaxed);
    sh.wake.notify_waiters();
    uploader(sh, session, None).await;
}

fn dispatch(sh: &Shared<'_>, mut item: Item, answer: &Value) {
    let reuse = answer["type"] == "result"
        && sh
            .options
            .cache
            .as_deref()
            .is_some_and(|c| recently_analyzed(c, &item.sha256));
    if known_by_hash(answer) || reuse {
        sh.done(item.index, verdict_of(answer).by("hash", item.size, 0));
    } else {
        item.by_hash = (answer["type"] != "need_upload").then(|| verdict_of(answer));
        sh.enqueue(item);
    }
}

async fn check(sh: &Shared<'_>, s: &mut Session, batch: &mut Vec<Item>) -> Result<()> {
    s.send_check(batch, sh.stop).await?;
    while !batch.is_empty() {
        let Ok(message) =
            tokio::time::timeout(sh.tune.check_idle, s.next_answer(sh.stop, sh.tune)).await
        else {
            break;
        };
        let message = message?;
        let id = message["id"].as_u64();
        match batch.iter().position(|i| Some(i.id) == id) {
            Some(p) => {
                let item = batch.swap_remove(p);
                dispatch(sh, item, &message);
            }
            None => s.keep_late(message),
        }
    }
    Ok(())
}

fn settle_held(sh: &Shared<'_>, s: &mut Session, held: &mut Vec<Item>) {
    let mut waiting = Vec::new();
    for item in held.drain(..) {
        match s.late.remove(&item.id) {
            Some(a) => {
                s.held.remove(&item.id);
                dispatch(sh, item, &a);
            }
            None => waiting.push(item),
        }
    }
    *held = waiting;
}

async fn uploader(
    sh: &Shared<'_>,
    session: Option<Session>,
    permit: Option<SemaphorePermit<'static>>,
) {
    let mut session = session;
    let mut permit = permit;
    let mut inflight: Vec<(Item, u64, Instant)> = Vec::new();
    while !stopped(sh.stop) {
        if let Some(s) = session.as_mut() {
            if let Some(p) = inflight
                .iter()
                .position(|(i, _, _)| s.late.contains_key(&i.id))
            {
                let (item, total, _) = inflight.swap_remove(p);
                s.held.remove(&item.id);
                let reply = s.late.remove(&item.id).unwrap_or_default();
                if reply["type"] == "result" {
                    if let Some(c) = sh.options.cache.as_deref() {
                        remember_analyzed(c, &item.sha256);
                    }
                }
                sh.done(
                    item.index,
                    verdict_of(&reply).by("upload", item.size, total),
                );
                continue;
            }
            if let Some(p) = inflight
                .iter()
                .position(|(_, _, at)| at.elapsed() >= sh.tune.result_timeout)
            {
                let (item, total, _) = inflight.swap_remove(p);
                s.held.remove(&item.id);
                sh.done(
                    item.index,
                    FileVerdict::new(
                        "error",
                        format!(
                            "Multron Cloud did not finish analyzing it within {} minutes; scan it again later",
                            sh.tune.result_timeout.as_secs() / 60
                        ),
                    )
                    .by("upload", item.size, total),
                );
                continue;
            }
        }
        let room = session.as_ref().map_or(1, |s| s.pipeline);
        if inflight.len() < room {
            if let Some(item) = sh.take() {
                if session.is_none() {
                    match sh.open(item.index, permit.take()).await {
                        Ok(s) => session = Some(s),
                        Err(e) => {
                            if stopped(sh.stop) {
                                break;
                            }
                            sh.fail(&item, &format!("{e:#}"));
                            continue;
                        }
                    }
                }
                let Some(s) = session.as_mut() else { continue };
                let path = &sh.files[item.index];
                let index = item.index;
                let mut stage = |st: Stage| sh.stage(index, st);
                match s
                    .upload(
                        path,
                        &item,
                        sh.limit,
                        sh.options.compress,
                        sh.stop,
                        sh.tune,
                        &mut stage,
                    )
                    .await
                {
                    Ok(Upload::Done(v)) => sh.done(index, v),
                    Ok(Upload::Sent(total)) => {
                        s.held.insert(item.id);
                        inflight.push((item, total, Instant::now()));
                    }
                    Err(_) if stopped(sh.stop) => break,
                    Err(e) => {
                        let why = format!("{e:#}");
                        if why.starts_with("cannot read") {
                            sh.done(index, FileVerdict::new("error", why));
                            continue;
                        }
                        session = None;
                        sh.stage(index, Stage::Reconnecting);
                        sh.retry(item, &why);
                        for (item, _, _) in inflight.drain(..) {
                            sh.retry(item, &why);
                        }
                    }
                }
                continue;
            }
        }
        if inflight.is_empty() {
            if !sh.checking.load(Ordering::Relaxed) && sh.queued() == 0 {
                break;
            }
            tokio::select! {
                _ = sh.wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(200)) => {}
            }
            continue;
        }
        let Some(s) = session.as_mut() else { continue };
        if let Some((item, _, _)) = inflight.first() {
            sh.stage(item.index, Stage::Analyzing);
        }
        let deadline = inflight
            .iter()
            .map(|(_, _, at)| sh.tune.result_timeout.saturating_sub(at.elapsed()))
            .min()
            .unwrap_or(sh.tune.result_timeout);
        match tokio::time::timeout(deadline, s.next_answer(sh.stop, sh.tune)).await {
            Ok(Ok(message)) => s.keep_late(message),
            Ok(Err(_)) if stopped(sh.stop) => break,
            Ok(Err(e)) => {
                let why = format!("{e:#}");
                session = None;
                for (item, _, _) in inflight.drain(..) {
                    sh.stage(item.index, Stage::Reconnecting);
                    sh.retry(item, &why);
                }
            }
            Err(_) => {}
        }
    }
    if let Some(s) = session {
        s.close().await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_scan(
    server: &str,
    files: &[PathBuf],
    limit_mb: u32,
    options: &Options,
    stop: &AtomicBool,
    tune: &Tuning,
    on_stage: impl FnMut(usize, Stage) + Send,
    on_file: impl FnMut(usize, &FileVerdict) + Send,
) -> Vec<(PathBuf, FileVerdict)> {
    let sh = Shared {
        server,
        files,
        limit: u64::from(limit_mb.clamp(1, MAX_LIMIT_MB)) * 1024 * 1024,
        options,
        stop,
        tune,
        on_stage: Mutex::new(Box::new(on_stage)),
        on_file: Mutex::new(Box::new(on_file)),
        results: Mutex::new(Vec::new()),
        queue: Mutex::new(Vec::new()),
        checking: AtomicBool::new(true),
        wake: Notify::new(),
        next_id: AtomicU64::new(0),
        unreachable: Mutex::new(None),
        gave_up: Mutex::new(None),
    };
    {
        let mut workers: FuturesUnordered<Pin<Box<dyn Future<Output = ()> + Send + '_>>> =
            FuturesUnordered::new();
        workers.push(Box::pin(checker(&sh)));
        let mut extras = 0;
        while !workers.is_empty() {
            tokio::select! {
                _ = workers.next() => {}
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
            let queued = sh.queued();
            let busy = sh.checking.load(Ordering::Relaxed);
            if extras + 1 < options.connections.clamp(1, MAX_CONNECTIONS)
                && (queued > extras + 1 || (busy && queued > extras))
                && !stopped(stop)
            {
                if let Ok(permit) = CONNECTIONS.try_acquire() {
                    workers.push(Box::pin(uploader(&sh, None, Some(permit))));
                    extras += 1;
                }
            }
        }
    }
    if let Some(c) = options.cache.as_deref() {
        save_fingerprints(c);
    }
    let mut results = sh.results.into_inner().unwrap_or_default();
    results.sort_by_key(|(index, _)| *index);
    results
        .into_iter()
        .map(|(index, v)| (files[index].clone(), v))
        .collect()
}

async fn hash_file(path: &Path, stop: &AtomicBool) -> Result<(String, u64)> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut size = 0u64;
    loop {
        if stopped(stop) {
            bail!(STOPPED);
        }
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok((hex::encode_upper(hasher.finalize()), size))
}

pub fn normalize_verdict(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "clean" => "clean",
        "possible_clean" | "xorfilter_clean" => "possible_clean",
        "malicious" | "xorfilter_malicious" => "malicious",
        "suspicious" => "suspicious",
        "error" | "rejected" => "error",
        _ => "unknown",
    }
    .to_string()
}

fn verdict_of(reply: &Value) -> FileVerdict {
    if reply["type"] != "result" {
        return FileVerdict::new("error", reply["message"].as_str().unwrap_or("server error"));
    }
    let av = &reply["antivirus"];
    let verdict = normalize_verdict(av["verdict"].as_str().unwrap_or(""));
    let threat = reply["threat"]["indicator"]["name"]
        .as_str()
        .or(reply["rule"]["name"].as_str())
        .unwrap_or("");
    let detections: Vec<String> = av["detections"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|d| {
                    let name = d["name"].as_str()?;
                    let layer = d["layer"].as_str().unwrap_or("");
                    Some(if layer.is_empty() {
                        name.to_string()
                    } else {
                        format!("{name} ({layer})")
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let detail = if !threat.is_empty() {
        threat.to_string()
    } else if !detections.is_empty() {
        detections.join(", ")
    } else {
        av["detail"].as_str().unwrap_or("").to_string()
    };
    FileVerdict::new(&verdict, detail)
}

pub fn summarize(verdicts: &[(PathBuf, FileVerdict)], limit_mb: u32) -> Summary {
    let named = |wanted: &[&str]| -> Vec<String> {
        verdicts
            .iter()
            .filter(|(_, v)| wanted.contains(&v.verdict.as_str()))
            .map(|(p, v)| {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if v.detail.is_empty() {
                    name
                } else {
                    format!("{name}: {}", v.detail)
                }
            })
            .collect::<Vec<String>>()
    };
    let named = |wanted: &[&str]| -> Vec<String> {
        let mut list = named(wanted);
        if list.len() > 5 {
            let more = list.len() - 5;
            list.truncate(5);
            list.push(format!("and {more} more"));
        }
        list
    };
    let threats = named(&["malicious"]);
    if !threats.is_empty() {
        return Summary {
            state: "threat",
            detail: format!("Multron Cloud: {}", threats.join(", ")),
        };
    }
    let suspicious = named(&["suspicious"]);
    if !suspicious.is_empty() {
        return Summary {
            state: "suspicious",
            detail: format!("Multron Cloud: {}", suspicious.join(", ")),
        };
    }
    let clean = verdicts
        .iter()
        .filter(|(_, v)| matches!(v.verdict.as_str(), "clean" | "possible_clean"))
        .count();
    if clean == verdicts.len() && clean > 0 {
        return Summary {
            state: "clean",
            detail: "Multron Cloud".into(),
        };
    }
    let locked = named(&["locked"]);
    if !locked.is_empty() {
        return Summary {
            state: "locked",
            detail: format!("Multron Cloud: {}", locked.join(", ")),
        };
    }
    let count = |wanted: &str| verdicts.iter().filter(|(_, v)| v.verdict == wanted).count();
    let unknown = count("unknown");
    let checked = clean + unknown;
    let skipped = count("skipped");
    let errors = named(&["error"]);
    let missed = verdicts.len() - checked;
    if checked > 0 && missed == 0 {
        return Summary {
            state: "unknown",
            detail: format!(
                "Multron Cloud: no threat found; {unknown} of {} files are not verified as clean yet",
                verdicts.len()
            ),
        };
    }
    if checked > 0 {
        let mut why = errors;
        if skipped > 0 {
            why.push(format!(
                "{skipped} larger than the {limit_mb} MB upload limit"
            ));
        }
        return Summary {
            state: "partial",
            detail: format!(
                "Multron Cloud: no threat found in {checked} of {} files; {missed} could not be checked ({})",
                verdicts.len(),
                why.join(", ")
            ),
        };
    }
    if !errors.is_empty() {
        return Summary {
            state: "error",
            detail: format!("Multron Cloud: {}", errors.join(", ")),
        };
    }
    if skipped == 0 {
        return Summary {
            state: "skipped",
            detail: "Multron Cloud: no file was checked".into(),
        };
    }
    if verdicts.len() == 1 {
        return Summary {
            state: "skipped",
            detail: format!(
                "Multron Cloud: the file is larger than the {limit_mb} MB upload limit and the cloud does not know its fingerprint"
            ),
        };
    }
    Summary {
        state: "skipped",
        detail: format!(
            "Multron Cloud: {skipped} of {} files are larger than the {limit_mb} MB upload limit",
            verdicts.len()
        ),
    }
}

fn normalize_folder(path: &Path) -> String {
    let Some(dir) = path.parent() else {
        return String::new();
    };
    let dir = dir
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_string();
    let mut places: Vec<(&str, String)> = Vec::new();
    for (name, var) in [
        ("%TEMP%", "TEMP"),
        ("%LOCALAPPDATA%", "LOCALAPPDATA"),
        ("%APPDATA%", "APPDATA"),
        ("%USERPROFILE%", "USERPROFILE"),
        ("%USERPROFILE%", "HOME"),
    ] {
        if let Some(value) = std::env::var_os(var) {
            let value = value
                .to_string_lossy()
                .trim_end_matches(['\\', '/'])
                .to_string();
            if !value.is_empty() {
                places.push((name, value));
            }
        }
    }
    places.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    let folded = |s: &str| {
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s.to_string()
        }
    };
    let lower = folded(&dir);
    for (name, value) in &places {
        let v = folded(value);
        if lower == v {
            return name.to_string();
        }
        if lower.starts_with(&format!("{v}\\")) || lower.starts_with(&format!("{v}/")) {
            let rest = &dir[value.len()..];
            return format!("{name}{rest}").chars().take(260).collect();
        }
    }
    dir.rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .chars()
        .take(260)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mdm-cloud-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[derive(Clone, Default)]
    struct Fake {
        known: Option<String>,
        unknown_by_hash: bool,
        hold_first: bool,
        keep_held: bool,
        brotli: bool,
        pipeline: u64,
        drop_uploads: usize,
        result_delay: Duration,
        reject: bool,
        silent: bool,
    }

    #[derive(Default)]
    struct Log {
        uploaded: Mutex<Vec<u8>>,
        connections: AtomicUsize,
        pings: AtomicUsize,
        rescans: AtomicUsize,
        compressed: AtomicUsize,
        drops: AtomicUsize,
        finished: tokio::sync::Notify,
    }

    async fn fake(cfg: Fake) -> (String, Arc<Log>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let log = Arc::new(Log::default());
        let shared = log.clone();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let cfg = cfg.clone();
                let log = shared.clone();
                tokio::spawn(async move {
                    let Ok(mut ws) = accept_async(tcp).await else {
                        return;
                    };
                    log.connections.fetch_add(1, Ordering::Relaxed);
                    let mut held: Option<u64> = None;
                    let mut first_check = true;
                    let mut upload: Option<(u64, usize, bool)> = None;
                    let mut buffer: Vec<u8> = Vec::new();
                    let mut later: Vec<(Instant, Value)> = Vec::new();
                    loop {
                        let next = later.iter().map(|(at, _)| *at).min();
                        let msg = tokio::select! {
                            m = ws.next() => m,
                            _ = log.finished.notified(), if held.is_some() && !cfg.keep_held => {
                                if let Some(h) = held.take() {
                                    let r = json!({ "type": "result", "id": h, "antivirus": { "verdict": "clean" } });
                                    let _ = ws.send(Message::text(r.to_string())).await;
                                }
                                continue;
                            }
                            _ = tokio::time::sleep_until(next.unwrap_or_else(|| Instant::now() + Duration::from_secs(3600)).into()), if next.is_some() => {
                                let now = Instant::now();
                                let (due, rest): (Vec<_>, Vec<_>) = later.drain(..).partition(|(at, _)| *at <= now);
                                later = rest;
                                for (_, v) in due {
                                    let _ = ws.send(Message::text(v.to_string())).await;
                                }
                                continue;
                            }
                        };
                        let Some(Ok(msg)) = msg else { break };
                        let mut replies: Vec<Value> = Vec::new();
                        match msg {
                            Message::Text(text) => {
                                let v: Value = serde_json::from_str(text.as_str()).unwrap();
                                match v["type"].as_str().unwrap() {
                                    "hello" => {
                                        assert_eq!(v["client"], CLIENT);
                                        if cfg.reject {
                                            replies.push(
                                                json!({ "type": "error", "message": "bad token" }),
                                            );
                                        } else {
                                            let mut ok =
                                                json!({ "type": "hello_ok", "version": 3 });
                                            if cfg.pipeline > 0 {
                                                ok["pipeline"] = json!(cfg.pipeline);
                                            }
                                            if cfg.brotli {
                                                ok["encodings"] = json!(["br"]);
                                            }
                                            replies.push(ok);
                                        }
                                    }
                                    "check" if !cfg.silent => {
                                        for item in v["items"].as_array().unwrap() {
                                            let id = item["id"].as_u64().unwrap();
                                            assert!(item.get("path").is_none());
                                            if cfg.hold_first && first_check {
                                                first_check = false;
                                                held = Some(id);
                                                continue;
                                            }
                                            first_check = false;
                                            let sha = item["sha256"].as_str().unwrap();
                                            replies.push(if Some(sha) == cfg.known.as_deref() {
                                                json!({ "type": "result", "id": id, "antivirus": { "verdict": "malicious" }, "threat": { "indicator": { "name": "Test.Eicar" } } })
                                            } else if cfg.unknown_by_hash {
                                                json!({ "type": "result", "id": id, "antivirus": { "verdict": "unknown" } })
                                            } else {
                                                json!({ "type": "need_upload", "id": id })
                                            });
                                        }
                                    }
                                    "scan" => {
                                        let id = v["id"].as_u64().unwrap();
                                        if v["rescan"] == true {
                                            log.rescans.fetch_add(1, Ordering::Relaxed);
                                        }
                                        let packed = v["encoding"] == "br";
                                        let len = if packed {
                                            log.compressed.fetch_add(1, Ordering::Relaxed);
                                            v["csize"].as_u64().unwrap()
                                        } else {
                                            v["size"].as_u64().unwrap()
                                        };
                                        upload = Some((id, len as usize, packed));
                                        buffer.clear();
                                        replies.push(json!({ "type": "send_file", "id": id }));
                                    }
                                    _ => {}
                                }
                            }
                            Message::Binary(data) => {
                                if log.drops.load(Ordering::Relaxed) < cfg.drop_uploads {
                                    log.drops.fetch_add(1, Ordering::Relaxed);
                                    break;
                                }
                                buffer.extend_from_slice(&data);
                                if let Some((id, len, packed)) = upload {
                                    if buffer.len() >= len {
                                        let plain = if packed {
                                            let mut out = Vec::new();
                                            brotli::BrotliDecompress(&mut &buffer[..], &mut out)
                                                .unwrap();
                                            out
                                        } else {
                                            buffer.clone()
                                        };
                                        log.uploaded.lock().unwrap().extend_from_slice(&plain);
                                        let result = json!({ "type": "result", "id": id, "antivirus": { "verdict": "clean" } });
                                        if cfg.result_delay > Duration::ZERO {
                                            later.push((Instant::now() + cfg.result_delay, result));
                                        } else {
                                            replies.push(result);
                                        }
                                        if let Some(h) = held.take().filter(|_| !cfg.keep_held) {
                                            replies.push(json!({ "type": "result", "id": h, "antivirus": { "verdict": "clean" } }));
                                        }
                                        log.finished.notify_waiters();
                                        upload = None;
                                    }
                                }
                            }
                            Message::Ping(_) => {
                                log.pings.fetch_add(1, Ordering::Relaxed);
                            }
                            Message::Close(_) => break,
                            _ => {}
                        }
                        for r in replies {
                            if ws.send(Message::text(r.to_string())).await.is_err() {
                                return;
                            }
                        }
                    }
                });
            }
        });
        (format!("ws://{addr}/ws"), log)
    }

    fn quick() -> Tuning {
        Tuning {
            check_idle: Duration::from_millis(200),
            give_up: Duration::from_millis(600),
            delays: vec![Duration::from_millis(20)],
            ..Tuning::default()
        }
    }

    async fn run_with(
        server: &str,
        files: &[PathBuf],
        limit_mb: u32,
        stop: &AtomicBool,
        tune: &Tuning,
    ) -> (Summary, Vec<(PathBuf, FileVerdict)>, Vec<(usize, Stage)>) {
        let mut stages = Vec::new();
        let verdicts = run_scan(
            server,
            files,
            limit_mb,
            &Options::default(),
            stop,
            tune,
            |i, s| stages.push((i, s)),
            |_, _| {},
        )
        .await;
        (summarize(&verdicts, limit_mb), verdicts, stages)
    }

    async fn run(server: &str, files: &[PathBuf], limit_mb: u32) -> Summary {
        run_with(server, files, limit_mb, &AtomicBool::new(false), &quick())
            .await
            .0
    }

    #[tokio::test]
    async fn checks_every_fingerprint_before_uploading() {
        let dir = temp("stages");
        let first = dir.join("new.exe");
        let second = dir.join("other.exe");
        std::fs::write(&first, noise(CHUNK + 10, 1)).unwrap();
        std::fs::write(&second, b"second").unwrap();
        let (server, log) = fake(Fake::default()).await;
        let files = vec![first, second];
        let (summary, verdicts, stages) =
            run_with(&server, &files, 15, &AtomicBool::new(false), &quick()).await;
        assert_eq!(
            &stages[..3],
            &[
                (0, Stage::Hashing { done: 0, total: 2 }),
                (1, Stage::Hashing { done: 1, total: 2 }),
                (0, Stage::Checking { count: 2 }),
            ]
        );
        assert!(stages[3..]
            .iter()
            .all(|(_, s)| !matches!(s, Stage::Hashing { .. } | Stage::Checking { .. })));
        assert_eq!(summary.state, "clean");
        assert!(verdicts.iter().all(|(_, v)| v.route == "upload"));
        assert_eq!(log.uploaded.lock().unwrap().len(), CHUNK + 10 + 6);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_stopped_scan_checks_nothing_more() {
        let dir = temp("stop");
        let file = dir.join("setup.exe");
        std::fs::write(&file, b"hello").unwrap();
        let (server, log) = fake(Fake::default()).await;
        let (summary, _, stages) = run_with(
            &server,
            std::slice::from_ref(&file),
            15,
            &AtomicBool::new(true),
            &quick(),
        )
        .await;
        assert!(stages.is_empty());
        assert_eq!(summary.state, "skipped");
        assert!(log.uploaded.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn stop_does_not_wait_for_a_silent_server() {
        let dir = temp("silent");
        let file = dir.join("setup.exe");
        std::fs::write(&file, b"hello").unwrap();
        let (server, _log) = fake(Fake {
            silent: true,
            ..Fake::default()
        })
        .await;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            flag.store(true, Ordering::Relaxed);
        });
        let tune = Tuning {
            check_idle: Duration::from_secs(30),
            ..quick()
        };
        let started = Instant::now();
        let (summary, _, _) =
            run_with(&server, std::slice::from_ref(&file), 15, &stop, &tune).await;
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(summary.state, "skipped");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn uploads_unknown_files_and_reports_clean() {
        let dir = temp("clean");
        let file = dir.join("setup.exe");
        std::fs::write(&file, b"hello from multron").unwrap();
        let (server, log) = fake(Fake::default()).await;
        let summary = run(&server, std::slice::from_ref(&file), 15).await;
        assert_eq!(summary.state, "clean");
        assert_eq!(*log.uploaded.lock().unwrap(), b"hello from multron");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn known_hashes_are_not_uploaded() {
        let dir = temp("known");
        let file = dir.join("bad.exe");
        std::fs::write(&file, b"bad").unwrap();
        let (server, log) = fake(Fake {
            known: Some(hex::encode_upper(Sha256::digest(b"bad"))),
            ..Fake::default()
        })
        .await;
        let summary = run(&server, std::slice::from_ref(&file), 15).await;
        assert_eq!(summary.state, "threat");
        assert!(summary.detail.contains("Test.Eicar"));
        assert!(log.uploaded.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn password_protected_archives_are_not_uploaded() {
        use std::io::Write;
        let dir = temp("locked");
        let file = dir.join("files.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&file).unwrap());
        zip.start_file(
            "setup.exe",
            zip::write::SimpleFileOptions::default()
                .with_aes_encryption(zip::AesMode::Aes256, "secret"),
        )
        .unwrap();
        zip.write_all(b"MZ pretend program").unwrap();
        zip.finish().unwrap();
        let (server, log) = fake(Fake::default()).await;
        let summary = run(&server, std::slice::from_ref(&file), 15).await;
        assert_eq!(summary.state, "locked");
        assert!(summary.detail.contains("password"));
        assert!(log.uploaded.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn files_over_the_limit_are_not_uploaded() {
        let dir = temp("large");
        let file = dir.join("big.bin");
        std::fs::write(&file, vec![7u8; 1024 * 1024 + 1]).unwrap();
        let (server, log) = fake(Fake::default()).await;
        let summary = run(&server, std::slice::from_ref(&file), 1).await;
        assert_eq!(summary.state, "skipped");
        assert!(summary.detail.contains("1 MB"));
        assert!(log.uploaded.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_held_answer_does_not_block_the_other_files() {
        let dir = temp("held");
        let first = dir.join("held.exe");
        let second = dir.join("new.exe");
        std::fs::write(&first, b"held by another upload").unwrap();
        std::fs::write(&second, b"unknown file").unwrap();
        let (server, _log) = fake(Fake {
            hold_first: true,
            ..Fake::default()
        })
        .await;
        let files = vec![first, second];
        let (_, verdicts, _) =
            run_with(&server, &files, 15, &AtomicBool::new(false), &quick()).await;
        assert_eq!(verdicts.len(), 2);
        assert_eq!(verdicts[0].1.route, "hash");
        assert_eq!(verdicts[0].1.verdict, "clean");
        assert_eq!(verdicts[1].1.route, "upload");
    }

    #[tokio::test]
    async fn files_the_cloud_only_knows_as_unknown_are_uploaded_again() {
        let dir = temp("unknown");
        let file = dir.join("setup.exe");
        std::fs::write(&file, b"seen before, not classified").unwrap();
        let (server, log) = fake(Fake {
            unknown_by_hash: true,
            ..Fake::default()
        })
        .await;
        let summary = run(&server, std::slice::from_ref(&file), 15).await;
        assert_eq!(summary.state, "clean");
        assert_eq!(log.rescans.load(Ordering::Relaxed), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_file_analyzed_recently_is_not_uploaded_again() {
        let dir = temp("recent");
        let file = dir.join("setup.exe");
        std::fs::write(&file, noise(2000, 77)).unwrap();
        let (server, log) = fake(Fake {
            unknown_by_hash: true,
            ..Fake::default()
        })
        .await;
        let options = Options {
            cache: Some(dir.join("cache.json")),
            ..Options::default()
        };
        for _ in 0..2 {
            run_scan(
                &server,
                std::slice::from_ref(&file),
                15,
                &options,
                &AtomicBool::new(false),
                &quick(),
                |_, _| {},
                |_, _| {},
            )
            .await;
        }
        assert_eq!(log.rescans.load(Ordering::Relaxed), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn compressible_files_are_sent_with_brotli() {
        let dir = temp("brotli");
        let file = dir.join("script.ps1");
        let text = "Write-Host 'multron download manager'\r\n".repeat(4000);
        std::fs::write(&file, &text).unwrap();
        let (server, log) = fake(Fake {
            brotli: true,
            ..Fake::default()
        })
        .await;
        let (summary, verdicts, _) = run_with(
            &server,
            std::slice::from_ref(&file),
            15,
            &AtomicBool::new(false),
            &quick(),
        )
        .await;
        assert_eq!(summary.state, "clean");
        assert_eq!(log.compressed.load(Ordering::Relaxed), 1);
        assert_eq!(*log.uploaded.lock().unwrap(), text.as_bytes());
        assert!(verdicts[0].1.uploaded < text.len() as u64 / 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remembers_fingerprints_of_unchanged_files() {
        let dir = temp("cache");
        let file = dir.join("setup.exe");
        let cache = dir.join("cloud-fingerprints.json");
        std::fs::write(&file, b"first").unwrap();
        assert!(cached_fingerprint(&cache, &file).is_none());
        remember_fingerprint(&cache, &file, "AA", 5);
        save_fingerprints(&cache);
        assert_eq!(cached_fingerprint(&cache, &file), Some(("AA".into(), 5)));
        std::fs::write(&file, b"changed!").unwrap();
        assert!(cached_fingerprint(&cache, &file).is_none());
        assert!(cache.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn already_packed_files_are_not_compressed() {
        assert!(!worth_compressing(Path::new("a.zip"), 1 << 20));
        assert!(!worth_compressing(Path::new("a.exe"), 100));
        assert!(worth_compressing(Path::new("a.exe"), 1 << 20));
        assert!(compress(&noise(100_000, 9)).is_none());
    }

    #[tokio::test]
    async fn a_dropped_connection_is_reopened_and_the_upload_retried() {
        let dir = temp("drop");
        let file = dir.join("setup.exe");
        std::fs::write(&file, noise(300_000, 3)).unwrap();
        let (server, log) = fake(Fake {
            drop_uploads: 1,
            ..Fake::default()
        })
        .await;
        let summary = run(&server, std::slice::from_ref(&file), 15).await;
        assert_eq!(summary.state, "clean");
        assert_eq!(log.drops.load(Ordering::Relaxed), 1);
        assert!(log.connections.load(Ordering::Relaxed) >= 2);
        assert_eq!(log.uploaded.lock().unwrap().len(), 300_000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn gives_up_after_three_failed_uploads() {
        let dir = temp("drops");
        let file = dir.join("setup.exe");
        std::fs::write(&file, noise(10_000, 4)).unwrap();
        let (server, log) = fake(Fake {
            drop_uploads: 10,
            ..Fake::default()
        })
        .await;
        let summary = run(&server, std::slice::from_ref(&file), 15).await;
        assert_eq!(summary.state, "error");
        assert_eq!(log.drops.load(Ordering::Relaxed), MAX_ATTEMPTS as usize);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn keeps_a_slow_analysis_alive_with_pings() {
        let dir = temp("slow");
        let file = dir.join("setup.exe");
        std::fs::write(&file, b"slow to analyze").unwrap();
        let (server, log) = fake(Fake {
            result_delay: Duration::from_millis(1200),
            ..Fake::default()
        })
        .await;
        let tune = Tuning {
            keepalive: Duration::from_millis(150),
            silent: Duration::from_millis(500),
            ..quick()
        };
        let (summary, _, _) = run_with(
            &server,
            std::slice::from_ref(&file),
            15,
            &AtomicBool::new(false),
            &tune,
        )
        .await;
        assert_eq!(summary.state, "clean");
        assert!(log.pings.load(Ordering::Relaxed) >= 3);
        assert_eq!(log.connections.load(Ordering::Relaxed), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn scans_many_files_over_several_connections() {
        let dir = temp("many");
        let files: Vec<PathBuf> = (0..40)
            .map(|i| {
                let f = dir.join(format!("f{i}.exe"));
                std::fs::write(&f, noise(5000 + i, i as u64 + 10)).unwrap();
                f
            })
            .collect();
        let (server, log) = fake(Fake {
            pipeline: 4,
            result_delay: Duration::from_millis(50),
            ..Fake::default()
        })
        .await;
        let started = Instant::now();
        let (summary, verdicts, _) =
            run_with(&server, &files, 15, &AtomicBool::new(false), &quick()).await;
        assert_eq!(summary.state, "clean");
        assert_eq!(verdicts.len(), 40);
        assert!(log.connections.load(Ordering::Relaxed) >= 2);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_stuck_analysis_does_not_hold_up_the_scan() {
        let dir = temp("stuck");
        let file = dir.join("setup.exe");
        std::fs::write(&file, b"never analyzed").unwrap();
        let other = dir.join("held.exe");
        std::fs::write(&other, b"held forever").unwrap();
        let (server, _log) = fake(Fake {
            result_delay: Duration::from_secs(3600),
            hold_first: true,
            keep_held: true,
            ..Fake::default()
        })
        .await;
        let tune = Tuning {
            result_timeout: Duration::from_millis(400),
            held_timeout: Duration::from_millis(400),
            ..quick()
        };
        let started = Instant::now();
        let files = vec![other, file];
        let (_, verdicts, _) = run_with(&server, &files, 15, &AtomicBool::new(false), &tune).await;
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(verdicts.len(), 2);
        assert!(
            verdicts[0].1.detail.contains("still analyzing"),
            "{}",
            verdicts[0].1.detail
        );
        assert!(
            verdicts[1].1.detail.contains("scan it again later"),
            "{}",
            verdicts[1].1.detail
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_unreachable_server_fails_soon() {
        let dir = temp("down");
        let file = dir.join("setup.exe");
        std::fs::write(&file, b"x").unwrap();
        let free = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = format!("ws://{}/ws", free.local_addr().unwrap());
        drop(free);
        let started = Instant::now();
        let summary = run(&server, std::slice::from_ref(&file), 15).await;
        assert_eq!(summary.state, "error");
        assert!(
            summary.detail.contains("could not reach"),
            "{}",
            summary.detail
        );
        assert!(started.elapsed() < Duration::from_secs(10));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_rejected_connection_is_not_retried() {
        let dir = temp("reject");
        let file = dir.join("setup.exe");
        std::fs::write(&file, b"x").unwrap();
        let (server, log) = fake(Fake {
            reject: true,
            ..Fake::default()
        })
        .await;
        let summary = run(&server, std::slice::from_ref(&file), 15).await;
        assert_eq!(summary.state, "error");
        assert!(summary.detail.contains("bad token"));
        assert_eq!(log.connections.load(Ordering::Relaxed), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn understands_every_server_verdict() {
        assert_eq!(normalize_verdict("CLEAN"), "clean");
        assert_eq!(normalize_verdict("xorfilter_clean"), "possible_clean");
        assert_eq!(normalize_verdict("xorfilter_malicious"), "malicious");
        assert_eq!(normalize_verdict("rejected"), "error");
        assert_eq!(normalize_verdict("unknown"), "unknown");
        assert_eq!(normalize_verdict(""), "unknown");
        let reply = json!({ "type": "result", "id": 1, "antivirus": { "verdict": "unknown", "detail": "Certificate: issued by Sectigo" } });
        let v = verdict_of(&reply);
        assert_eq!(v.verdict, "unknown");
        let summary = summarize(&[(PathBuf::from("/d/setup.exe"), v)], 15);
        assert_eq!(summary.state, "unknown");
        assert!(summary.detail.contains("no threat found"));
    }

    #[test]
    fn files_that_were_checked_count_even_when_others_were_not() {
        let v = |verdict: &str, detail: &str| FileVerdict::new(verdict, detail);
        let mixed = summarize(
            &[
                (PathBuf::from("/d/setup.exe"), v("clean", "")),
                (
                    PathBuf::from("/d/data.bin"),
                    v("skipped", "larger than the 100 MB upload limit"),
                ),
            ],
            100,
        );
        assert_eq!(mixed.state, "partial");
        assert!(
            mixed.detail.contains("no threat found in 1 of 2 files"),
            "{}",
            mixed.detail
        );
        assert!(mixed.detail.contains("100 MB"), "{}", mixed.detail);
        let failed = summarize(
            &[
                (PathBuf::from("/d/a.exe"), v("unknown", "")),
                (
                    PathBuf::from("/d/b.exe"),
                    v("error", "the cloud server did not answer in time"),
                ),
            ],
            100,
        );
        assert_eq!(failed.state, "partial");
        assert!(
            failed
                .detail
                .contains("b.exe: the cloud server did not answer in time"),
            "{}",
            failed.detail
        );
        let only_failed = summarize(&[(PathBuf::from("/d/b.exe"), v("error", "timeout"))], 100);
        assert_eq!(only_failed.state, "error");
        let threat = summarize(
            &[
                (PathBuf::from("/d/a.exe"), v("malicious", "Trojan")),
                (PathBuf::from("/d/b.exe"), v("error", "timeout")),
            ],
            100,
        );
        assert_eq!(threat.state, "threat");
    }

    #[test]
    fn long_threat_lists_are_shortened() {
        let verdicts: Vec<(PathBuf, FileVerdict)> = (0..8)
            .map(|i| {
                (
                    PathBuf::from(format!("/d/f{i}.exe")),
                    FileVerdict::new("malicious", "Trojan"),
                )
            })
            .collect();
        let summary = summarize(&verdicts, 15);
        assert_eq!(summary.state, "threat");
        assert!(summary.detail.contains("f4.exe"));
        assert!(!summary.detail.contains("f5.exe"));
        assert!(summary.detail.ends_with("and 3 more"));
    }

    #[test]
    fn hides_the_user_name_in_folders() {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap();
        let path = PathBuf::from(&home).join("Downloads").join("a.exe");
        let folder = normalize_folder(&path);
        assert!(folder.starts_with("%USERPROFILE%"), "{folder}");
        assert!(!folder.contains(&home));
        let other = normalize_folder(Path::new("/srv/private/team/a.exe"));
        assert_eq!(other, "team");
    }

    #[test]
    fn plain_websocket_is_only_allowed_on_this_computer() {
        let local = |s: &str| is_local(&url::Url::parse(s).unwrap());
        assert!(local("ws://127.0.0.1:5307/ws"));
        assert!(local("ws://localhost:5306/ws"));
        assert!(local("ws://[::1]:5306/ws"));
        assert!(!local("ws://example.org/ws"));
        assert!(!local("ws://10.0.0.5/ws"));
    }

    #[test]
    fn rejects_plain_websocket_servers() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let Err(err) = rt.block_on(connect("ws://example.org/ws")) else {
            panic!("plain ws:// to another computer was accepted");
        };
        assert!(err.to_string().contains("wss://"));
    }

    fn noise(len: usize, seed: u64) -> Vec<u8> {
        let mut x = seed | 1;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect()
    }

    #[tokio::test]
    #[ignore]
    async fn live_rescan_after_a_stopped_upload() {
        let dir = temp("live");
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        let files: Vec<PathBuf> = [(0usize, 20 * 1024 * 1024), (1, 3000), (2, 5000)]
            .iter()
            .map(|(i, len)| {
                let f = dir.join(format!("probe{i}.bin"));
                std::fs::write(&f, noise(*len, seed + *i as u64)).unwrap();
                f
            })
            .collect();
        let stop = AtomicBool::new(false);
        let start = Instant::now();
        let first = scan(
            &files[..1],
            100,
            &Options::default(),
            &stop,
            |_, s| {
                if let Stage::Uploading { sent, .. } = s {
                    if sent > 4 * 1024 * 1024 {
                        stop.store(true, Ordering::Relaxed);
                    }
                }
            },
            |_, _| {},
        )
        .await;
        println!(
            "first scan stopped after {:?}: {:?}",
            start.elapsed(),
            first
        );
        let start = Instant::now();
        let again = AtomicBool::new(false);
        let mut last = String::new();
        let second = scan(
            &files,
            100,
            &Options::default(),
            &again,
            |i, s| {
                let now = format!("{i} {s:?}");
                let kind = now.split([' ', '{']).take(2).collect::<String>();
                if kind != last {
                    println!("{:>7.1?} stage {now}", start.elapsed());
                    last = kind;
                }
            },
            |i, v| {
                println!(
                    "{:>7.1?} file {i}: {} {} via {}",
                    start.elapsed(),
                    v.verdict,
                    v.detail,
                    v.route
                )
            },
        )
        .await;
        println!("second scan took {:?}: {:?}", start.elapsed(), second);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    #[ignore]
    async fn live_many_files_then_rescan() {
        let dir = temp("many");
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        let files: Vec<PathBuf> = (0..120)
            .map(|i| {
                let f = dir.join(format!("file{i}.bin"));
                std::fs::write(&f, noise(2000 + i, seed + i as u64)).unwrap();
                f
            })
            .collect();
        for round in 1..=2 {
            let start = Instant::now();
            let mut checks = 0;
            let mut routes: HashMap<&'static str, usize> = HashMap::new();
            let summary = scan(
                &files,
                100,
                &Options::default(),
                &AtomicBool::new(false),
                |_, s| {
                    if let Stage::Checking { count } = s {
                        checks += 1;
                        println!(
                            "{:>7.1?} round {round}: checking {count} fingerprints",
                            start.elapsed()
                        );
                    }
                },
                |_, v| *routes.entry(v.route).or_default() += 1,
            )
            .await;
            println!(
                "round {round}: {:?}, {checks} check messages, routes {routes:?}, {summary:?}",
                start.elapsed()
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    #[ignore]
    async fn live_test_folder() {
        let folder = PathBuf::from(std::env::var("MULTRON_TEST_DIR").expect("MULTRON_TEST_DIR"));
        let mut files: Vec<PathBuf> = std::fs::read_dir(&folder)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && p.extension().is_none_or(|e| e != "json"))
            .collect();
        files.sort();
        let permit = CONNECTIONS.acquire().await.unwrap();
        let s = Session::open(&server(), permit).await.unwrap();
        println!(
            "server: check={} brotli={} batch={} pipeline={} rate={:.0}",
            s.check, s.brotli, s.batch, s.pipeline, s.rate
        );
        s.close().await;
        let options = Options {
            cache: Some(folder.join("fingerprints.json")),
            ..Options::default()
        };
        for round in 1..=2 {
            let start = Instant::now();
            let mut seen: HashMap<usize, &'static str> = HashMap::new();
            let summary = scan(
                &files,
                100,
                &options,
                &AtomicBool::new(false),
                |i, st| {
                    let kind = match st {
                        Stage::Hashing { .. } => "hashing",
                        Stage::Checking { .. } => "checking",
                        Stage::Waiting { .. } => "waiting",
                        Stage::Uploading { .. } => "uploading",
                        Stage::Analyzing => "analyzing",
                        Stage::Reconnecting => "RECONNECTING",
                    };
                    if seen.insert(i, kind) != Some(kind)
                        && matches!(kind, "uploading" | "analyzing" | "waiting" | "RECONNECTING")
                    {
                        println!(
                            "{:>7.1?} [{round}] {} {kind}",
                            start.elapsed(),
                            files[i].file_name().unwrap_or_default().to_string_lossy()
                        );
                    }
                },
                |i, v| {
                    println!(
                        "{:>7.1?} [{round}] {}: {} via {} ({} bytes, {} sent)",
                        start.elapsed(),
                        files[i].file_name().unwrap_or_default().to_string_lossy(),
                        v.verdict,
                        v.route,
                        v.size,
                        v.uploaded
                    )
                },
            )
            .await;
            println!(
                "[{round}] finished in {:?}: {} - {}",
                start.elapsed(),
                summary.state,
                summary.detail
            );
        }
    }

    #[tokio::test]
    #[ignore]
    async fn live_check_latency() {
        for round in 0..3u64 {
            let permit = CONNECTIONS.acquire().await.unwrap();
            let started = Instant::now();
            let mut s = Session::open(&server(), permit).await.unwrap();
            let opened = started.elapsed();
            let items: Vec<Value> = (0..3u64)
                .map(|i| json!({ "id": i + 1, "name": "probe.bin", "size": 10, "sha256": hex::encode_upper(Sha256::digest(noise(64, round * 10 + i + 999))), "folder": "" }))
                .collect();
            let sent = Instant::now();
            s.send(json!({ "type": "check", "items": items }))
                .await
                .unwrap();
            let mut times = Vec::new();
            for _ in 0..3 {
                let m = s
                    .next_answer(&AtomicBool::new(false), &Tuning::default())
                    .await
                    .unwrap();
                times.push(format!(
                    "{} {:?}",
                    m["type"].as_str().unwrap_or(""),
                    sent.elapsed()
                ));
            }
            println!("connect {opened:?}; answers: {}", times.join(", "));
            s.close().await;
        }
    }
}
