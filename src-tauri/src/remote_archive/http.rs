use std::time::Duration;

use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, CONTENT_RANGE, IF_RANGE, RANGE};
use reqwest::{Client, StatusCode};

use super::Source;

pub struct HttpSource {
    pub client: Client,
    pub url: String,
    pub headers: HeaderMap,
    pub size: u64,
    pub validator: Option<String>,
    pub runtime: tokio::runtime::Handle,
}

impl HttpSource {
    async fn get(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let last = offset + len as u64 - 1;
        let mut req = self
            .client
            .get(&self.url)
            .headers(self.headers.clone())
            .header(RANGE, format!("bytes={offset}-{last}"))
            .timeout(Duration::from_secs(120));
        if let Some(v) = &self.validator {
            req = req.header(IF_RANGE, v.as_str());
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("could not connect to {}", self.url))?;
        let status = resp.status();
        if status != StatusCode::PARTIAL_CONTENT {
            if status.is_success() {
                bail!("the server sent the whole file instead of a part of it (it changed, or stopped allowing partial downloads)");
            }
            bail!("server answered HTTP {status}");
        }
        let served = resp
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().strip_prefix("bytes"))
            .and_then(|v| v.trim().split_once('-'))
            .and_then(|(s, rest)| {
                let total = rest
                    .split_once('/')
                    .and_then(|(_, t)| t.trim().parse::<u64>().ok());
                Some((s.trim().parse::<u64>().ok()?, total))
            });
        match served {
            Some((s, _)) if s != offset => {
                bail!("the server sent bytes from {s} instead of {offset}")
            }
            Some((_, Some(t))) if t != self.size => {
                bail!(
                    "the archive on the server changed size ({} → {t} bytes)",
                    self.size
                )
            }
            _ => {}
        }
        let mut out = Vec::with_capacity(len);
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("connection interrupted")?;
            let take = chunk.len().min(len - out.len());
            out.extend_from_slice(&chunk[..take]);
            if out.len() >= len {
                break;
            }
        }
        Ok(out)
    }
}

impl Source for HttpSource {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        if offset >= self.size || len == 0 {
            return Ok(Vec::new());
        }
        let len = len.min((self.size - offset) as usize);
        let mut last_error = None;
        for attempt in 0..3u64 {
            match self.runtime.block_on(self.get(offset, len)) {
                Ok(d) if d.len() == len => return Ok(d),
                Ok(d) => {
                    last_error = Some(anyhow::anyhow!(
                        "connection closed after {} of {len} bytes",
                        d.len()
                    ))
                }
                Err(e) => last_error = Some(e),
            }
            std::thread::sleep(Duration::from_millis(400 * (attempt + 1)));
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("read failed")))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::Arc;

    use super::*;
    use crate::engine::{self, Progress};
    use crate::remote_archive::{decode, list, Cached, Format, Pick};

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn takes_one_file_out_of_a_zip_on_a_web_server() {
        let mut zip_data = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut zip_data);
            let filler: Vec<u8> = (0..4_000_000u32)
                .map(|i| (i.wrapping_mul(2654435761) >> 7) as u8)
                .collect();
            let stored = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            w.start_file("filler.bin", stored).unwrap();
            w.write_all(&filler).unwrap();
            let deflated = zip::write::SimpleFileOptions::default();
            w.start_file("wanted/notes.txt", deflated).unwrap();
            w.write_all(&b"the one file we want\n".repeat(2000))
                .unwrap();
            w.finish().unwrap();
        }
        let data = zip_data.into_inner();
        let total = data.len() as u64;
        let url = engine::tests::range_server(Arc::new(data), "\"z1\"").await;
        let client = Client::new();
        let mut headers = HeaderMap::new();
        let probe = engine::probe_with_fallback(&client, &url, &mut headers)
            .await
            .unwrap();
        assert!(probe.resumable);
        let src = Arc::new(HttpSource {
            client: client.clone(),
            url: probe.final_url.clone(),
            headers: headers.clone(),
            size: total,
            validator: probe.validator.clone(),
            runtime: tokio::runtime::Handle::current(),
        });

        let s = src.clone();
        let (format, entries, _) = tokio::task::spawn_blocking(move || list(&Cached::new(&*s)))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(format, Format::Zip);
        let entry = entries
            .iter()
            .find(|e| e.path == "wanted/notes.txt")
            .unwrap()
            .clone();

        let s = src.clone();
        let e = entry.clone();
        let start = tokio::task::spawn_blocking(move || decode::data_start(&*s, &e))
            .await
            .unwrap()
            .unwrap();
        let dir = std::env::temp_dir().join(format!("mdm-http-ra-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let packed = dir.join("notes.txt.packed.part");
        let job = Arc::new(engine::Job {
            client,
            url: src.url.clone(),
            headers,
            part: packed.clone(),
            resumable: true,
            progress: Progress::new(Some(entry.fetch), &[], 4),
            cancel: tokio_util::sync::CancellationToken::new(),
            limiters: Vec::new(),
            retries: 0,
            transform: None,
            if_range: probe.validator.clone(),
            max_request: None,
            window: Some((start, total)),
        });
        assert!(engine::run(job.clone()).await.unwrap());
        let received = job
            .progress
            .received
            .load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            received < 10_000,
            "downloaded {received} bytes of a {total}-byte archive"
        );

        let out = dir.join("notes.txt");
        let pick = Pick {
            format,
            archive_size: total,
            entry,
        };
        let (s, o, p) = (src.clone(), out.clone(), packed.clone());
        tokio::task::spawn_blocking(move || decode::unpack(&pick, start, &p, &o, &*s, &|_| {}))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            std::fs::read(&out).unwrap(),
            b"the one file we want\n".repeat(2000)
        );
        assert!(!packed.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
