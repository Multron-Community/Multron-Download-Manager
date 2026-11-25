use anyhow::{bail, Result};

use super::{
    clean_path, dos_time, le16, le32, over_budget, read_exact_at, Entry, Source, MAX_ENTRIES,
};

const RAR4: &[u8] = b"Rar!\x1A\x07\x00";
const RAR5: &[u8] = b"Rar!\x1A\x07\x01\x00";

fn vint(b: &[u8], pos: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *b.get(*pos)?;
        *pos += 1;
        v |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

pub fn list(src: &dyn Source) -> Result<(Vec<Entry>, bool)> {
    let head = read_exact_at(src, 0, 8)?;
    if head.starts_with(RAR5) {
        list5(src)
    } else if head.starts_with(RAR4) {
        list4(src)
    } else {
        bail!("unknown RAR version")
    }
}

fn walk_error(e: anyhow::Error, entries: Vec<Entry>) -> Result<(Vec<Entry>, bool)> {
    if over_budget(&e) && !entries.is_empty() {
        Ok((entries, true))
    } else {
        Err(e)
    }
}

fn list5(src: &dyn Source) -> Result<(Vec<Entry>, bool)> {
    let size = src.size();
    let mut entries = Vec::new();
    let mut pos = RAR5.len() as u64;
    let mut solid = false;
    while pos + 7 <= size {
        let probe = match src.read_at(pos, 16) {
            Ok(b) => b,
            Err(e) => return walk_error(e, entries),
        };
        let mut p = 4;
        let Some(header_size) = vint(&probe, &mut p) else {
            break;
        };
        let total = p as u64 + header_size;
        if header_size == 0 || header_size > 2 * 1024 * 1024 {
            bail!("the RAR archive is damaged");
        }
        let h = match read_exact_at(src, pos, total as usize) {
            Ok(h) => h,
            Err(e) => return walk_error(e, entries),
        };
        let mut i = p;
        let kind = vint(&h, &mut i).unwrap_or(0);
        let flags = vint(&h, &mut i).unwrap_or(0);
        let extra_size = if flags & 1 != 0 {
            vint(&h, &mut i).unwrap_or(0)
        } else {
            0
        };
        let data_size = if flags & 2 != 0 {
            vint(&h, &mut i).unwrap_or(0)
        } else {
            0
        };
        let data = pos + total;
        pos = data + data_size;
        match kind {
            1 => {
                let archive_flags = vint(&h, &mut i).unwrap_or(0);
                if archive_flags & 1 != 0 {
                    bail!("this RAR is one part of a multi-part set, which cannot be browsed");
                }
                solid = archive_flags & 4 != 0;
            }
            2 => {
                let file_flags = vint(&h, &mut i).unwrap_or(0);
                let unpacked = vint(&h, &mut i).unwrap_or(0);
                let _attributes = vint(&h, &mut i);
                let mtime = if file_flags & 2 != 0 {
                    let t = le32(&h, i) as i64;
                    i += 4;
                    Some(t)
                } else {
                    None
                };
                let crc = if file_flags & 4 != 0 {
                    let c = le32(&h, i);
                    i += 4;
                    Some(c)
                } else {
                    None
                };
                let compression = vint(&h, &mut i).unwrap_or(0);
                let _host = vint(&h, &mut i);
                let name_len = vint(&h, &mut i).unwrap_or(0) as usize;
                let name =
                    String::from_utf8_lossy(h.get(i..i + name_len).unwrap_or(&[])).into_owned();
                i += name_len;
                let mut encrypted = false;
                if extra_size > 0 {
                    let end = (i + extra_size as usize).min(h.len());
                    while i < end {
                        let rec_size = vint(&h, &mut i).unwrap_or(0) as usize;
                        let rec_start = i;
                        if vint(&h, &mut i) == Some(1) {
                            encrypted = true;
                        }
                        i = rec_start + rec_size;
                        if rec_size == 0 {
                            break;
                        }
                    }
                }
                let path = clean_path(&name);
                if path.is_empty() {
                    continue;
                }
                let is_dir = file_flags & 1 != 0;
                let mut e = Entry {
                    path,
                    dir: is_dir,
                    size: if is_dir { 0 } else { unpacked },
                    modified: mtime,
                    offset: data,
                    fetch: data_size,
                    crc,
                    ..Default::default()
                };
                let method = (compression >> 7) & 7;
                if !is_dir {
                    e.problem = if encrypted {
                        Some("encrypted".into())
                    } else if flags & 0x18 != 0 {
                        Some("fragmented".into())
                    } else if method != 0 {
                        Some(if solid { "solid" } else { "method" }.into())
                    } else {
                        None
                    };
                }
                entries.push(e);
                if entries.len() >= MAX_ENTRIES {
                    return Ok((entries, true));
                }
            }
            4 => bail!("this RAR archive's file list is encrypted"),
            5 => break,
            _ => {}
        }
    }
    Ok((entries, false))
}

fn rar4_unicode(raw: &[u8]) -> Option<String> {
    let zero = raw.iter().position(|&b| b == 0)?;
    let ascii = &raw[..zero];
    let enc = &raw[zero + 1..];
    if enc.is_empty() {
        return None;
    }
    let high = *enc.first()? as u16;
    let mut pos = 1;
    let mut out: Vec<u16> = Vec::new();
    let mut flags = 0u8;
    let mut bits = 0;
    while pos < enc.len() {
        if bits == 0 {
            flags = enc[pos];
            pos += 1;
            bits = 8;
        }
        bits -= 2;
        match (flags >> bits) & 3 {
            0 => {
                out.push(*enc.get(pos)? as u16);
                pos += 1;
            }
            1 => {
                out.push(*enc.get(pos)? as u16 | (high << 8));
                pos += 1;
            }
            2 => {
                out.push(u16::from_le_bytes([*enc.get(pos)?, *enc.get(pos + 1)?]));
                pos += 2;
            }
            _ => {
                let len = *enc.get(pos)?;
                pos += 1;
                if len & 0x80 != 0 {
                    let correction = *enc.get(pos)?;
                    pos += 1;
                    for _ in 0..(len & 0x7F) + 2 {
                        let c = *ascii.get(out.len())?;
                        out.push(((c.wrapping_add(correction)) as u16) | (high << 8));
                    }
                } else {
                    for _ in 0..len + 2 {
                        out.push(*ascii.get(out.len())? as u16);
                    }
                }
            }
        }
    }
    Some(String::from_utf16_lossy(&out))
}

fn list4(src: &dyn Source) -> Result<(Vec<Entry>, bool)> {
    let size = src.size();
    let mut entries = Vec::new();
    let mut pos = RAR4.len() as u64;
    let mut solid = false;
    while pos + 7 <= size {
        let fixed = match read_exact_at(src, pos, 7) {
            Ok(b) => b,
            Err(e) => return walk_error(e, entries),
        };
        let kind = fixed[2];
        let flags = le16(&fixed, 3);
        let head_size = le16(&fixed, 5) as u64;
        if head_size < 7 {
            bail!("the RAR archive is damaged");
        }
        let h = match read_exact_at(src, pos, head_size as usize) {
            Ok(h) => h,
            Err(e) => return walk_error(e, entries),
        };
        let add_size = if flags & 0x8000 != 0 || kind == 0x74 {
            le32(&h, 7) as u64
        } else {
            0
        };
        let data = pos + head_size;
        match kind {
            0x73 => {
                if flags & 0x0001 != 0 {
                    bail!("this RAR is one part of a multi-part set, which cannot be browsed");
                }
                if flags & 0x0080 != 0 {
                    bail!("this RAR archive's file list is encrypted");
                }
                solid = flags & 0x0008 != 0;
            }
            0x74 => {
                let mut packed = le32(&h, 7) as u64;
                let mut unpacked = le32(&h, 11) as u64;
                let crc = le32(&h, 16);
                let ftime = le32(&h, 20);
                let method = h.get(25).copied().unwrap_or(0);
                let name_len = le16(&h, 26) as usize;
                let mut name_at = 32;
                if flags & 0x100 != 0 {
                    packed |= (le32(&h, 32) as u64) << 32;
                    unpacked |= (le32(&h, 36) as u64) << 32;
                    name_at = 40;
                }
                let raw = h.get(name_at..name_at + name_len).unwrap_or(&[]);
                let name = if flags & 0x200 != 0 {
                    rar4_unicode(raw).unwrap_or_else(|| {
                        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
                        String::from_utf8_lossy(&raw[..end]).into_owned()
                    })
                } else {
                    String::from_utf8_lossy(raw).into_owned()
                };
                let is_dir = flags & 0xE0 == 0xE0;
                let path = clean_path(&name);
                pos = data + packed;
                if path.is_empty() {
                    continue;
                }
                let mut e = Entry {
                    path,
                    dir: is_dir,
                    size: if is_dir { 0 } else { unpacked },
                    modified: dos_time((ftime >> 16) as u16, ftime as u16),
                    offset: data,
                    fetch: packed,
                    crc: Some(crc),
                    ..Default::default()
                };
                if !is_dir {
                    e.problem = if flags & 0x04 != 0 {
                        Some("encrypted".into())
                    } else if flags & 0x03 != 0 {
                        Some("fragmented".into())
                    } else if method != 0x30 {
                        Some(
                            if solid || flags & 0x10 != 0 {
                                "solid"
                            } else {
                                "method"
                            }
                            .into(),
                        )
                    } else {
                        None
                    };
                }
                entries.push(e);
                if entries.len() >= MAX_ENTRIES {
                    return Ok((entries, true));
                }
                continue;
            }
            0x7B => break,
            _ => {}
        }
        pos = data + add_size;
    }
    Ok((entries, false))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{find, take};
    use super::super::{list as list_any, Format, MemSource};
    use super::*;

    fn v(mut n: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let b = (n & 0x7F) as u8;
            n >>= 7;
            if n == 0 {
                out.push(b);
                return out;
            }
            out.push(b | 0x80);
        }
    }

    fn block5(kind: u64, flags: u64, body: &[u8], data: &[u8]) -> Vec<u8> {
        let mut h = v(kind);
        h.extend(v(flags | if data.is_empty() { 0 } else { 2 }));
        if !data.is_empty() {
            h.extend(v(data.len() as u64));
        }
        h.extend_from_slice(body);
        let mut size = v(h.len() as u64);
        size.extend(h);
        let mut out = crc32fast::hash(&size).to_le_bytes().to_vec();
        out.extend(size);
        out.extend_from_slice(data);
        out
    }

    fn file5(name: &str, data: &[u8], method: u64) -> Vec<u8> {
        let mut b = v(0x06);
        b.extend(v(data.len() as u64));
        b.extend(v(0x20));
        b.extend(1_714_564_800u32.to_le_bytes());
        b.extend(crc32fast::hash(data).to_le_bytes());
        b.extend(v(method << 7));
        b.extend(v(0));
        b.extend(v(name.len() as u64));
        b.extend(name.as_bytes());
        block5(2, 0, &b, data)
    }

    #[test]
    fn rar5_stored_files_come_out_and_compressed_ones_are_marked() {
        let mut rar = RAR5.to_vec();
        rar.extend(block5(1, 0, &v(0), &[]));
        rar.extend(file5("folder/stored.txt", b"stored in a rar", 0));
        rar.extend(file5("packed.bin", b"pretend compressed", 3));
        rar.extend(block5(5, 0, &v(0), &[]));
        let src = MemSource(rar);
        let (format, entries, _) = list_any(&src).unwrap();
        assert_eq!(format, Format::Rar);
        let stored = find(&entries, "folder/stored.txt");
        assert!(stored.problem.is_none());
        assert_eq!(stored.modified, Some(1_714_564_800));
        assert_eq!(take(&src, format, stored), b"stored in a rar");
        assert_eq!(
            find(&entries, "packed.bin").problem.as_deref(),
            Some("method")
        );
    }

    fn file4(name: &str, data: &[u8], method: u8) -> Vec<u8> {
        let mut h = vec![0, 0, 0x74];
        h.extend(0x8000u16.to_le_bytes());
        let size = 32 + name.len();
        h.extend((size as u16).to_le_bytes());
        h.extend((data.len() as u32).to_le_bytes());
        h.extend((data.len() as u32).to_le_bytes());
        h.push(2);
        h.extend(crc32fast::hash(data).to_le_bytes());
        h.extend(0x5821_6000u32.to_le_bytes());
        h.push(29);
        h.push(method);
        h.extend((name.len() as u16).to_le_bytes());
        h.extend(0x20u32.to_le_bytes());
        h.extend(name.as_bytes());
        h.extend_from_slice(data);
        h
    }

    #[test]
    fn rar4_stored_files_come_out() {
        let mut rar = RAR4.to_vec();
        rar.extend([0, 0, 0x73, 0, 0, 13, 0, 0, 0, 0, 0, 0, 0]);
        rar.extend(file4("dir\\old.txt", b"rar four data", 0x30));
        rar.extend(file4("comp.txt", b"xx", 0x33));
        rar.extend([0, 0, 0x7B, 0, 0x40, 7, 0]);
        let src = MemSource(rar);
        let (format, entries, _) = list_any(&src).unwrap();
        assert_eq!(format, Format::Rar);
        let old = find(&entries, "dir/old.txt");
        assert!(old.modified.is_some());
        assert_eq!(take(&src, format, old), b"rar four data");
        assert_eq!(
            find(&entries, "comp.txt").problem.as_deref(),
            Some("method")
        );
    }

    #[test]
    fn rar4_unicode_names_decode() {
        let raw = [b'a', b'b', 0, 0x04, 0b0101_0000, 0x30, 0x31];
        assert_eq!(rar4_unicode(&raw).as_deref(), Some("\u{430}\u{431}"));
    }
}
