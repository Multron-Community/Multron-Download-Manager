pub mod limiter;

use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use parking_lot::Mutex;
use reqwest::header::{
    HeaderMap, CONTENT_DISPOSITION, CONTENT_RANGE, CONTENT_TYPE, ETAG, IF_RANGE, LAST_MODIFIED,
    RANGE,
};
use reqwest::{Client, StatusCode};
use tokio::io::AsyncWriteExt;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::model::SegmentState;
use crate::util;
use limiter::Limiter;

const MIN_SPLIT: u64 = 256 * 1024;
const MIN_STEAL: u64 = 64 * 1024;
const RATE_AFTER: f64 = 1.0;
const MIN_GAIN_SECS: f64 = 0.5;
const STALL_AFTER: Duration = Duration::from_secs(3);
const MIN_INITIAL_SEGMENT: u64 = 1024 * 1024;
const FLUSH_AT: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug)]
struct Seg {
    start: u64,
    end: u64,
    pos: u64,
    written: u64,
    busy: bool,
    since: Option<(Instant, u64)>,
}

fn seg_rate(s: &Seg, now: Instant, after: f64) -> Option<f64> {
    let (at, from) = s.since?;
    let secs = now.duration_since(at).as_secs_f64();
    (secs >= after).then(|| s.pos.saturating_sub(from) as f64 / secs)
}

fn split_plan(segs: &[Seg], now: Instant, thief: Option<f64>) -> Option<(usize, u64)> {
    let measured: Vec<f64> = segs
        .iter()
        .filter(|s| s.busy)
        .filter_map(|s| seg_rate(s, now, RATE_AFTER))
        .collect();
    let typical =
        (!measured.is_empty()).then(|| measured.iter().sum::<f64>() / measured.len() as f64);
    let taker = thief.or(typical);
    segs.iter()
        .enumerate()
        .filter(|(_, s)| s.busy)
        .filter_map(|(i, s)| {
            let remaining = s.end.saturating_sub(s.pos);
            let own = seg_rate(s, now, RATE_AFTER).or(typical);
            let keep = match (own, taker) {
                (Some(v), Some(t)) if v + t > 0.0 => {
                    if remaining < 2 * MIN_STEAL || remaining as f64 / v.max(1.0) < MIN_GAIN_SECS {
                        return None;
                    }
                    ((remaining as f64 * v / (v + t)) as u64)
                        .clamp(MIN_STEAL, remaining - MIN_STEAL)
                }
                _ if remaining >= 2 * MIN_SPLIT => remaining / 2,
                _ => return None,
            };
            let eta = remaining as f64 / own.unwrap_or(1.0).max(1.0);
            Some((i, s.pos + keep, eta))
        })
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(i, at, _)| (i, at))
}

pub struct Progress {
    segs: Mutex<Vec<Seg>>,
    total: AtomicU64,
    pub received: AtomicU64,
    single_pos: AtomicU64,
    pub connections: AtomicU32,
    pub target: AtomicU32,
    retire: AtomicU32,
}

impl Progress {
    pub fn new(total: Option<u64>, saved: &[SegmentState], threads: u32) -> Arc<Self> {
        let segs = saved
            .iter()
            .filter(|s| s.end > s.start)
            .map(|s| Seg {
                start: s.start,
                end: s.end,
                pos: s.pos.clamp(s.start, s.end),
                written: s.pos.clamp(s.start, s.end),
                busy: false,
                since: None,
            })
            .collect();
        Arc::new(Self {
            segs: Mutex::new(segs),
            total: AtomicU64::new(total.unwrap_or(0)),
            received: AtomicU64::new(0),
            single_pos: AtomicU64::new(0),
            connections: AtomicU32::new(0),
            target: AtomicU32::new(threads.max(1)),
            retire: AtomicU32::new(0),
        })
    }

    pub fn total(&self) -> Option<u64> {
        match self.total.load(Ordering::Relaxed) {
            0 => None,
            t => Some(t),
        }
    }

    pub fn set_target(&self, threads: u32) {
        self.target.store(
            threads.clamp(1, crate::settings::MAX_THREADS),
            Ordering::Relaxed,
        );
    }

    pub fn downloaded(&self) -> u64 {
        let segs = self.segs.lock();
        if segs.is_empty() {
            return self.single_pos.load(Ordering::Relaxed);
        }
        segs.iter().map(|s| s.pos - s.start).sum()
    }

    pub fn segments(&self) -> Vec<SegmentState> {
        let mut segs: Vec<SegmentState> = self
            .segs
            .lock()
            .iter()
            .map(|s| SegmentState {
                start: s.start,
                end: s.end,
                pos: s.written,
            })
            .collect();
        segs.sort_by_key(|s| s.start);
        segs
    }

    fn init_segments(&self, total: u64) {
        let mut segs = self.segs.lock();
        let covered: u64 = segs.iter().map(|s| s.end - s.start).sum();
        if !segs.is_empty() && covered == total {
            return;
        }
        segs.clear();
        let wanted = self.target.load(Ordering::Relaxed).max(1) as u64;
        let count = wanted.min((total / MIN_INITIAL_SEGMENT).max(1));
        let size = total / count;
        for i in 0..count {
            let start = i * size;
            let end = if i == count - 1 { total } else { start + size };
            segs.push(Seg {
                start,
                end,
                pos: start,
                written: start,
                busy: false,
                since: None,
            });
        }
    }

    fn is_complete(&self) -> bool {
        self.segs
            .lock()
            .iter()
            .all(|s| s.written >= s.end && s.pos >= s.end)
    }

    fn has_work(&self) -> bool {
        let segs = self.segs.lock();
        segs.iter().any(|s| !s.busy && s.pos < s.end)
            || split_plan(&segs, Instant::now(), None).is_some()
    }

    fn claim(&self, speed: Option<f64>) -> Option<usize> {
        let mut segs = self.segs.lock();
        let now = Instant::now();
        if let Some(i) = segs.iter().position(|s| !s.busy && s.pos < s.end) {
            segs[i].busy = true;
            segs[i].since = Some((now, segs[i].pos));
            return Some(i);
        }
        let (victim, at) = split_plan(&segs, now, speed)?;
        let end = segs[victim].end;
        segs[victim].end = at;
        segs.push(Seg {
            start: at,
            end,
            pos: at,
            written: at,
            busy: true,
            since: Some((now, at)),
        });
        Some(segs.len() - 1)
    }

    fn release(&self, idx: usize) -> Option<f64> {
        let mut segs = self.segs.lock();
        let s = &mut segs[idx];
        let speed = seg_rate(s, Instant::now(), MIN_GAIN_SECS);
        s.busy = false;
        s.since = None;
        s.pos = s.written;
        speed
    }

    fn should_retire(&self) -> bool {
        let mut current = self.retire.load(Ordering::Relaxed);
        while current > 0 {
            match self.retire.compare_exchange_weak(
                current,
                current - 1,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
        false
    }
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub final_url: String,
    pub total: Option<u64>,
    pub resumable: bool,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub validator: Option<String>,
}

fn validator(headers: &HeaderMap) -> Option<String> {
    let get = |name| {
        headers
            .get(name)
            .and_then(|v: &reqwest::header::HeaderValue| v.to_str().ok())
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    get(ETAG)
        .filter(|e| !e.starts_with("W/"))
        .or_else(|| get(LAST_MODIFIED))
        .map(str::to_string)
}

pub async fn probe(client: &Client, url: &str, headers: &HeaderMap) -> Result<ProbeResult> {
    let resp = client
        .get(url)
        .headers(headers.clone())
        .header(RANGE, "bytes=0-0")
        .send()
        .await
        .with_context(|| format!("could not connect to {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        bail!("server answered HTTP {status}");
    }
    let final_url = resp.url().to_string();
    let validator = validator(resp.headers());
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string());
    let mut filename = resp
        .headers()
        .get(CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(util::filename_from_disposition)
        .or_else(|| util::filename_from_url(&final_url))
        .or_else(|| util::filename_from_url(url));

    let (total, resumable) = if status == StatusCode::PARTIAL_CONTENT {
        let total = resp
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.trim().parse::<u64>().ok());
        (total, total.is_some())
    } else {
        (resp.content_length().filter(|l| *l > 0), false)
    };

    if let (Some(name), Some(ct)) = (filename.as_mut(), content_type.as_deref()) {
        if util::extension(name).is_empty() {
            if let Some(ext) = mime_guess::get_mime_extensions_str(ct).and_then(|e| e.first()) {
                name.push('.');
                name.push_str(ext);
            }
        }
    }
    drop(resp);
    Ok(ProbeResult {
        final_url,
        total,
        resumable,
        filename,
        content_type,
        validator,
    })
}

pub trait ChunkTransform: Send + Sync {
    fn apply(&self, data: &mut [u8], offset: u64);
}

pub const APP_USER_AGENT: &str = concat!("Multron-Download-Manager/", env!("CARGO_PKG_VERSION"));

pub async fn probe_with_fallback(
    client: &Client,
    url: &str,
    headers: &mut HeaderMap,
) -> Result<ProbeResult> {
    match probe(client, url, headers).await {
        Ok(p) => Ok(p),
        Err(first) => {
            let mut alt = headers.clone();
            alt.insert(
                reqwest::header::USER_AGENT,
                reqwest::header::HeaderValue::from_static(APP_USER_AGENT),
            );
            match probe(client, url, &alt).await {
                Ok(p) => {
                    tracing::info!("{url} only accepts non-browser User-Agents");
                    *headers = alt;
                    Ok(p)
                }
                Err(_) => Err(first),
            }
        }
    }
}

pub struct Job {
    pub client: Client,
    pub url: String,
    pub headers: HeaderMap,
    pub part: PathBuf,
    pub resumable: bool,
    pub progress: Arc<Progress>,
    pub cancel: CancellationToken,
    pub limiters: Vec<Arc<Limiter>>,
    pub retries: u32,
    pub transform: Option<Arc<dyn ChunkTransform>>,
    pub if_range: Option<String>,
    pub max_request: Option<u64>,
}

impl Job {
    async fn throttle(&self, n: usize) {
        for l in &self.limiters {
            l.acquire(n).await;
        }
    }
}

pub fn ensure_space(part: &std::path::Path, total: u64) -> Result<()> {
    let have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    let needed = total.saturating_sub(have);
    let dir = part.parent().unwrap_or(std::path::Path::new("."));
    if let Ok(free) = fs4::available_space(dir) {
        if needed > free {
            bail!(
                "not enough disk space: {} MB needed, {} MB free on this drive",
                needed.div_ceil(1 << 20),
                free >> 20
            );
        }
    }
    Ok(())
}

fn preallocate(f: &File, total: u64) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        util::make_sparse(f);
        f.set_len(total)
    }
    #[cfg(not(windows))]
    {
        if fs4::fs_std::FileExt::allocate(f, total).is_err() {
            f.set_len(total)?;
        }
        if f.metadata()?.len() != total {
            f.set_len(total)?;
        }
        Ok(())
    }
}

pub async fn run(job: Arc<Job>) -> Result<bool> {
    if let Some(total) = job.progress.total() {
        let part = job.part.clone();
        tokio::task::spawn_blocking(move || ensure_space(&part, total)).await??;
    }
    let result = match (job.resumable, job.progress.total()) {
        (true, Some(total)) => run_segmented(job.clone(), total).await,
        _ => run_single(job.clone()).await,
    };
    job.progress.connections.store(0, Ordering::Relaxed);
    result
}

async fn run_single(job: Arc<Job>) -> Result<bool> {
    let mut attempt = 0;
    loop {
        match single_attempt(&job).await {
            Ok(done) => return Ok(done),
            Err(e) if attempt < job.retries && !job.cancel.is_cancelled() => {
                attempt += 1;
                tracing::warn!("single-stream download failed (attempt {attempt}): {e:#}");
                let backoff = Duration::from_millis(500 * 2u64.pow(attempt.min(5)));
                tokio::select! {
                    _ = job.cancel.cancelled() => return Ok(false),
                    _ = tokio::time::sleep(backoff) => {}
                }
            }
            Err(e) => return Err(e),
        }
    }
}

async fn single_attempt(job: &Job) -> Result<bool> {
    job.progress.single_pos.store(0, Ordering::Relaxed);
    let resp = tokio::select! {
        _ = job.cancel.cancelled() => return Ok(false),
        r = job.client.get(&job.url).headers(job.headers.clone()).send() => r?,
    };
    let resp = resp.error_for_status()?;
    if job.progress.total().is_none() {
        if let Some(len) = resp.content_length() {
            job.progress.total.store(len, Ordering::Relaxed);
        }
    }
    let file = tokio::fs::File::create(&job.part)
        .await
        .with_context(|| format!("could not create {}", job.part.display()))?;
    let mut writer = tokio::io::BufWriter::with_capacity(FLUSH_AT, file);
    let mut stream = resp.bytes_stream();
    job.progress.connections.store(1, Ordering::Relaxed);
    loop {
        let next = tokio::select! {
            _ = job.cancel.cancelled() => {
                writer.flush().await?;
                return Ok(false);
            }
            n = stream.next() => n,
        };
        let Some(chunk) = next else { break };
        let chunk = chunk.context("connection interrupted")?;
        job.throttle(chunk.len()).await;
        if let Some(t) = &job.transform {
            let mut data = chunk.to_vec();
            t.apply(&mut data, job.progress.single_pos.load(Ordering::Relaxed));
            writer.write_all(&data).await?;
        } else {
            writer.write_all(&chunk).await?;
        }
        job.progress
            .single_pos
            .fetch_add(chunk.len() as u64, Ordering::Relaxed);
        job.progress
            .received
            .fetch_add(chunk.len() as u64, Ordering::Relaxed);
    }
    writer.flush().await?;
    writer.into_inner().sync_all().await?;
    let got = job.progress.single_pos.load(Ordering::Relaxed);
    if let Some(total) = job.progress.total() {
        if got < total {
            bail!("connection closed after {got} of {total} bytes");
        }
    }
    Ok(true)
}

async fn run_segmented(job: Arc<Job>, total: u64) -> Result<bool> {
    let part = job.part.clone();
    let file = tokio::task::spawn_blocking(move || -> std::io::Result<File> {
        let f = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&part)?;
        if f.metadata()?.len() != total {
            preallocate(&f, total)?;
        }
        Ok(f)
    })
    .await?
    .with_context(|| format!("could not open {}", job.part.display()))?;
    let file = Arc::new(file);
    job.progress.init_segments(total);

    let workers_cancel = job.cancel.child_token();
    let mut set: JoinSet<Result<()>> = JoinSet::new();
    let mut failures = 0u32;
    let mut last_error: Option<anyhow::Error> = None;
    let mut tick = tokio::time::interval(Duration::from_millis(200));

    loop {
        let target = job.progress.target.load(Ordering::Relaxed).max(1);
        let fail_limit = (target * 2).max(4);
        let mut spawned = 0;
        while (set.len() as u32) < target
            && spawned < target
            && failures < fail_limit
            && job.progress.has_work()
        {
            set.spawn(worker(job.clone(), file.clone(), workers_cancel.clone()));
            spawned += 1;
        }
        let running = set.len() as u32;
        if running > target && job.progress.retire.load(Ordering::Relaxed) == 0 {
            job.progress
                .retire
                .store(running - target, Ordering::Relaxed);
        }
        job.progress.connections.store(running, Ordering::Relaxed);

        if set.is_empty() {
            if job.progress.is_complete() {
                break;
            }
            if job.cancel.is_cancelled() {
                return Ok(false);
            }
            return Err(last_error.unwrap_or_else(|| anyhow!("download stalled")));
        }

        tokio::select! {
            _ = job.cancel.cancelled() => {
                while set.join_next().await.is_some() {}
                let f = file.clone();
                let _ = tokio::task::spawn_blocking(move || f.sync_data()).await;
                return Ok(false);
            }
            Some(res) = set.join_next() => {
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        tracing::warn!("connection gave up: {e:#}");
                        failures += 1;
                        last_error = Some(e);
                    }
                    Err(join) => {
                        failures += 1;
                        last_error = Some(anyhow!("worker crashed: {join}"));
                    }
                }
            }
            _ = tick.tick() => {}
        }
    }

    workers_cancel.cancel();
    let f = file.clone();
    tokio::task::spawn_blocking(move || f.sync_all()).await??;
    Ok(true)
}

enum SegEnd {
    Done,
    More,
    Cancelled,
    Retired,
}

async fn worker(job: Arc<Job>, file: Arc<File>, cancel: CancellationToken) -> Result<()> {
    let mut errors = 0u32;
    let mut speed = None;
    loop {
        if cancel.is_cancelled() {
            return Ok(());
        }
        let Some(idx) = job.progress.claim(speed) else {
            return Ok(());
        };
        let outcome = loop {
            let outcome = fetch_segment(&job, &file, idx, &cancel).await;
            if matches!(outcome, Ok(SegEnd::More)) && !cancel.is_cancelled() {
                continue;
            }
            break outcome;
        };
        speed = job.progress.release(idx).or(speed);
        match outcome {
            Ok(SegEnd::Done) | Ok(SegEnd::More) => errors = 0,
            Ok(SegEnd::Cancelled) | Ok(SegEnd::Retired) => return Ok(()),
            Err(e) => {
                errors += 1;
                if errors > job.retries {
                    return Err(e);
                }
                tracing::debug!("segment error (retry {errors}): {e:#}");
                let backoff = Duration::from_millis(400 * 2u64.pow(errors.min(5)));
                tokio::select! {
                    _ = cancel.cancelled() => return Ok(()),
                    _ = tokio::time::sleep(backoff) => {}
                }
            }
        }
    }
}

struct SegWriter<'a> {
    job: &'a Job,
    file: &'a File,
    idx: usize,
    buf: Vec<u8>,
    offset: u64,
}

impl SegWriter<'_> {
    fn flush(&mut self) -> std::io::Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        tokio::task::block_in_place(|| util::write_all_at(self.file, &self.buf, self.offset))?;
        self.offset += self.buf.len() as u64;
        self.buf.clear();
        self.job.progress.segs.lock()[self.idx].written = self.offset;
        Ok(())
    }
}

async fn fetch_segment(
    job: &Job,
    file: &File,
    idx: usize,
    cancel: &CancellationToken,
) -> Result<SegEnd> {
    let (start, end) = {
        let segs = job.progress.segs.lock();
        (segs[idx].pos, segs[idx].end)
    };
    if start >= end {
        return Ok(SegEnd::Done);
    }
    let request_end = job
        .max_request
        .map_or(end, |m| end.min(start + m.max(MIN_STEAL)));
    let mut request = job
        .client
        .get(&job.url)
        .headers(job.headers.clone())
        .header(RANGE, format!("bytes={}-{}", start, request_end - 1));
    if let Some(v) = &job.if_range {
        request = request.header(IF_RANGE, v.as_str());
    }
    let request = request.send();
    let resp = tokio::select! {
        _ = cancel.cancelled() => return Ok(SegEnd::Cancelled),
        r = request => r?,
    };
    let status = resp.status();
    if status != StatusCode::PARTIAL_CONTENT {
        if status.is_success() {
            if job.if_range.is_some() {
                bail!("the file changed on the server; restart the download");
            }
            bail!("server ignored the range request (HTTP {status})");
        }
        bail!("HTTP {status}");
    }
    let served = resp
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_content_range);
    match (served, job.progress.total()) {
        (Some((s, _, _)), _) if s != start => {
            bail!("server sent bytes from {s} instead of {start}")
        }
        (Some((_, _, Some(t))), Some(total)) if t != total => {
            bail!("the file size changed on the server ({total} → {t} bytes)")
        }
        _ => {}
    }

    let mut writer = SegWriter {
        job,
        file,
        idx,
        buf: Vec::with_capacity(FLUSH_AT + 64 * 1024),
        offset: start,
    };
    let mut stream = resp.bytes_stream();
    let result: Result<SegEnd> = async {
        let mut quiet_since = Instant::now();
        let mut others = job.progress.received.load(Ordering::Relaxed);
        loop {
            let item = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Ok(SegEnd::Cancelled),
                i = stream.next() => i,
                _ = tokio::time::sleep_until((quiet_since + STALL_AFTER).into()) => {
                    let now = job.progress.received.load(Ordering::Relaxed);
                    if now > others {
                        bail!("connection stalled while the others kept downloading");
                    }
                    quiet_since = Instant::now();
                    continue;
                }
            };
            let Some(chunk) = item else {
                let segs = job.progress.segs.lock();
                if segs[idx].pos >= segs[idx].end {
                    return Ok(SegEnd::Done);
                }
                if segs[idx].pos >= request_end {
                    return Ok(SegEnd::More);
                }
                bail!("connection closed early");
            };
            let chunk = chunk.context("connection interrupted")?;
            job.throttle(chunk.len()).await;
            let (take, chunk_offset, reached_end) = {
                let mut segs = job.progress.segs.lock();
                let seg = &mut segs[idx];
                let take = (chunk.len() as u64).min(seg.end.saturating_sub(seg.pos)) as usize;
                let offset = seg.pos;
                seg.pos += take as u64;
                (take, offset, seg.pos >= seg.end)
            };
            let appended_at = writer.buf.len();
            writer.buf.extend_from_slice(&chunk[..take]);
            if let Some(t) = &job.transform {
                t.apply(&mut writer.buf[appended_at..], chunk_offset);
            }
            others = job
                .progress
                .received
                .fetch_add(take as u64, Ordering::Relaxed)
                + take as u64;
            quiet_since = Instant::now();
            if writer.buf.len() >= FLUSH_AT || reached_end {
                writer.flush()?;
            }
            if reached_end {
                return Ok(SegEnd::Done);
            }
            if job.progress.should_retire() {
                return Ok(SegEnd::Retired);
            }
        }
    }
    .await;
    writer.flush()?;
    result
}

fn parse_content_range(v: &str) -> Option<(u64, u64, Option<u64>)> {
    let rest = v.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let (a, b) = range.trim().split_once('-')?;
    let total = match total.trim() {
        "*" => None,
        t => Some(t.parse().ok()?),
    };
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_range() {
        assert_eq!(
            parse_content_range("bytes 100-199/1000"),
            Some((100, 199, Some(1000)))
        );
        assert_eq!(parse_content_range("bytes 0-9/*"), Some((0, 9, None)));
        assert_eq!(parse_content_range("items 0-9/10"), None);
    }

    #[test]
    fn prefers_strong_etags() {
        let mut h = HeaderMap::new();
        h.insert(
            LAST_MODIFIED,
            "Wed, 21 Oct 2015 07:28:00 GMT".parse().unwrap(),
        );
        h.insert(ETAG, "W/\"weak\"".parse().unwrap());
        assert_eq!(
            validator(&h).as_deref(),
            Some("Wed, 21 Oct 2015 07:28:00 GMT")
        );
        h.insert(ETAG, "\"strong\"".parse().unwrap());
        assert_eq!(validator(&h).as_deref(), Some("\"strong\""));
    }

    async fn range_server(data: Arc<Vec<u8>>, etag: &'static str) -> String {
        use axum::http::{HeaderMap as H, StatusCode as S};
        let app = axum::Router::new().route(
            "/file.bin",
            axum::routing::get(move |headers: H| {
                let data = data.clone();
                async move {
                    let total = data.len() as u64;
                    let range = headers
                        .get("range")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.strip_prefix("bytes="))
                        .and_then(|v| v.split_once('-'))
                        .and_then(|(a, b)| {
                            Some((
                                a.parse::<u64>().ok()?,
                                b.parse::<u64>().unwrap_or(total - 1),
                            ))
                        });
                    let if_range_ok = headers
                        .get("if-range")
                        .map(|v| v.to_str().unwrap_or("") == etag)
                        .unwrap_or(true);
                    match range {
                        Some((a, b)) if if_range_ok => {
                            let b = b.min(total - 1);
                            (
                                S::PARTIAL_CONTENT,
                                [
                                    ("content-range", format!("bytes {a}-{b}/{total}")),
                                    ("etag", etag.to_string()),
                                ],
                                data[a as usize..=b as usize].to_vec(),
                            )
                        }
                        _ => (
                            S::OK,
                            [
                                ("content-length", total.to_string()),
                                ("etag", etag.to_string()),
                            ],
                            data.to_vec(),
                        ),
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}/file.bin")
    }

    fn local_job(url: &str, part: PathBuf, total: u64, if_range: Option<String>) -> Arc<Job> {
        Arc::new(Job {
            client: Client::new(),
            url: url.to_string(),
            headers: HeaderMap::new(),
            part,
            resumable: true,
            progress: Progress::new(Some(total), &[], 8),
            cancel: CancellationToken::new(),
            limiters: Vec::new(),
            retries: 0,
            transform: None,
            if_range,
            max_request: None,
        })
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn segmented_download_writes_every_byte_at_its_offset() {
        let data: Vec<u8> = (0..12 * 1024 * 1024u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
            .collect();
        let expected = {
            use sha2::Digest;
            hex::encode(sha2::Sha256::digest(&data))
        };
        let total = data.len() as u64;
        let url = range_server(Arc::new(data), "\"v1\"").await;
        let p = probe(&Client::new(), &url, &HeaderMap::new())
            .await
            .unwrap();
        assert!(p.resumable);
        assert_eq!(p.total, Some(total));
        assert_eq!(p.validator.as_deref(), Some("\"v1\""));

        let dir = std::env::temp_dir().join(format!("mdm-engine-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let part = dir.join("file.bin.part");
        let job = local_job(&url, part.clone(), total, p.validator.clone());
        assert!(run(job.clone()).await.unwrap());
        assert!(job.progress.segments().len() >= 8);
        assert_eq!(sha256(&part), expected);

        let changed = local_job(&url, dir.join("changed.part"), total, Some("\"v0\"".into()));
        let err = run(changed).await.unwrap_err().to_string();
        assert!(err.contains("changed on the server"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_downloads_bigger_than_the_drive() {
        let part = std::env::temp_dir().join("mdm-space-check.part");
        assert!(ensure_space(&part, 1).is_ok());
        assert!(ensure_space(&part, u64::MAX / 2).is_err());
    }

    #[test]
    fn claims_and_steals() {
        let p = Progress::new(Some(64 * 1024 * 1024), &[], 4);
        p.init_segments(64 * 1024 * 1024);
        assert_eq!(p.segs.lock().len(), 4);
        for _ in 0..4 {
            assert!(p.claim(None).is_some());
        }
        let stolen = p.claim(None).unwrap();
        let segs = p.segs.lock();
        assert_eq!(segs.len(), 5);
        assert_eq!(segs[stolen].end - segs[stolen].start, 8 * 1024 * 1024);
        let covered: u64 = segs.iter().map(|s| s.end - s.start).sum();
        assert_eq!(covered, 64 * 1024 * 1024);
    }

    #[test]
    fn slow_connections_hand_most_of_their_work_to_fast_ones() {
        let now = Instant::now();
        let started = now - Duration::from_secs(2);
        let seg = |start: u64, pos: u64, end: u64| Seg {
            start,
            end,
            pos,
            written: pos,
            busy: true,
            since: Some((started, start)),
        };
        let mb = 1024 * 1024;
        let segs = [
            seg(0, 4 * mb, 8 * mb),
            seg(8 * mb, 8 * mb + mb / 8, 16 * mb),
        ];
        let (victim, at) = split_plan(&segs, now, Some(4.0 * mb as f64)).unwrap();
        assert_eq!(victim, 1);
        let kept = at - segs[1].pos;
        assert!(kept < mb / 4, "slow connection kept {kept} bytes");

        let segs = [seg(0, 6 * mb, 8 * mb)];
        let (_, at) = split_plan(&segs, now, Some(64.0 * 1024.0)).unwrap();
        assert!(at - segs[0].pos > 2 * mb * 9 / 10);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_stalled_connection_does_not_hold_up_the_download() {
        let took = paced_download(4 << 20, |n| {
            if n == 2 {
                Pace::StallAfter(64 << 10)
            } else {
                Pace::Rate(0)
            }
        })
        .await;
        assert!(took < Duration::from_secs(15), "took {took:?}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn caps_each_request_and_continues_on_the_same_connection() {
        let data: Vec<u8> = (0..6 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
        let expected = {
            use sha2::Digest;
            hex::encode(sha2::Sha256::digest(&data))
        };
        let total = data.len() as u64;
        let url = range_server(Arc::new(data), "\"v1\"").await;
        let dir = std::env::temp_dir().join(format!("mdm-cap-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let part = dir.join("file.bin.part");
        let mut job = local_job(&url, part.clone(), total, None);
        Arc::get_mut(&mut job).unwrap().max_request = Some(512 * 1024);
        assert!(run(job).await.unwrap());
        assert_eq!(sha256(&part), expected);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn small_files_get_one_segment() {
        let p = Progress::new(Some(1000), &[], 8);
        p.init_segments(1000);
        assert_eq!(p.segs.lock().len(), 1);
        assert!(p.claim(None).is_some());
        assert!(p.claim(None).is_none());
    }

    #[derive(Clone, Copy)]
    enum Pace {
        Rate(u64),
        StallAfter(u64),
    }

    async fn paced_server(data: Arc<Vec<u8>>, pace: fn(usize) -> Pace) -> String {
        use axum::http::{HeaderMap as H, StatusCode as S};
        use std::sync::atomic::AtomicUsize;
        let requests = Arc::new(AtomicUsize::new(0));
        let app = axum::Router::new().route(
            "/file.bin",
            axum::routing::get(move |headers: H| {
                let data = data.clone();
                let n = requests.fetch_add(1, Ordering::SeqCst);
                async move {
                    let total = data.len() as u64;
                    let (a, b) = headers
                        .get("range")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.strip_prefix("bytes="))
                        .and_then(|v| v.split_once('-'))
                        .and_then(|(a, b)| {
                            Some((
                                a.parse::<u64>().ok()?,
                                b.parse::<u64>().unwrap_or(total - 1).min(total - 1),
                            ))
                        })
                        .unwrap_or((0, total - 1));
                    let pace = if n == 0 { Pace::Rate(0) } else { pace(n) };
                    const STEP: u64 = 16 * 1024;
                    let body = futures_util::stream::unfold(a, move |at| {
                        let data = data.clone();
                        async move {
                            if at > b {
                                return None;
                            }
                            let next = (at + STEP).min(b + 1);
                            match pace {
                                Pace::Rate(0) => {}
                                Pace::Rate(r) => {
                                    tokio::time::sleep(Duration::from_secs_f64(
                                        (next - at) as f64 / r as f64,
                                    ))
                                    .await
                                }
                                Pace::StallAfter(n) if at >= a + n => {
                                    std::future::pending::<()>().await
                                }
                                Pace::StallAfter(_) => {}
                            }
                            let chunk =
                                bytes::Bytes::copy_from_slice(&data[at as usize..next as usize]);
                            Some((Ok::<_, std::convert::Infallible>(chunk), next))
                        }
                    });
                    (
                        S::PARTIAL_CONTENT,
                        [
                            ("content-range", format!("bytes {a}-{b}/{total}")),
                            ("content-length", (b + 1 - a).to_string()),
                        ],
                        axum::body::Body::from_stream(body),
                    )
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}/file.bin")
    }

    async fn paced_download(total: usize, pace: fn(usize) -> Pace) -> Duration {
        let data: Vec<u8> = (0..total as u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 11) as u8)
            .collect();
        let expected = {
            use sha2::Digest;
            hex::encode(sha2::Sha256::digest(&data))
        };
        let url = paced_server(Arc::new(data), pace).await;
        let client = Client::builder()
            .http1_only()
            .read_timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let p = probe(&client, &url, &HeaderMap::new()).await.unwrap();
        let dir = std::env::temp_dir().join(format!("mdm-paced-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let part = dir.join("file.bin.part");
        let mut job = local_job(&url, part.clone(), p.total.unwrap(), None);
        Arc::get_mut(&mut job).unwrap().client = client;
        Arc::get_mut(&mut job).unwrap().retries = 4;
        let started = std::time::Instant::now();
        assert!(run(job).await.unwrap());
        let took = started.elapsed();
        assert_eq!(sha256(&part), expected);
        let _ = std::fs::remove_dir_all(&dir);
        took
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "timing benchmark"]
    async fn bench_paced_connections() {
        let slow = paced_download(16 << 20, |n| {
            Pace::Rate(if n % 4 == 1 { 64 << 10 } else { 2 << 20 })
        })
        .await;
        println!("one slow connection in four: {slow:?}");
        let stalled = paced_download(16 << 20, |n| {
            if n == 2 {
                Pace::StallAfter(64 << 10)
            } else {
                Pace::Rate(2 << 20)
            }
        })
        .await;
        println!("one stalled connection: {stalled:?}");
        let even = paced_download(16 << 20, |_| Pace::Rate(2 << 20)).await;
        println!("even connections: {even:?}");
    }

    fn sha256(path: &std::path::Path) -> String {
        use sha2::Digest;
        hex::encode(sha2::Sha256::digest(std::fs::read(path).unwrap()))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    #[ignore = "needs network access"]
    async fn engine_network_segmented_matches_single() {
        let url = std::env::var("MULTRON_TEST_URL")
            .unwrap_or_else(|_| "https://nbg1-speed.hetzner.com/100MB.bin".into());
        let client = reqwest::Client::builder()
            .http1_only()
            .user_agent(crate::settings::DEFAULT_USER_AGENT)
            .tcp_nodelay(true)
            .pool_max_idle_per_host(64)
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let mut headers = HeaderMap::new();
        let probe = probe_with_fallback(&client, &url, &mut headers)
            .await
            .unwrap();
        println!("probe: {probe:?}");
        let total = probe.total.expect("server must report a size");
        assert!(probe.resumable, "server must support ranges");

        let dir = std::env::temp_dir().join(format!("multron-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut results = Vec::new();
        for (threads, resumable) in [(8u32, true), (1u32, false)] {
            let part = dir.join(format!("t{threads}.part"));
            let progress = Progress::new(Some(total), &[], threads);
            let job = Arc::new(Job {
                client: client.clone(),
                url: probe.final_url.clone(),
                headers: headers.clone(),
                part: part.clone(),
                resumable,
                progress: progress.clone(),
                cancel: CancellationToken::new(),
                limiters: Vec::new(),
                retries: 4,
                transform: None,
                if_range: probe.validator.clone(),
                max_request: None,
            });
            let started = std::time::Instant::now();
            assert!(run(job).await.unwrap(), "download did not complete");
            let secs = started.elapsed().as_secs_f64();
            let len = std::fs::metadata(&part).unwrap().len();
            assert_eq!(len, total);
            println!(
                "{threads} connection(s): {:.1} MB in {secs:.2}s = {:.2} MB/s",
                total as f64 / 1048576.0,
                total as f64 / 1048576.0 / secs
            );
            results.push(sha256(&part));
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            results[0], results[1],
            "segmented and single downloads differ"
        );
        println!("sha256 {}", results[0]);
    }
}
