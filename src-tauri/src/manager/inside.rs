use super::*;
use crate::remote_archive::{self, http::HttpSource, Cached, Listing, Method, Pick};

pub fn problem_text(problem: &str) -> &'static str {
    match problem {
        "encrypted" => "this file is encrypted in the archive",
        "solid" => "this file is in a solid archive, which can only be unpacked as a whole",
        "method" => "this file uses a compression method that cannot be unpacked here",
        "fragmented" => "this file is stored in pieces spread across the archive",
        "link" => "this entry is a link, not a file",
        _ => "this file cannot be taken out of the archive on its own",
    }
}

pub fn packed_path(final_path: &Path) -> PathBuf {
    let mut s = final_path.as_os_str().to_owned();
    s.push(".packed.part");
    PathBuf::from(s)
}

impl Manager {
    async fn archive_source(
        &self,
        url: &str,
        rec: &DownloadRecord,
    ) -> Result<(HttpSource, engine::ProbeResult)> {
        let mut headers = self.request_headers(rec);
        let client = self.http.read().clone();
        let resolved = resolver::resolve(&client, url).await?;
        if resolved.transform.is_some() {
            bail!("files on this site are encrypted while they download, so the archive cannot be browsed");
        }
        let probe = engine::probe_with_fallback(&client, &resolved.url, &mut headers).await?;
        if probe
            .content_type
            .as_deref()
            .is_some_and(|c| c.starts_with("text/html"))
        {
            bail!("the link opens a web page instead of a file");
        }
        let Some(size) = probe.total.filter(|_| probe.resumable) else {
            bail!("this server does not allow partial downloads, so single files cannot be taken out of the archive. Download the whole archive instead");
        };
        let src = HttpSource {
            client,
            url: probe.final_url.clone(),
            headers,
            size,
            validator: probe.validator.clone(),
            runtime: tokio::runtime::Handle::current(),
        };
        Ok((src, probe))
    }

    pub async fn list_archive(
        &self,
        url: &str,
        referrer: Option<String>,
        cookies: &[BrowserCookie],
    ) -> Result<Listing> {
        let url = url.trim();
        if !util::web_link(url) {
            bail!("only http(s) links can be browsed");
        }
        if self.settings().https_only && util::insecure_link(url) {
            bail!("this link is not encrypted (http://). Settings → Security → Only allow encrypted (HTTPS) links blocks it");
        }
        let rec = DownloadRecord {
            referrer,
            cookie_header: cookie_header(cookies, url),
            ..blank_record(url)
        };
        let (src, probe) = self.archive_source(url, &rec).await?;
        let size = src.size;
        let (format, entries, truncated) = tokio::task::spawn_blocking(move || {
            let cached = Cached::new(&src);
            remote_archive::list(&cached)
        })
        .await??;
        let solid = entries.iter().any(|e| e.shared > 1);
        Ok(Listing {
            format,
            url: probe.final_url,
            size,
            entries,
            truncated,
            solid,
        })
    }

    pub(super) fn place_archive_pick(
        &self,
        rec: &mut DownloadRecord,
        mut pick: Pick,
        folders: bool,
        filename: Option<String>,
        base_dir: &str,
        settings: &Settings,
    ) -> Result<()> {
        let entry = &mut pick.entry;
        if entry.dir {
            bail!("a folder cannot be downloaded on its own; pick the files in it");
        }
        if let Some(p) = entry.problem.as_deref() {
            bail!("{}", problem_text(p));
        }
        if entry.method == Method::Unsupported {
            bail!("{}", problem_text("method"));
        }
        entry.path = remote_archive::clean_path(&entry.path);
        let (parent, base) = entry.path.rsplit_once('/').unwrap_or(("", &entry.path));
        let name = filename
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| base.to_string());
        let name = util::sanitize_filename(&name);
        if name.is_empty() {
            bail!("the file inside the archive has no usable name");
        }
        rec.category = util::category_for(&name);
        let dir = if folders && !parent.is_empty() {
            let mut d = PathBuf::from(base_dir);
            for part in parent.split('/') {
                let part = util::sanitize_filename(part);
                if !part.is_empty() {
                    d.push(part);
                }
            }
            d
        } else {
            self.target_dir(base_dir, rec.category, settings.organize_by_category, &name)
        };
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("could not create the folder {}", dir.display()))?;
        let path = util::unique_path(&dir, &name);
        rec.name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or(name);
        rec.dir = dir.to_string_lossy().into_owned();
        rec.total = Some(entry.size);
        rec.resumable = true;
        rec.archive = Some(pick);
        Ok(())
    }

    pub(super) async fn run_archive_entry(
        self: Arc<Self>,
        id: &str,
        rec: DownloadRecord,
        pick: Pick,
        cancel: CancellationToken,
        limiter: Arc<Limiter>,
    ) -> Result<bool> {
        let settings = self.settings();
        let (src, probe) = self.archive_source(&rec.url, &rec).await?;
        if src.size != pick.archive_size {
            bail!(
                "the archive on the server changed ({} bytes, it was {} bytes when it was browsed). Browse it again",
                src.size,
                pick.archive_size
            );
        }
        let src = Arc::new(src);
        let entry = pick.entry.clone();
        let final_path = PathBuf::from(&rec.dir).join(&rec.name);
        let packed = packed_path(&final_path);

        if entry.size == 0 {
            std::fs::write(&final_path, b"")
                .with_context(|| format!("could not create {}", final_path.display()))?;
            self.with_entry(id, |e| {
                e.rec.total = Some(0);
                e.rec.downloaded = 0;
            });
            return Ok(true);
        }

        let start = {
            let src = src.clone();
            let e = entry.clone();
            tokio::task::spawn_blocking(move || remote_archive::decode::data_start(&*src, &e))
                .await??
        };
        if start + entry.fetch > src.size {
            bail!("the archive on the server is damaged or changed");
        }

        let part_ok = tokio::fs::metadata(&packed)
            .await
            .map(|m| m.len() == entry.fetch)
            .unwrap_or(false);
        let unchanged = match (&rec.validator, &probe.validator) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        };
        let same_file = part_ok && unchanged && rec.total == Some(entry.fetch);
        let segments = if same_file {
            rec.segments.clone()
        } else {
            Vec::new()
        };
        let threads = match settings.site_limit(&rec.url) {
            Some(l) if l.max_connections > 0 => rec.threads.min(l.max_connections).max(1),
            _ => rec.threads,
        };
        let progress = Progress::new(Some(entry.fetch), &segments, threads);
        self.with_entry(id, |e| {
            e.rec.total = Some(entry.fetch);
            e.rec.resumable = true;
            e.rec.validator = probe.validator.clone();
            if !same_file {
                e.rec.segments.clear();
                e.rec.downloaded = 0;
            }
            e.live.phase = Some("downloading".into());
            if let Some(r) = e.run.as_mut() {
                r.progress = Some(progress.clone());
                r.peak = 0;
                r.last_received = 0;
                r.last_tick = Instant::now();
            }
        });
        if !same_file && packed.exists() {
            let _ = tokio::fs::remove_file(&packed).await;
        }
        let job = Arc::new(engine::Job {
            client: src.client.clone(),
            url: src.url.clone(),
            headers: src.headers.clone(),
            part: packed.clone(),
            resumable: true,
            progress: progress.clone(),
            cancel,
            limiters: vec![self.global_limit.clone(), limiter],
            retries: settings.retries,
            transform: None,
            if_range: probe.validator.clone(),
            max_request: None,
            window: Some((start, src.size)),
        });
        let done = engine::run(job).await;
        self.with_entry(id, |e| {
            e.rec.segments = progress.segments();
            e.rec.downloaded = progress.downloaded();
        });
        if !done? {
            return Ok(false);
        }

        let target = if final_path.exists() {
            util::unique_path(final_path.parent().unwrap_or(Path::new(".")), &rec.name)
        } else {
            final_path
        };
        if entry.method != Method::Stored {
            self.with_entry(id, |e| e.live.phase = Some("unpacking".into()));
            self.emit_now();
        }
        let out = util::part_path(&target);
        {
            let src = src.clone();
            let pick = pick.clone();
            let packed = packed.clone();
            let out = out.clone();
            tokio::task::spawn_blocking(move || {
                remote_archive::decode::unpack(&pick, start, &packed, &out, &*src, &|_| {})
            })
            .await??;
        }
        tokio::fs::rename(&out, &target)
            .await
            .with_context(|| format!("could not move the file into {}", target.display()))?;
        let name = target.file_name().map(|n| n.to_string_lossy().into_owned());
        self.with_entry(id, |e| {
            if let Some(n) = name {
                e.rec.name = n;
            }
            e.rec.total = Some(entry.size);
            e.rec.downloaded = entry.size;
            e.rec.segments.clear();
            e.live.phase = None;
        });
        self.verify_hash(id, &target).await;
        Ok(true)
    }
}
