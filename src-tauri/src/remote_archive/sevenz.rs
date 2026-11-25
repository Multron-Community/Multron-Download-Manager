use std::io::{Read, Write};

use anyhow::{anyhow, bail, Result};
use sevenz_rust2::{Archive, BlockDecoder, EncoderMethod, Password};

use super::{clean_path, Entry, Method, Source, SourceReader, MAX_ENTRIES};

const SIGNATURE: u64 = sevenz_rust2::SIGNATURE_HEADER_SIZE;

fn read_archive(src: &dyn Source) -> Result<Archive> {
    let mut reader = SourceReader { src, pos: 0 };
    Archive::read(&mut reader, &Password::empty()).map_err(|e| {
        let text = e.to_string();
        if text.to_ascii_lowercase().contains("password") {
            anyhow!("this 7z archive's file list is encrypted")
        } else {
            anyhow!("could not read the 7z header: {text}")
        }
    })
}

fn nt_to_unix(t: sevenz_rust2::NtTime) -> i64 {
    (u64::from(t) / 10_000_000) as i64 - 11_644_473_600
}

fn block_range(a: &Archive, block: usize) -> (u64, u64) {
    let first = a.stream_map.block_first_pack_stream_index()[block];
    let next = a
        .stream_map
        .block_first_pack_stream_index()
        .get(block + 1)
        .copied()
        .unwrap_or(a.pack_sizes().len());
    let offset = SIGNATURE + a.pack_pos() + a.stream_map.pack_stream_offsets()[first];
    let len = a.pack_sizes()[first..next.max(first)].iter().sum();
    (offset, len)
}

fn block_problem(a: &Archive, block: usize) -> Option<&'static str> {
    let known = [
        EncoderMethod::ID_COPY,
        EncoderMethod::ID_DELTA,
        EncoderMethod::ID_LZMA,
        EncoderMethod::ID_LZMA2,
        EncoderMethod::ID_PPMD,
        EncoderMethod::ID_BCJ_X86,
        EncoderMethod::ID_BCJ_PPC,
        EncoderMethod::ID_BCJ_IA64,
        EncoderMethod::ID_BCJ_ARM,
        EncoderMethod::ID_BCJ_ARM64,
        EncoderMethod::ID_BCJ_ARM_THUMB,
        EncoderMethod::ID_BCJ_SPARC,
        EncoderMethod::ID_BCJ_RISCV,
    ];
    for c in &a.blocks[block].coders {
        let id = c.encoder_method_id();
        if id == EncoderMethod::ID_AES256_SHA256 {
            return Some("encrypted");
        }
        if !known.contains(&id) {
            return Some("method");
        }
    }
    None
}

pub fn list(src: &dyn Source) -> Result<Vec<Entry>> {
    let a = read_archive(src)?;
    let mut per_block = vec![0u32; a.blocks.len()];
    for b in a.stream_map.file_block_index.iter().flatten() {
        if let Some(n) = per_block.get_mut(*b) {
            *n += 1;
        }
    }
    let mut entries = Vec::new();
    for (i, f) in a.files.iter().enumerate() {
        let path = clean_path(f.name());
        if path.is_empty() || f.is_anti_item() {
            continue;
        }
        let mut e = Entry {
            path,
            dir: f.is_directory(),
            size: if f.is_directory() { 0 } else { f.size() },
            modified: f
                .has_last_modified_date
                .then(|| nt_to_unix(f.last_modified_date())),
            crc: f.has_crc.then_some(f.crc as u32),
            index: Some(i as u32),
            ..Default::default()
        };
        if let Some(block) = a
            .stream_map
            .file_block_index
            .get(i)
            .copied()
            .flatten()
            .filter(|_| f.size() > 0)
        {
            let (offset, len) = block_range(&a, block);
            e.method = Method::SevenZip;
            e.offset = offset;
            e.fetch = len;
            e.block = Some(block as u32);
            e.shared = per_block[block];
            e.problem = block_problem(&a, block).map(str::to_string);
        } else {
            e.fetch = 0;
        }
        entries.push(e);
        if entries.len() >= MAX_ENTRIES {
            break;
        }
    }
    Ok(entries)
}

pub fn unpack(src: &dyn Source, entry: &Entry, out: &mut dyn Write) -> Result<u64> {
    let a = read_archive(src)?;
    let (Some(block), Some(index)) = (entry.block, entry.index) else {
        bail!("this 7z entry has no data");
    };
    let block = block as usize;
    let index = index as usize;
    if a.files.get(index).map(|f| clean_path(f.name())) != Some(entry.path.clone()) {
        bail!("the 7z archive changed on the server");
    }
    let mut reader = SourceReader { src, pos: 0 };
    let password = Password::empty();
    let decoder = BlockDecoder::new(1, block, &a, &password, &mut reader);
    let mut written = 0u64;
    let mut failure: Option<std::io::Error> = None;
    let target = &a.files[index];
    decoder
        .for_each_entries(&mut |f, r: &mut dyn Read| {
            let me = std::ptr::eq(f, target);
            if me {
                match std::io::copy(r, out) {
                    Ok(n) => written = n,
                    Err(e) => failure = Some(e),
                }
                return Ok(false);
            }
            std::io::copy(r, &mut std::io::sink())?;
            Ok(true)
        })
        .map_err(|e| anyhow!("could not unpack the 7z file: {e}"))?;
    if let Some(e) = failure {
        return Err(e.into());
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{find, take};
    use super::super::{list as list_any, Format, MemSource};

    fn build(solid: bool) -> Vec<u8> {
        let dir = std::env::temp_dir().join(format!("mdm-7z-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), b"first file in the 7z").unwrap();
        std::fs::write(dir.join("sub/b.log"), b"second ".repeat(500)).unwrap();
        std::fs::write(dir.join("empty.dat"), b"").unwrap();
        let out = dir.with_extension("7z");
        let mut w = sevenz_rust2::ArchiveWriter::create(&out).unwrap();
        if solid {
            w.push_source_path(&dir, |_| true).unwrap();
        } else {
            w.push_source_path_non_solid(&dir, |_| true).unwrap();
        }
        w.finish().unwrap();
        let data = std::fs::read(&out).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&out);
        data
    }

    #[test]
    fn non_solid_7z_fetches_only_the_files_block() {
        let data = build(false);
        let total = data.len() as u64;
        let src = MemSource(data);
        let (format, entries, _) = list_any(&src).unwrap();
        assert_eq!(format, Format::SevenZip);
        let a = find(&entries, "a.txt");
        assert_eq!(a.shared, 1);
        assert!(a.fetch < total / 2);
        assert_eq!(take(&src, format, a), b"first file in the 7z");
        let b = find(&entries, "sub/b.log");
        assert_eq!(take(&src, format, b), b"second ".repeat(500));
        let empty = find(&entries, "empty.dat");
        assert_eq!(empty.fetch, 0);
    }

    #[test]
    fn solid_7z_files_share_a_block() {
        let src = MemSource(build(true));
        let (format, entries, _) = list_any(&src).unwrap();
        let b = find(&entries, "sub/b.log");
        assert!(b.shared >= 2, "{b:?}");
        assert_eq!(take(&src, format, b), b"second ".repeat(500));
        assert_eq!(
            take(&src, format, find(&entries, "a.txt")),
            b"first file in the 7z"
        );
    }
}
