use anyhow::{bail, Context, Result};

use super::{
    clean_path, dos_time, le16, le32, le64, read_exact_at, Entry, Method, Source, MAX_ENTRIES,
};

const EOCD: u32 = 0x0605_4b50;
const ZIP64_LOCATOR: u32 = 0x0706_4b50;
const ZIP64_EOCD: u32 = 0x0606_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

struct Directory {
    offset: u64,
    size: u64,
    count: u64,
    delta: u64,
}

fn find_directory(src: &dyn Source) -> Result<Directory> {
    let size = src.size();
    let tail_len = size.min(22 + 65_535);
    let tail_start = size - tail_len;
    let tail = read_exact_at(src, tail_start, tail_len as usize)?;
    let mut found = None;
    let mut i = tail.len().saturating_sub(22) as isize;
    while i >= 0 {
        let at = i as usize;
        if le32(&tail, at) == EOCD {
            let comment = le16(&tail, at + 20) as usize;
            if at + 22 + comment == tail.len() || found.is_none() {
                let exact = at + 22 + comment == tail.len();
                found = Some(at);
                if exact {
                    break;
                }
            }
        }
        i -= 1;
    }
    let at = found.context("this is not a ZIP file (no central directory at its end)")?;
    let eocd = &tail[at..];
    let eocd_pos = tail_start + at as u64;
    let disk = le16(eocd, 4);
    let cd_disk = le16(eocd, 6);
    let mut count = le16(eocd, 10) as u64;
    let mut cd_size = le32(eocd, 12) as u64;
    let mut cd_offset = le32(eocd, 16) as u64;
    let mut end_of_cd = eocd_pos;
    let mut split = disk != 0 || cd_disk != 0;

    let wide = count == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_offset == 0xFFFF_FFFF;
    if eocd_pos >= 20 {
        let loc = read_exact_at(src, eocd_pos - 20, 20)?;
        if le32(&loc, 0) == ZIP64_LOCATOR {
            let disks = le32(&loc, 16);
            split |= disks > 1;
            let mut z64_pos = le64(&loc, 8);
            let mut rec = read_exact_at(src, z64_pos, 56).unwrap_or_default();
            if le32(&rec, 0) != ZIP64_EOCD && eocd_pos >= 20 + 56 {
                z64_pos = eocd_pos - 20 - 56;
                rec = read_exact_at(src, z64_pos, 56)?;
            }
            if le32(&rec, 0) == ZIP64_EOCD {
                count = le64(&rec, 32);
                cd_size = le64(&rec, 40);
                cd_offset = le64(&rec, 48);
                end_of_cd = z64_pos;
            } else if wide {
                bail!("the ZIP64 end record is missing");
            }
        } else if wide && count != 0xFFFF {
            bail!("the ZIP64 end record is missing");
        }
    }
    if split {
        bail!("this ZIP is split into several parts (.z01, .z02, ...), which cannot be browsed");
    }
    let real_start = end_of_cd
        .checked_sub(cd_size)
        .context("the ZIP central directory is damaged")?;
    let delta = real_start.saturating_sub(cd_offset);
    Ok(Directory {
        offset: cd_offset + delta,
        size: cd_size,
        count,
        delta,
    })
}

const CP437: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', 'É', 'æ', 'Æ',
    'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', 'á', 'í', 'ó', 'ú', 'ñ', 'Ñ',
    'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕',
    '╣', '║', '╗', '╝', '╜', '╛', '┐', '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦',
    '╠', '═', '╬', '╧', '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐',
    '▀', 'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', '≡', '±',
    '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

fn decode_name(raw: &[u8], utf8: bool) -> String {
    if utf8 {
        return String::from_utf8_lossy(raw).into_owned();
    }
    match std::str::from_utf8(raw) {
        Ok(s) => s.to_string(),
        Err(_) => raw
            .iter()
            .map(|&b| {
                if b < 0x80 {
                    b as char
                } else {
                    CP437[(b - 0x80) as usize]
                }
            })
            .collect(),
    }
}

pub fn list(src: &dyn Source) -> Result<Vec<Entry>> {
    let dir = find_directory(src)?;
    if dir.size > super::LIST_BUDGET {
        return Err(super::OverBudget.into());
    }
    let cd = read_exact_at(src, dir.offset, dir.size as usize)?;
    let mut entries = Vec::new();
    let mut pos = 0usize;
    while pos + 46 <= cd.len() && (entries.len() as u64) < dir.count.max(1) * 2 {
        if le32(&cd, pos) != CENTRAL {
            if entries.is_empty() {
                bail!("the ZIP central directory is damaged");
            }
            break;
        }
        let flags = le16(&cd, pos + 8);
        let method = le16(&cd, pos + 10);
        let time = le16(&cd, pos + 12);
        let date = le16(&cd, pos + 14);
        let crc = le32(&cd, pos + 16);
        let mut csize = le32(&cd, pos + 20) as u64;
        let mut usize_ = le32(&cd, pos + 24) as u64;
        let nlen = le16(&cd, pos + 28) as usize;
        let elen = le16(&cd, pos + 30) as usize;
        let clen = le16(&cd, pos + 32) as usize;
        let ext_attr = le32(&cd, pos + 38);
        let mut local = le32(&cd, pos + 42) as u64;
        let name_end = pos + 46 + nlen;
        let extra_end = name_end + elen;
        if extra_end > cd.len() {
            break;
        }
        let mut name = decode_name(&cd[pos + 46..name_end], flags & 0x0800 != 0);
        let mut modified = dos_time(date, time);
        let extra = &cd[name_end..extra_end];
        let mut e = 0;
        while e + 4 <= extra.len() {
            let id = le16(extra, e);
            let len = le16(extra, e + 2) as usize;
            let body = &extra[e + 4..(e + 4 + len).min(extra.len())];
            match id {
                0x0001 => {
                    let mut f = 0;
                    let (wu, wc, wl) = (
                        usize_ == 0xFFFF_FFFF,
                        csize == 0xFFFF_FFFF,
                        local == 0xFFFF_FFFF,
                    );
                    let mut next = |v: &mut u64, wide: bool| {
                        if wide && f + 8 <= body.len() {
                            *v = le64(body, f);
                            f += 8;
                        }
                    };
                    next(&mut usize_, wu);
                    next(&mut csize, wc);
                    next(&mut local, wl);
                }
                0x7075 if body.len() > 5 => {
                    name = String::from_utf8_lossy(&body[5..]).into_owned();
                }
                0x5455 if body.len() >= 5 && body[0] & 1 != 0 => {
                    modified = Some(le32(body, 1) as i32 as i64);
                }
                _ => {}
            }
            e += 4 + len;
        }
        pos = extra_end + clen;

        let is_dir =
            name.ends_with('/') || name.ends_with('\\') || (ext_attr & 0x10 != 0 && usize_ == 0);
        let path = clean_path(&name);
        if path.is_empty() {
            continue;
        }
        let unix_mode = ext_attr >> 16;
        let mut entry = Entry {
            path,
            dir: is_dir,
            size: if is_dir { 0 } else { usize_ },
            modified,
            offset: local + dir.delta,
            fetch: csize,
            crc: Some(crc),
            local_header: true,
            ..Default::default()
        };
        entry.method = match method {
            0 => Method::Stored,
            8 => Method::Deflate,
            9 => Method::Deflate64,
            _ => Method::Unsupported,
        };
        if flags & 1 != 0 || method == 99 {
            entry.problem = Some("encrypted".into());
        } else if entry.method == Method::Unsupported {
            entry.problem = Some("method".into());
        } else if unix_mode & 0o170000 == 0o120000 {
            entry.problem = Some("link".into());
        }
        entries.push(entry);
        if entries.len() >= MAX_ENTRIES {
            break;
        }
    }
    Ok(entries)
}

pub fn data_start(src: &dyn Source, local: u64) -> Result<u64> {
    let head = read_exact_at(src, local, 30)?;
    if le32(&head, 0) != LOCAL {
        bail!("the file inside the ZIP was not found where the archive says (the archive may have changed on the server)");
    }
    Ok(local + 30 + le16(&head, 26) as u64 + le16(&head, 28) as u64)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{counting, find, take};
    use super::super::{list as list_any, Cached, Format, MemSource};
    use super::*;
    use std::io::Write;

    fn build(prefix: &[u8], files: &[(&str, &[u8], zip::CompressionMethod)]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        out.write_all(prefix).unwrap();
        {
            let mut w = zip::ZipWriter::new(&mut out);
            for (name, data, method) in files {
                let opts = zip::write::SimpleFileOptions::default().compression_method(*method);
                if name.ends_with('/') {
                    w.add_directory(*name, opts).unwrap();
                } else {
                    w.start_file(*name, opts).unwrap();
                    w.write_all(data).unwrap();
                }
            }
            w.set_comment("a comment at the end").unwrap();
            w.finish().unwrap();
        }
        out.into_inner()
    }

    fn sample() -> Vec<u8> {
        build(
            b"",
            &[
                ("docs/", b"", zip::CompressionMethod::Stored),
                (
                    "docs/readme.txt",
                    b"read me please",
                    zip::CompressionMethod::Stored,
                ),
                (
                    "big.log",
                    &b"0123456789abcdef".repeat(4000),
                    zip::CompressionMethod::Deflated,
                ),
                (
                    "Türkçe ğüşiöç.txt",
                    b"unicode name",
                    zip::CompressionMethod::Deflated,
                ),
            ],
        )
    }

    #[test]
    fn lists_and_takes_out_stored_and_deflated_files() {
        let data = sample();
        let src = MemSource(data);
        let (format, entries, truncated) = list_any(&src).unwrap();
        assert_eq!(format, Format::Zip);
        assert!(!truncated);
        assert_eq!(entries.len(), 4);
        assert!(find(&entries, "docs").dir);
        let readme = find(&entries, "docs/readme.txt");
        assert_eq!(readme.method, Method::Stored);
        assert_eq!(take(&src, Format::Zip, readme), b"read me please");
        let big = find(&entries, "big.log");
        assert_eq!(big.method, Method::Deflate);
        assert_eq!(big.size, 64_000);
        assert!(big.fetch < 2_000, "only the compressed bytes are fetched");
        assert_eq!(
            take(&src, Format::Zip, big),
            b"0123456789abcdef".repeat(4000)
        );
        let tr = find(&entries, "Türkçe ğüşiöç.txt");
        assert_eq!(take(&src, Format::Zip, tr), b"unicode name");
    }

    #[test]
    fn listing_needs_only_the_end_of_the_file() {
        let mut files = vec![];
        let blob = vec![7u8; 3_000_000];
        files.push(("huge.bin", &blob[..], zip::CompressionMethod::Stored));
        files.push(("small.txt", b"x", zip::CompressionMethod::Stored));
        let src = counting(build(b"", &files));
        let cached = Cached::new(&src);
        let (_, entries, _) = list_any(&cached).unwrap();
        assert_eq!(entries.len(), 2);
        let read = src.2.load(std::sync::atomic::Ordering::Relaxed);
        assert!(read < 400_000, "read {read} bytes of a 3 MB archive");
    }

    #[test]
    fn finds_a_zip_behind_other_data() {
        let data = build(
            b"MZ this pretends to be a self-extracting program header",
            &[(
                "inner.txt",
                b"from the sfx",
                zip::CompressionMethod::Deflated,
            )],
        );
        let src = MemSource(data);
        let (format, entries, _) = list_any(&src).unwrap();
        assert_eq!(format, Format::Zip);
        assert_eq!(take(&src, Format::Zip, &entries[0]), b"from the sfx");
    }

    #[test]
    fn marks_encrypted_entries() {
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut out);
            let opts = zip::write::SimpleFileOptions::default()
                .with_aes_encryption(zip::AesMode::Aes256, "secret");
            w.start_file("locked.txt", opts).unwrap();
            w.write_all(b"hidden").unwrap();
            w.finish().unwrap();
        }
        let entries = list(&MemSource(out.into_inner())).unwrap();
        assert_eq!(entries[0].problem.as_deref(), Some("encrypted"));
    }

    #[test]
    fn cp437_names_decode() {
        assert_eq!(decode_name(&[0x80, b'a', 0x81], false), "Çaü");
        assert_eq!(decode_name("ğ".as_bytes(), false), "ğ");
    }
}
