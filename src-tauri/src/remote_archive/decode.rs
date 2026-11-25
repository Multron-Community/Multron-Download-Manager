use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

use anyhow::{bail, Context, Result};

use super::{Cached, Entry, Method, Pick, Source};

pub fn data_start(src: &dyn Source, entry: &Entry) -> Result<u64> {
    if entry.local_header {
        super::zip::data_start(src, entry.offset)
    } else {
        Ok(entry.offset)
    }
}

struct Checked<'a, W: Write> {
    out: W,
    written: u64,
    limit: u64,
    crc: crc32fast::Hasher,
    progress: &'a dyn Fn(u64),
}

impl<W: Write> Write for Checked<'_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.written + buf.len() as u64 > self.limit {
            return Err(std::io::Error::other(
                "the file inside the archive is larger than the archive says",
            ));
        }
        let n = self.out.write(buf)?;
        self.crc.update(&buf[..n]);
        let before = self.written >> 20;
        self.written += n as u64;
        if self.written >> 20 != before {
            (self.progress)(self.written);
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

struct Overlay<'a> {
    remote: &'a dyn Source,
    local: parking_lot::Mutex<File>,
    start: u64,
    len: u64,
}

impl Source for Overlay<'_> {
    fn size(&self) -> u64 {
        self.remote.size()
    }

    fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let end = (offset + len as u64).min(self.size());
        let local_end = self.start + self.len;
        let mut out = Vec::with_capacity((end.saturating_sub(offset)) as usize);
        let mut pos = offset;
        while pos < end {
            if pos >= self.start && pos < local_end {
                let take = (end.min(local_end) - pos) as usize;
                let mut buf = vec![0u8; take];
                let mut f = self.local.lock();
                f.seek(SeekFrom::Start(pos - self.start))?;
                f.read_exact(&mut buf)?;
                drop(f);
                out.extend(buf);
                pos += take as u64;
            } else {
                let stop = if pos < self.start {
                    end.min(self.start)
                } else {
                    end
                };
                let data = self.remote.read_at(pos, (stop - pos) as usize)?;
                if data.is_empty() {
                    break;
                }
                pos += data.len() as u64;
                out.extend(data);
            }
        }
        Ok(out)
    }
}

pub fn unpack(
    pick: &Pick,
    start: u64,
    part: &Path,
    out: &Path,
    remote: &dyn Source,
    progress: &dyn Fn(u64),
) -> Result<()> {
    let entry = &pick.entry;
    let part_len = std::fs::metadata(part)
        .with_context(|| format!("could not open {}", part.display()))?
        .len();
    if part_len != entry.fetch {
        bail!(
            "the download of the file inside the archive is incomplete ({part_len} of {} bytes)",
            entry.fetch
        );
    }
    let make = || -> Result<Checked<'_, BufWriter<File>>> {
        let f = File::create(out).with_context(|| format!("could not create {}", out.display()))?;
        Ok(Checked {
            out: BufWriter::with_capacity(1 << 20, f),
            written: 0,
            limit: entry.size,
            crc: crc32fast::Hasher::new(),
            progress,
        })
    };
    let finish = |mut w: Checked<'_, BufWriter<File>>, check_crc: bool| -> Result<()> {
        w.flush()?;
        let file = w.out.into_inner().map_err(|e| e.into_error())?;
        file.sync_all()?;
        if w.written != entry.size {
            bail!(
                "the file inside the archive came out with {} bytes instead of {}",
                w.written,
                entry.size
            );
        }
        if check_crc {
            if let Some(crc) = entry.crc {
                if w.crc.finalize() != crc {
                    bail!("the file inside the archive is damaged (CRC mismatch); the archive may have changed on the server");
                }
            }
        }
        Ok(())
    };
    let result = match entry.method {
        Method::Stored => {
            if let Some(crc) = entry.crc {
                let mut h = crc32fast::Hasher::new();
                let mut r = BufReader::with_capacity(1 << 20, File::open(part)?);
                let mut buf = vec![0u8; 1 << 20];
                loop {
                    let n = r.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    h.update(&buf[..n]);
                }
                if h.finalize() != crc {
                    bail!("the file inside the archive is damaged (CRC mismatch); the archive may have changed on the server");
                }
            }
            if entry.size != entry.fetch {
                bail!("a stored file has a different packed size");
            }
            std::fs::rename(part, out)
                .with_context(|| format!("could not move the file into {}", out.display()))?;
            return Ok(());
        }
        Method::Deflate => {
            let mut w = make()?;
            let mut r = flate2::read::DeflateDecoder::new(BufReader::with_capacity(
                1 << 20,
                File::open(part)?,
            ));
            std::io::copy(&mut r, &mut w).context("could not inflate the file")?;
            finish(w, true)
        }
        Method::Deflate64 => {
            let mut w = make()?;
            let mut r = deflate64::Deflate64Decoder::new(File::open(part)?);
            std::io::copy(&mut r, &mut w).context("could not inflate the file")?;
            finish(w, true)
        }
        Method::SevenZip => {
            let cached = Cached::new(remote);
            let overlay = Overlay {
                remote: &cached,
                local: parking_lot::Mutex::new(File::open(part)?),
                start,
                len: part_len,
            };
            let mut w = make()?;
            super::sevenz::unpack(&overlay, entry, &mut w)?;
            finish(w, false)
        }
        Method::Unsupported => bail!("this file's compression method is not supported"),
    };
    if result.is_err() {
        let _ = std::fs::remove_file(out);
    } else {
        let _ = std::fs::remove_file(part);
    }
    result
}
