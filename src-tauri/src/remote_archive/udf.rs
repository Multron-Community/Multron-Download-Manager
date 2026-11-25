use std::collections::HashSet;

use anyhow::{anyhow, bail, Context, Result};

use super::{clean_path, le16, le32, le64, read_exact_at, unix_time, Entry, Source, MAX_ENTRIES};

const SECTOR: u64 = 2048;

const TAG_AVDP: u16 = 2;
const TAG_PARTITION: u16 = 5;
const TAG_LOGICAL_VOLUME: u16 = 6;
const TAG_TERMINATOR: u16 = 8;
const TAG_FILE_SET: u16 = 256;
const TAG_FID: u16 = 257;
const TAG_FILE_ENTRY: u16 = 261;
const TAG_EXT_FILE_ENTRY: u16 = 266;

#[derive(Clone, Copy, Debug)]
struct Extent {
    at: u64,
    len: u64,
    recorded: bool,
}

enum Map {
    Physical(u16),
    Metadata {
        partition: u16,
        extents: Vec<(u64, u64, u64)>,
    },
}

struct Volume<'a> {
    src: &'a dyn Source,
    block: u64,
    partitions: Vec<(u16, u64)>,
    maps: Vec<Map>,
}

struct Node {
    file_type: u8,
    size: u64,
    modified: Option<i64>,
    extents: Vec<Extent>,
}

fn tag_id(b: &[u8]) -> u16 {
    le16(b, 0)
}

fn timestamp(b: &[u8]) -> Option<i64> {
    let type_tz = le16(b, 0);
    let year = le16(b, 2) as i16 as i64;
    let t = unix_time(
        year,
        b[4] as u32,
        b[5] as u32,
        b[6] as u32,
        b[7] as u32,
        b[8] as u32,
    )?;
    let tz = (type_tz & 0x0FFF) as i16;
    let tz = if tz & 0x0800 != 0 { tz | !0x0FFF } else { tz };
    if type_tz >> 12 == 1 && tz != -2047 {
        Some(t - tz as i64 * 60)
    } else {
        Some(t)
    }
}

fn dstring(b: &[u8]) -> String {
    match b.first() {
        Some(8) => b[1..].iter().map(|&c| c as char).collect(),
        Some(16) => {
            let units: Vec<u16> = b[1..]
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        }
        _ => String::new(),
    }
}

impl Volume<'_> {
    fn sector(&self, n: u64) -> Result<Vec<u8>> {
        read_exact_at(self.src, n * SECTOR, SECTOR as usize)
    }

    fn partition_start(&self, number: u16) -> Result<u64> {
        self.partitions
            .iter()
            .find(|(n, _)| *n == number)
            .map(|(_, s)| *s)
            .ok_or_else(|| anyhow!("UDF partition {number} is missing"))
    }

    fn offset(&self, part: u16, lbn: u64) -> Result<u64> {
        match self
            .maps
            .get(part as usize)
            .context("bad UDF partition reference")?
        {
            Map::Physical(n) => Ok(self.partition_start(*n)? * SECTOR + lbn * self.block),
            Map::Metadata { partition, extents } => {
                let at = lbn * self.block;
                let start = self.partition_start(*partition)? * SECTOR;
                extents
                    .iter()
                    .find(|(file_at, _, len)| at >= *file_at && at < file_at + len)
                    .map(|(file_at, phys, _)| start + phys + (at - file_at))
                    .ok_or_else(|| anyhow!("UDF metadata block {lbn} is outside the metadata file"))
            }
        }
    }

    fn read_block(&self, part: u16, lbn: u64) -> Result<Vec<u8>> {
        read_exact_at(self.src, self.offset(part, lbn)?, self.block as usize)
    }

    fn node(&self, part: u16, lbn: u64) -> Result<Node> {
        let at = self.offset(part, lbn)?;
        let b = self.read_block(part, lbn)?;
        let id = tag_id(&b);
        let (size, modified, l_ea, l_ad, base) = match id {
            TAG_FILE_ENTRY => (
                le64(&b, 56),
                timestamp(&b[84..96]),
                le32(&b, 168),
                le32(&b, 172),
                176usize,
            ),
            TAG_EXT_FILE_ENTRY => (
                le64(&b, 56),
                timestamp(&b[92..104]),
                le32(&b, 208),
                le32(&b, 212),
                216usize,
            ),
            _ => bail!("expected a UDF file entry, found tag {id}"),
        };
        let file_type = b[16 + 11];
        let ad_type = le16(&b, 16 + 18) & 7;
        let ads_at = base + l_ea as usize;
        let ads_end = ads_at + l_ad as usize;
        if ads_end > b.len() {
            bail!("a UDF file entry is damaged");
        }
        let mut extents = Vec::new();
        match ad_type {
            0 | 1 => {
                let step = if ad_type == 0 { 8 } else { 16 };
                let mut i = ads_at;
                while i + step <= ads_end {
                    let raw = le32(&b, i);
                    let len = (raw & 0x3FFF_FFFF) as u64;
                    let kind = raw >> 30;
                    if len == 0 {
                        break;
                    }
                    if kind == 3 {
                        bail!("UDF allocation continuation extents are not supported");
                    }
                    let pos = le32(&b, i + 4) as u64;
                    let p = if ad_type == 0 { part } else { le16(&b, i + 8) };
                    extents.push(Extent {
                        at: if kind == 0 { self.offset(p, pos)? } else { 0 },
                        len,
                        recorded: kind == 0,
                    });
                    i += step;
                }
            }
            3 => {
                extents.push(Extent {
                    at: at + ads_at as u64,
                    len: l_ad as u64,
                    recorded: true,
                });
            }
            _ => bail!("UDF extended allocation descriptors are not supported"),
        }
        Ok(Node {
            file_type,
            size,
            modified,
            extents,
        })
    }

    fn read_node(&self, node: &Node) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(node.size as usize);
        for e in &node.extents {
            let want = e.len.min(node.size - out.len() as u64);
            if e.recorded {
                out.extend(read_exact_at(self.src, e.at, want as usize)?);
            } else {
                out.resize(out.len() + want as usize, 0);
            }
            if out.len() as u64 >= node.size {
                break;
            }
        }
        Ok(out)
    }
}

fn contiguous(node: &Node) -> Option<u64> {
    if node.size == 0 {
        return Some(0);
    }
    let first = node.extents.first()?;
    let mut end = first.at;
    let mut left = node.size;
    for e in &node.extents {
        if !e.recorded || e.at != end {
            return None;
        }
        end += e.len;
        left = left.saturating_sub(e.len);
        if left == 0 {
            return Some(first.at);
        }
    }
    None
}

fn find_avdp(src: &dyn Source) -> Result<Vec<u8>> {
    let last = src.size() / SECTOR;
    for n in [256, last.saturating_sub(1), last.saturating_sub(257), 512] {
        if n == 0 || n >= last {
            continue;
        }
        let b = read_exact_at(src, n * SECTOR, SECTOR as usize)?;
        if tag_id(&b) == TAG_AVDP && le32(&b, 12) as u64 == n {
            return Ok(b);
        }
    }
    bail!("this disc image has no UDF anchor")
}

pub fn list(src: &dyn Source) -> Result<Vec<Entry>> {
    let avdp = find_avdp(src)?;
    let vds_len = le32(&avdp, 16) as u64;
    let vds_at = le32(&avdp, 20) as u64;
    let mut vol = Volume {
        src,
        block: SECTOR,
        partitions: Vec::new(),
        maps: Vec::new(),
    };
    let mut lvd: Option<Vec<u8>> = None;
    for i in 0..(vds_len / SECTOR).min(64) {
        let d = vol.sector(vds_at + i)?;
        match tag_id(&d) {
            TAG_PARTITION => vol.partitions.push((le16(&d, 22), le32(&d, 188) as u64)),
            TAG_LOGICAL_VOLUME if lvd.is_none() => lvd = Some(d),
            TAG_TERMINATOR => break,
            _ => {}
        }
    }
    let lvd = lvd.context("the UDF volume has no logical volume descriptor")?;
    vol.block = match le32(&lvd, 212) as u64 {
        0 => SECTOR,
        b => b,
    };
    let fsd_lbn = le32(&lvd, 252) as u64;
    let fsd_part = le16(&lvd, 256);
    let map_count = le32(&lvd, 268) as usize;
    let mut at = 440usize;
    let mut metadata_files = Vec::new();
    for _ in 0..map_count.min(16) {
        let kind = *lvd.get(at).unwrap_or(&0);
        let len = *lvd.get(at + 1).unwrap_or(&0) as usize;
        if len == 0 || at + len > lvd.len() {
            break;
        }
        let m = &lvd[at..at + len];
        match kind {
            1 => vol.maps.push(Map::Physical(le16(m, 4))),
            2 => {
                let ident = &m[5..28];
                let partition = le16(m, 38);
                if ident.starts_with(b"*UDF Metadata Partition") {
                    metadata_files.push((vol.maps.len(), partition, le32(m, 40) as u64));
                    vol.maps.push(Map::Metadata {
                        partition,
                        extents: Vec::new(),
                    });
                } else if ident.starts_with(b"*UDF Sparable Partition") {
                    vol.maps.push(Map::Physical(partition));
                } else {
                    bail!(
                        "this UDF image uses a {} partition, which is not supported",
                        String::from_utf8_lossy(ident).trim_end_matches('\0').trim()
                    );
                }
            }
            _ => break,
        }
        at += len;
    }
    for (index, partition, file_lbn) in metadata_files {
        let start = vol.partition_start(partition)? * SECTOR;
        let b = read_exact_at(src, start + file_lbn * vol.block, vol.block as usize)?;
        let ad_type = le16(&b, 16 + 18) & 7;
        let (l_ea, l_ad, base) = match tag_id(&b) {
            TAG_FILE_ENTRY => (le32(&b, 168), le32(&b, 172), 176usize),
            TAG_EXT_FILE_ENTRY => (le32(&b, 208), le32(&b, 212), 216usize),
            t => bail!("the UDF metadata file is damaged (tag {t})"),
        };
        let step = if ad_type == 1 { 16 } else { 8 };
        let mut extents = Vec::new();
        let mut file_at = 0u64;
        let mut i = base + l_ea as usize;
        while i + step <= (base + l_ea as usize + l_ad as usize).min(b.len()) {
            let len = (le32(&b, i) & 0x3FFF_FFFF) as u64;
            if len == 0 {
                break;
            }
            extents.push((file_at, le32(&b, i + 4) as u64 * vol.block, len));
            file_at += len;
            i += step;
        }
        vol.maps[index] = Map::Metadata { partition, extents };
    }

    let fsd = vol.read_block(fsd_part, fsd_lbn)?;
    if tag_id(&fsd) != TAG_FILE_SET {
        bail!("the UDF file set descriptor is missing");
    }
    let root_lbn = le32(&fsd, 404) as u64;
    let root_part = le16(&fsd, 408);

    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![(root_part, root_lbn, String::new(), 0u32)];
    while let Some((part, lbn, prefix, depth)) = stack.pop() {
        if depth > 64 || !seen.insert((part, lbn)) {
            continue;
        }
        let dir = vol.node(part, lbn)?;
        if dir.size > 64 * 1024 * 1024 {
            continue;
        }
        let data = vol.read_node(&dir)?;
        let mut pos = 0usize;
        while pos + 38 <= data.len() {
            let b = &data[pos..];
            if tag_id(b) != TAG_FID {
                break;
            }
            let flags = b[18];
            let l_fi = b[19] as usize;
            let icb_lbn = le32(b, 24) as u64;
            let icb_part = le16(b, 28);
            let l_iu = le16(b, 36) as usize;
            let name_at = 38 + l_iu;
            let total = (name_at + l_fi + 3) & !3;
            if name_at + l_fi > b.len() {
                break;
            }
            pos += total;
            if flags & 0x08 != 0 || flags & 0x04 != 0 || l_fi == 0 {
                continue;
            }
            let name = dstring(&b[name_at..name_at + l_fi]);
            let path = clean_path(&format!("{prefix}/{name}"));
            if path.is_empty() {
                continue;
            }
            let node = match vol.node(icb_part, icb_lbn) {
                Ok(n) => n,
                Err(e) => {
                    tracing::debug!("skipping UDF entry {path}: {e:#}");
                    continue;
                }
            };
            if flags & 0x02 != 0 || node.file_type == 4 {
                entries.push(Entry {
                    path: path.clone(),
                    dir: true,
                    modified: node.modified,
                    ..Default::default()
                });
                stack.push((icb_part, icb_lbn, path, depth + 1));
                continue;
            }
            let mut e = Entry::file(path, node.size);
            e.modified = node.modified;
            match contiguous(&node) {
                Some(at) => e.offset = at,
                None => e.problem = Some("fragmented".into()),
            }
            if node.file_type == 12 {
                e.problem = Some("link".into());
            }
            entries.push(e);
            if entries.len() >= MAX_ENTRIES {
                return Ok(entries);
            }
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{find, fixture, take};
    use super::super::{list as list_any, Format, MemSource};

    fn check(name: &str) {
        let Some(data) = fixture(name) else {
            return;
        };
        let src = MemSource(data);
        let (format, entries, _) = list_any(&src).unwrap();
        assert_eq!(format, Format::Udf, "{name}");
        assert!(find(&entries, "docs").dir);
        assert!(find(&entries, "docs/deep").dir);
        let hello = find(&entries, "hello.txt");
        assert!(hello.problem.is_none());
        assert_eq!(take(&src, format, hello), b"Hello from the ISO fixture");
        let long = find(&entries, "docs/A long file name with spaces.txt");
        assert_eq!(take(&src, format, long), b"line\n".repeat(1000));
        assert_eq!(
            take(&src, format, find(&entries, "docs/deep/nested.bin")),
            b"nested file"
        );
        assert!(hello.modified.unwrap_or(0) > 1_600_000_000);
    }

    #[test]
    fn udf_102_image() {
        check("udf.iso.gz");
    }

    #[test]
    fn udf_250_image_with_metadata_partition() {
        check("udf250.iso.gz");
    }

    #[test]
    fn hybrid_image_prefers_udf() {
        check("hybrid.iso.gz");
    }
}
