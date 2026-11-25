use std::collections::HashSet;

use anyhow::{bail, Result};

use super::{clean_path, le16, le32, read_exact_at, unix_time, Entry, Source, MAX_ENTRIES};

const SECTOR: u64 = 2048;
const FIRST_DESCRIPTOR: u64 = 16;

fn descriptor(head: &[u8], index: u64) -> Option<&[u8]> {
    let at = ((FIRST_DESCRIPTOR + index) * SECTOR) as usize;
    head.get(at..at + SECTOR as usize)
}

pub fn has_iso9660(head: &[u8]) -> bool {
    descriptor(head, 0).is_some_and(|d| &d[1..6] == b"CD001")
}

pub fn has_udf(head: &[u8]) -> bool {
    (0..16)
        .any(|i| descriptor(head, i).is_some_and(|d| &d[1..6] == b"NSR02" || &d[1..6] == b"NSR03"))
}

#[derive(Clone, Copy, PartialEq)]
enum Names {
    Plain,
    Joliet,
    RockRidge,
}

struct Record<'a> {
    extent: u64,
    len: u64,
    flags: u8,
    name: &'a [u8],
    system: &'a [u8],
    date: &'a [u8],
}

fn record(b: &[u8]) -> Option<Record<'_>> {
    let len = *b.first()? as usize;
    if len < 34 || len > b.len() {
        return None;
    }
    let name_len = b[32] as usize;
    let name_end = 33 + name_len;
    if name_end > len {
        return None;
    }
    let system_start = name_end + (name_len + 1) % 2;
    Some(Record {
        extent: le32(b, 2) as u64,
        len: le32(b, 10) as u64,
        flags: b[25],
        name: &b[33..name_end],
        system: b.get(system_start.min(len)..len).unwrap_or(&[]),
        date: &b[18..25],
    })
}

fn record_time(d: &[u8]) -> Option<i64> {
    let t = unix_time(
        1900 + d[0] as i64,
        d[1] as u32,
        d[2] as u32,
        d[3] as u32,
        d[4] as u32,
        d[5] as u32,
    )?;
    Some(t - (d[6] as i8 as i64) * 15 * 60)
}

fn rock_ridge(system: &[u8], skip: usize) -> (Option<String>, bool) {
    let mut name: Option<Vec<u8>> = None;
    let mut link = false;
    let mut i = skip;
    while i + 4 <= system.len() {
        let len = system[i + 2] as usize;
        if len < 4 || i + len > system.len() {
            break;
        }
        let sig = &system[i..i + 2];
        let body = &system[i + 4..i + len];
        match sig {
            b"NM" if !body.is_empty() && body[0] & 0x06 == 0 => {
                name.get_or_insert_with(Vec::new)
                    .extend_from_slice(&body[1..]);
            }
            b"SL" => link = true,
            b"ST" => break,
            _ => {}
        }
        i += len;
    }
    (name.map(|n| String::from_utf8_lossy(&n).into_owned()), link)
}

fn plain_name(raw: &[u8], names: Names) -> String {
    let s = if names == Names::Joliet {
        let units: Vec<u16> = raw
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(raw).into_owned()
    };
    let s = match s.rfind(';') {
        Some(i) if s[i + 1..].chars().all(|c| c.is_ascii_digit()) => s[..i].to_string(),
        _ => s,
    };
    s.strip_suffix('.').map(str::to_string).unwrap_or(s)
}

pub fn list(src: &dyn Source) -> Result<Vec<Entry>> {
    let mut primary: Option<Vec<u8>> = None;
    let mut joliet: Option<Vec<u8>> = None;
    for i in 0..32 {
        let d = read_exact_at(src, (FIRST_DESCRIPTOR + i) * SECTOR, SECTOR as usize)?;
        if &d[1..6] != b"CD001" {
            break;
        }
        match d[0] {
            1 if primary.is_none() => primary = Some(d),
            2 if &d[88..90] == b"%/" && matches!(d[90], b'@' | b'C' | b'E') => joliet = Some(d),
            255 => break,
            _ => {}
        }
    }
    let Some(primary) = primary else {
        bail!("this disc image has no ISO 9660 volume");
    };
    let block = match le16(&primary, 128) as u64 {
        0 => SECTOR,
        b => b,
    };
    let root_of = |d: &[u8]| -> Result<(u64, u64)> {
        let r = record(&d[156..190])
            .ok_or_else(|| anyhow::anyhow!("the ISO root folder is damaged"))?;
        Ok((r.extent, r.len))
    };

    let (root, root_len) = root_of(&primary)?;
    let first = read_exact_at(src, root * block, block.min(root_len.max(34)) as usize)?;
    let mut names = Names::Plain;
    let mut skip = 0;
    if let Some(r) = record(&first) {
        if r.system.len() >= 7 && &r.system[..2] == b"SP" && r.system[4..6] == [0xBE, 0xEF] {
            names = Names::RockRidge;
            skip = r.system[6] as usize;
        }
    }
    let (root, root_len) = if names == Names::Plain {
        match &joliet {
            Some(j) => {
                names = Names::Joliet;
                root_of(j)?
            }
            None => (root, root_len),
        }
    } else {
        (root, root_len)
    };

    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![(root, root_len, String::new(), 0u32)];
    while let Some((extent, len, prefix, depth)) = stack.pop() {
        if depth > 64 || !seen.insert(extent) || len > 64 * 1024 * 1024 {
            continue;
        }
        let data = read_exact_at(src, extent * block, len as usize)?;
        let mut pos = 0usize;
        let mut pending: Option<(Entry, u64)> = None;
        while pos < data.len() {
            let rec_len = data[pos] as usize;
            if rec_len == 0 {
                pos = ((pos / block as usize) + 1) * block as usize;
                continue;
            }
            let Some(r) = record(&data[pos..]) else {
                break;
            };
            pos += rec_len;
            if r.name == [0] || r.name == [1] {
                continue;
            }
            let (rr_name, link) = if names == Names::RockRidge {
                rock_ridge(r.system, skip)
            } else {
                (None, false)
            };
            let name = rr_name.unwrap_or_else(|| plain_name(r.name, names));
            let path = clean_path(&format!("{prefix}/{name}"));
            if path.is_empty() {
                continue;
            }
            let is_dir = r.flags & 0x02 != 0;
            if is_dir {
                entries.push(Entry {
                    path: path.clone(),
                    dir: true,
                    modified: record_time(r.date),
                    ..Default::default()
                });
                stack.push((r.extent, r.len, path, depth + 1));
                continue;
            }
            let offset = r.extent * block;
            match pending.take() {
                Some((mut e, next)) if e.path == path => {
                    if offset != next {
                        e.problem = Some("fragmented".into());
                    }
                    e.size += r.len;
                    e.fetch = e.size;
                    if r.flags & 0x80 != 0 {
                        pending = Some((e, offset + r.len.div_ceil(block) * block));
                    } else {
                        entries.push(e);
                    }
                    continue;
                }
                Some((e, _)) => entries.push(e),
                None => {}
            }
            let mut e = Entry::file(path, r.len);
            e.offset = offset;
            e.modified = record_time(r.date);
            if link {
                e.problem = Some("link".into());
            }
            if r.flags & 0x80 != 0 {
                pending = Some((e, offset + r.len.div_ceil(block) * block));
            } else {
                entries.push(e);
            }
            if entries.len() >= MAX_ENTRIES {
                return Ok(entries);
            }
        }
        if let Some((e, _)) = pending {
            entries.push(e);
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{find, fixture, take};
    use super::super::{list as list_any, Format, MemSource};

    const LONG: &str = "docs/A long file name with spaces.txt";

    #[test]
    fn joliet_image_lists_long_names_and_gives_files() {
        let Some(data) = fixture("joliet.iso.gz") else {
            return;
        };
        let src = MemSource(data);
        let (format, entries, _) = list_any(&src).unwrap();
        assert_eq!(format, Format::Iso);
        assert!(find(&entries, "docs").dir);
        assert!(find(&entries, "docs/deep").dir);
        let hello = find(&entries, "hello.txt");
        assert_eq!(take(&src, format, hello), b"Hello from the ISO fixture");
        let long = find(&entries, LONG);
        assert_eq!(long.size, 5000);
        assert_eq!(take(&src, format, long), b"line\n".repeat(1000));
        assert_eq!(
            take(&src, format, find(&entries, "docs/deep/nested.bin")),
            b"nested file"
        );
        assert!(hello.modified.is_some());
    }

    #[test]
    fn plain_iso9660_image_uses_short_names() {
        let Some(data) = fixture("plain.iso.gz") else {
            return;
        };
        let src = MemSource(data);
        let (format, entries, _) = list_any(&src).unwrap();
        assert_eq!(format, Format::Iso);
        let hello = find(&entries, "HELLO.TXT");
        assert_eq!(take(&src, format, hello), b"Hello from the ISO fixture");
        let deep = entries
            .iter()
            .find(|e| e.path.starts_with("DOCS/DEEP/"))
            .unwrap();
        assert_eq!(take(&src, format, deep), b"nested file");
    }

    #[test]
    fn rock_ridge_names_are_read() {
        let (name, link) = super::rock_ridge(b"NM\x0e\x01\x00long-name", 0);
        assert_eq!(name.as_deref(), Some("long-name"));
        assert!(!link);
        let (_, link) = super::rock_ridge(b"SL\x05\x01\x00", 0);
        assert!(link);
    }
}
