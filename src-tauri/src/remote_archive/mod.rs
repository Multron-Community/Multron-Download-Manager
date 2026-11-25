use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

use anyhow::{bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

pub mod decode;
pub mod http;
mod iso;
mod rar;
mod sevenz;
mod tar;
mod udf;
mod zip;

pub const MAX_ENTRIES: usize = 200_000;
pub const LIST_BUDGET: u64 = 96 * 1024 * 1024;
pub const LIST_REQUESTS: u32 = 6_000;
const BLOCK: u64 = 64 * 1024;
const CACHE_BLOCKS: usize = 1024;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    #[default]
    Stored,
    Deflate,
    Deflate64,
    SevenZip,
    Unsupported,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Entry {
    pub path: String,
    pub dir: bool,
    pub size: u64,
    pub modified: Option<i64>,
    pub method: Method,
    pub offset: u64,
    pub fetch: u64,
    pub crc: Option<u32>,
    pub local_header: bool,
    pub block: Option<u32>,
    pub index: Option<u32>,
    pub shared: u32,
    pub problem: Option<String>,
}

impl Entry {
    fn file(path: String, size: u64) -> Self {
        Entry {
            path,
            size,
            fetch: size,
            ..Default::default()
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Zip,
    Iso,
    Udf,
    Tar,
    Rar,
    SevenZip,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub format: Format,
    pub url: String,
    pub size: u64,
    pub entries: Vec<Entry>,
    pub truncated: bool,
    pub solid: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Pick {
    pub format: Format,
    pub archive_size: u64,
    pub entry: Entry,
}

pub trait Source: Send + Sync {
    fn size(&self) -> u64;
    fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>>;
}

pub fn read_exact_at(src: &dyn Source, offset: u64, len: usize) -> Result<Vec<u8>> {
    let data = src.read_at(offset, len)?;
    if data.len() < len {
        bail!("the archive ends early (wanted {len} bytes at {offset})");
    }
    Ok(data)
}

#[derive(Debug)]
pub struct OverBudget;

impl std::fmt::Display for OverBudget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the archive's file list is too large to read over the network; download the whole archive instead")
    }
}

impl std::error::Error for OverBudget {}

pub fn over_budget(e: &anyhow::Error) -> bool {
    e.downcast_ref::<OverBudget>().is_some()
}

pub struct Cached<'a> {
    inner: &'a dyn Source,
    blocks: Mutex<HashMap<u64, std::sync::Arc<Vec<u8>>>>,
    spent: Mutex<(u64, u32)>,
    budget: (u64, u32),
}

impl<'a> Cached<'a> {
    pub fn new(inner: &'a dyn Source) -> Self {
        Self::with_budget(inner, LIST_BUDGET, LIST_REQUESTS)
    }

    pub fn with_budget(inner: &'a dyn Source, bytes: u64, requests: u32) -> Self {
        Cached {
            inner,
            blocks: Mutex::new(HashMap::new()),
            spent: Mutex::new((0, 0)),
            budget: (bytes, requests),
        }
    }

    fn fetch(&self, first: u64, last: u64) -> Result<()> {
        let start = first * BLOCK;
        let end = ((last + 1) * BLOCK).min(self.inner.size());
        if end <= start {
            return Ok(());
        }
        {
            let mut spent = self.spent.lock();
            spent.0 += end - start;
            spent.1 += 1;
            if spent.0 > self.budget.0 || spent.1 > self.budget.1 {
                return Err(OverBudget.into());
            }
        }
        let data = self.inner.read_at(start, (end - start) as usize)?;
        let mut blocks = self.blocks.lock();
        if blocks.len() + (last - first + 1) as usize > CACHE_BLOCKS {
            blocks.clear();
        }
        for (i, chunk) in data.chunks(BLOCK as usize).enumerate() {
            blocks.insert(first + i as u64, std::sync::Arc::new(chunk.to_vec()));
        }
        Ok(())
    }
}

impl Source for Cached<'_> {
    fn size(&self) -> u64 {
        self.inner.size()
    }

    fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let size = self.inner.size();
        if offset >= size || len == 0 {
            return Ok(Vec::new());
        }
        let end = (offset + len as u64).min(size);
        let first = offset / BLOCK;
        let last = (end - 1) / BLOCK;
        let mut missing: Option<u64> = None;
        for b in first..=last + 1 {
            let have = b <= last && self.blocks.lock().contains_key(&b);
            match (missing, b <= last && !have) {
                (None, true) => missing = Some(b),
                (Some(m), false) => {
                    self.fetch(m, b - 1)?;
                    missing = None;
                }
                _ => {}
            }
        }
        let mut out = Vec::with_capacity((end - offset) as usize);
        for b in first..=last {
            let block = self.blocks.lock().get(&b).cloned();
            let Some(block) = block else {
                return self.inner.read_at(offset, (end - offset) as usize);
            };
            let from = if b == first {
                (offset - b * BLOCK) as usize
            } else {
                0
            };
            let to = if b == last {
                (end - b * BLOCK) as usize
            } else {
                block.len()
            };
            out.extend_from_slice(&block[from.min(block.len())..to.min(block.len())]);
        }
        Ok(out)
    }
}

pub struct SourceReader<'a> {
    pub src: &'a dyn Source,
    pub pos: u64,
}

impl Read for SourceReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let want = buf.len().min(4 * 1024 * 1024);
        let data = self
            .src
            .read_at(self.pos, want)
            .map_err(std::io::Error::other)?;
        buf[..data.len()].copy_from_slice(&data);
        self.pos += data.len() as u64;
        Ok(data.len())
    }
}

impl Seek for SourceReader<'_> {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let size = self.src.size() as i128;
        let next = match to {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::End(d) => size + d as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
        };
        if next < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before the start",
            ));
        }
        self.pos = next as u64;
        Ok(self.pos)
    }
}

#[cfg(test)]
pub struct MemSource(pub Vec<u8>);

#[cfg(test)]
impl Source for MemSource {
    fn size(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let start = (offset as usize).min(self.0.len());
        let end = start.saturating_add(len).min(self.0.len());
        Ok(self.0[start..end].to_vec())
    }
}

enum Detected {
    Format(Format),
    Stream(&'static str),
    Other(&'static str),
    Unknown,
}

fn detect(head: &[u8]) -> Detected {
    let at = |off: usize, magic: &[u8]| {
        head.len() >= off + magic.len() && &head[off..off + magic.len()] == magic
    };
    if at(0, b"PK\x03\x04") || at(0, b"PK\x05\x06") || at(0, b"PK\x07\x08") {
        return Detected::Format(Format::Zip);
    }
    if at(0, b"7z\xBC\xAF\x27\x1C") {
        return Detected::Format(Format::SevenZip);
    }
    if at(0, b"Rar!\x1A\x07") {
        return Detected::Format(Format::Rar);
    }
    if at(257, b"ustar") || tar::valid_header(head) {
        return Detected::Format(Format::Tar);
    }
    if iso::has_udf(head) {
        return Detected::Format(Format::Udf);
    }
    if iso::has_iso9660(head) {
        return Detected::Format(Format::Iso);
    }
    if at(0, b"\x1F\x8B") {
        return Detected::Stream("gzip");
    }
    if at(0, b"\xFD7zXZ\x00") {
        return Detected::Stream("xz");
    }
    if at(0, b"BZh") {
        return Detected::Stream("bzip2");
    }
    if at(0, b"\x28\xB5\x2F\xFD") {
        return Detected::Stream("zstd");
    }
    if at(0, b"\x04\x22\x4D\x18") {
        return Detected::Stream("lz4");
    }
    if at(0, b"LZIP") {
        return Detected::Stream("lzip");
    }
    if at(0, b"\x1F\x9D") {
        return Detected::Stream("compress");
    }
    if at(0, b"MSCF") {
        return Detected::Other("CAB");
    }
    if at(0, b"MSWIM\x00") {
        return Detected::Other("WIM");
    }
    Detected::Unknown
}

pub fn list(src: &dyn Source) -> Result<(Format, Vec<Entry>, bool)> {
    let head = src.read_at(0, 64 * 1024)?;
    let format = match detect(&head) {
        Detected::Format(f) => f,
        Detected::Stream(kind) => bail!(
            "this file is compressed as one {kind} stream (like .tar.gz or .tar.xz). A file inside can only be reached by unpacking everything before it, so download the whole archive instead"
        ),
        Detected::Other(kind) => bail!(
            "{kind} archives cannot be browsed over the network yet; download the whole archive instead"
        ),
        Detected::Unknown => {
            if let Ok(entries) = zip::list(src) {
                return Ok((Format::Zip, entries, false));
            }
            bail!("this is not an archive that can be browsed (supported: ZIP and ZIP-based files, ISO, UDF, TAR, 7z, RAR)")
        }
    };
    let mut truncated = false;
    let entries = match format {
        Format::Zip => zip::list(src)?,
        Format::SevenZip => sevenz::list(src)?,
        Format::Rar => {
            let (e, t) = rar::list(src)?;
            truncated = t;
            e
        }
        Format::Tar => {
            let (e, t) = tar::list(src)?;
            truncated = t;
            e
        }
        Format::Udf => match udf::list(src) {
            Ok(e) if !e.is_empty() => e,
            other => {
                if iso::has_iso9660(&head) {
                    if let Ok(e) = iso::list(src) {
                        return Ok((Format::Iso, e, false));
                    }
                }
                other?
            }
        },
        Format::Iso => iso::list(src)?,
    };
    let truncated = truncated || entries.len() >= MAX_ENTRIES;
    Ok((format, entries, truncated))
}

pub fn clean_path(raw: &str) -> String {
    raw.replace('\\', "/")
        .split('/')
        .filter(|p| !p.is_empty() && *p != "." && *p != "..")
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn le16(b: &[u8], at: usize) -> u16 {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .unwrap_or(0)
}

pub(crate) fn le32(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .unwrap_or(0)
}

pub(crate) fn le64(b: &[u8], at: usize) -> u64 {
    b.get(at..at + 8)
        .map(|s| u64::from_le_bytes(s.try_into().unwrap_or([0; 8])))
        .unwrap_or(0)
}

pub(crate) fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub(crate) fn unix_time(y: i64, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> Option<i64> {
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    Some(days_from_civil(y, mo, d) * 86400 + (h * 3600 + mi * 60 + s) as i64)
}

pub(crate) fn dos_time(date: u16, time: u16) -> Option<i64> {
    if date == 0 {
        return None;
    }
    unix_time(
        1980 + (date >> 9) as i64,
        ((date >> 5) & 15) as u32,
        (date & 31) as u32,
        (time >> 11) as u32,
        ((time >> 5) & 63) as u32,
        ((time & 31) * 2) as u32,
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    pub fn fixture(name: &str) -> Option<Vec<u8>> {
        let dir = std::env::var_os("MULTRON_TEST_FILES")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Multron-test-files")
            });
        let path = dir.join("archives").join(name);
        let Ok(gz) = std::fs::read(&path) else {
            eprintln!("skipped: {} not found", path.display());
            return None;
        };
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&gz[..])
            .read_to_end(&mut out)
            .unwrap();
        Some(out)
    }

    pub struct Counting(
        pub MemSource,
        pub AtomicU32,
        pub std::sync::atomic::AtomicU64,
    );

    impl Source for Counting {
        fn size(&self) -> u64 {
            self.0.size()
        }
        fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
            self.1.fetch_add(1, Ordering::Relaxed);
            let d = self.0.read_at(offset, len)?;
            self.2.fetch_add(d.len() as u64, Ordering::Relaxed);
            Ok(d)
        }
    }

    pub fn counting(data: Vec<u8>) -> Counting {
        Counting(MemSource(data), AtomicU32::new(0), Default::default())
    }

    pub fn take(src: &dyn Source, format: Format, entry: &Entry) -> Vec<u8> {
        let start = decode::data_start(src, entry).unwrap();
        let packed = read_exact_at(src, start, entry.fetch as usize).unwrap();
        let dir = std::env::temp_dir().join(format!("mdm-ra-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let part = dir.join("packed");
        let out = dir.join("out");
        std::fs::write(&part, &packed).unwrap();
        let pick = Pick {
            format,
            archive_size: src.size(),
            entry: entry.clone(),
        };
        decode::unpack(&pick, start, &part, &out, src, &|_| {}).unwrap();
        let data = std::fs::read(&out).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        data
    }

    pub fn find<'a>(entries: &'a [Entry], path: &str) -> &'a Entry {
        entries.iter().find(|e| e.path == path).unwrap_or_else(|| {
            panic!(
                "{path} not in {:#?}",
                entries.iter().map(|e| &e.path).collect::<Vec<_>>()
            )
        })
    }

    #[test]
    fn cache_merges_small_reads_into_block_requests() {
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let src = counting(data.clone());
        let cached = Cached::new(&src);
        assert_eq!(cached.read_at(10, 20).unwrap(), data[10..30].to_vec());
        assert_eq!(cached.read_at(1000, 5).unwrap(), data[1000..1005].to_vec());
        assert_eq!(src.1.load(Ordering::Relaxed), 1);
        let across = cached.read_at(60_000, 140_000).unwrap();
        assert_eq!(across, data[60_000..200_000].to_vec());
        assert_eq!(src.1.load(Ordering::Relaxed), 2);
        assert_eq!(
            cached.read_at(299_990, 100).unwrap(),
            data[299_990..].to_vec()
        );
        assert!(cached.read_at(400_000, 10).unwrap().is_empty());
    }

    #[test]
    fn cache_stops_at_the_budget() {
        let src = counting(vec![0; 1 << 20]);
        let cached = Cached::with_budget(&src, 200_000, 100);
        cached.read_at(0, 10).unwrap();
        let err = cached.read_at(500_000, 300_000).unwrap_err();
        assert!(over_budget(&err));
    }

    #[test]
    fn stream_compressed_files_are_refused_with_a_reason() {
        let mut gz = Vec::new();
        {
            use std::io::Write;
            let mut w = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::fast());
            w.write_all(b"not a tar").unwrap();
            w.finish().unwrap();
        }
        let err = list(&MemSource(gz)).unwrap_err().to_string();
        assert!(err.contains("gzip"), "{err}");
        let err = list(&MemSource(b"just some text, not an archive".to_vec()))
            .unwrap_err()
            .to_string();
        assert!(err.contains("not an archive"), "{err}");
    }

    #[test]
    fn inner_paths_cannot_climb_out() {
        assert_eq!(clean_path("../../etc/passwd"), "etc/passwd");
        assert_eq!(clean_path("a\\b\\..\\c"), "a/b/c");
        assert_eq!(clean_path("/abs//x/./y/"), "abs/x/y");
    }

    #[test]
    fn dates_convert() {
        assert_eq!(unix_time(1970, 1, 1, 0, 0, 0), Some(0));
        assert_eq!(unix_time(2024, 5, 1, 12, 0, 0), Some(1_714_564_800));
        assert_eq!(dos_time(0, 0), None);
    }
}
