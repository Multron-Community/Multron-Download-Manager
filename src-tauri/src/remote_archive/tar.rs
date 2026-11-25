use anyhow::{bail, Result};

use super::{clean_path, over_budget, Entry, Source, MAX_ENTRIES};

const BLOCK: u64 = 512;

fn octal(field: &[u8]) -> Option<u64> {
    if field.first().is_some_and(|b| b & 0x80 != 0) {
        let mut v: u64 = (field[0] & 0x7F) as u64;
        for &b in &field[1..] {
            v = v.checked_mul(256)?.checked_add(b as u64)?;
        }
        return Some(v);
    }
    let s: String = field
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect();
    let s = s.trim();
    if s.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(s, 8).ok()
}

fn text(field: &[u8]) -> String {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

pub fn valid_header(h: &[u8]) -> bool {
    if h.len() < 512 || h[..512].iter().all(|&b| b == 0) {
        return false;
    }
    let Some(stored) = octal(&h[148..156]) else {
        return false;
    };
    let sum: u64 = h[..512]
        .iter()
        .enumerate()
        .map(|(i, &b)| {
            if (148..156).contains(&i) {
                32
            } else {
                b as u64
            }
        })
        .sum();
    sum == stored
}

fn pax(data: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = data;
    while let Some(space) = rest.iter().position(|&b| b == b' ') {
        let Ok(len) = std::str::from_utf8(&rest[..space])
            .unwrap_or("")
            .parse::<usize>()
        else {
            break;
        };
        if len <= space + 1 || len > rest.len() {
            break;
        }
        let record = &rest[space + 1..len - 1];
        if let Some(eq) = record.iter().position(|&b| b == b'=') {
            out.push((
                String::from_utf8_lossy(&record[..eq]).into_owned(),
                String::from_utf8_lossy(&record[eq + 1..]).into_owned(),
            ));
        }
        rest = &rest[len..];
    }
    out
}

pub fn list(src: &dyn Source) -> Result<(Vec<Entry>, bool)> {
    let size = src.size();
    let mut entries = Vec::new();
    let mut pos = 0u64;
    let mut long_name: Option<String> = None;
    let mut pax_local: Vec<(String, String)> = Vec::new();
    while pos + BLOCK <= size {
        let h = match src.read_at(pos, BLOCK as usize) {
            Ok(h) => h,
            Err(e) if over_budget(&e) && !entries.is_empty() => return Ok((entries, true)),
            Err(e) => return Err(e),
        };
        if h.iter().all(|&b| b == 0) {
            break;
        }
        if !valid_header(&h) {
            if entries.is_empty() {
                bail!("this TAR file is damaged");
            }
            break;
        }
        let mut file_size = octal(&h[124..136]).unwrap_or(0);
        let mtime = octal(&h[136..148]).map(|t| t as i64);
        let kind = h[156];
        let data = pos + BLOCK;
        let next = data + file_size.div_ceil(BLOCK) * BLOCK;
        match kind {
            b'L' | b'x' => {
                let body = super::read_exact_at(src, data, file_size.min(1 << 20) as usize)?;
                if kind == b'L' {
                    long_name = Some(text(&body));
                } else {
                    pax_local = pax(&body);
                }
                pos = next;
                continue;
            }
            b'g' | b'K' => {
                pos = next;
                continue;
            }
            _ => {}
        }
        let mut name = long_name.take().unwrap_or_else(|| {
            let base = text(&h[0..100]);
            let prefix = if &h[257..262] == b"ustar" {
                text(&h[345..500])
            } else {
                String::new()
            };
            if prefix.is_empty() {
                base
            } else {
                format!("{prefix}/{base}")
            }
        });
        let mut modified = mtime;
        for (k, v) in pax_local.drain(..) {
            match k.as_str() {
                "path" => name = v,
                "size" => file_size = v.parse().unwrap_or(file_size),
                "mtime" => modified = v.split('.').next().and_then(|s| s.parse().ok()),
                _ => {}
            }
        }
        let next = data + file_size.div_ceil(BLOCK) * BLOCK;
        let path = clean_path(&name);
        pos = next;
        if path.is_empty() {
            continue;
        }
        match kind {
            b'5' => entries.push(Entry {
                path,
                dir: true,
                modified,
                ..Default::default()
            }),
            b'0' | 0 | b'7' => {
                let mut e = Entry::file(path, file_size);
                e.offset = data;
                e.modified = modified;
                entries.push(e);
            }
            b'1' | b'2' => {
                let mut e = Entry::file(path, 0);
                e.modified = modified;
                e.problem = Some("link".into());
                entries.push(e);
            }
            b'S' => {
                let mut e = Entry::file(path, file_size);
                e.problem = Some("fragmented".into());
                entries.push(e);
            }
            _ => {}
        }
        if entries.len() >= MAX_ENTRIES {
            return Ok((entries, true));
        }
    }
    Ok((entries, false))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{counting, find, fixture, take};
    use super::super::{list as list_any, Cached, Format, MemSource};

    fn long() -> String {
        format!("docs/{}.txt", "x".repeat(120))
    }

    fn check(name: &str, with_long: bool) {
        let Some(data) = fixture(name) else {
            return;
        };
        let src = MemSource(data);
        let (format, entries, truncated) = list_any(&src).unwrap();
        assert_eq!(format, Format::Tar, "{name}");
        assert!(!truncated);
        assert_eq!(
            take(&src, format, find(&entries, "hello.txt")),
            b"Hello from the ISO fixture"
        );
        assert_eq!(
            take(&src, format, find(&entries, "docs/deep/nested.bin")),
            b"nested file"
        );
        assert!(find(&entries, "docs/deep").dir);
        if with_long {
            assert_eq!(
                take(&src, format, find(&entries, &long())),
                b"long name content"
            );
            assert_eq!(
                find(&entries, "hello.txt").modified,
                Some(1_714_554_000),
                "GNU tar read the date as local time (UTC+3)"
            );
        }
    }

    #[test]
    fn gnu_tar_with_long_names() {
        check("gnu.tar.gz", true);
    }

    #[test]
    fn pax_tar_with_long_names() {
        check("pax.tar.gz", true);
    }

    #[test]
    fn ustar() {
        check("ustar.tar.gz", false);
    }

    #[test]
    fn stops_early_instead_of_reading_everything() {
        let mut tar = Vec::new();
        for i in 0..400 {
            let mut h = vec![0u8; 512];
            let name = format!("f{i}.bin");
            h[..name.len()].copy_from_slice(name.as_bytes());
            h[100..107].copy_from_slice(b"0000644");
            h[124..135].copy_from_slice(format!("{:011o}", 300_000).as_bytes());
            h[136..147].copy_from_slice(b"00000000000");
            h[156] = b'0';
            h[257..263].copy_from_slice(b"ustar\0");
            h[148..156].copy_from_slice(b"        ");
            let sum: u32 = h.iter().map(|&b| b as u32).sum();
            h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
            tar.extend(h);
            tar.extend(vec![1u8; 300_000usize.div_ceil(512) * 512]);
        }
        tar.extend(vec![0u8; 1024]);
        let src = counting(tar);
        let cached = Cached::with_budget(&src, 8 * 1024 * 1024, 1000);
        let (_, entries, truncated) = list_any(&cached).unwrap();
        assert!(truncated);
        assert!(!entries.is_empty() && entries.len() < 400);
    }
}
