use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const API: &str = "https://www.virustotal.com/api/v3";
const DIRECT_LIMIT: u64 = 32 * 1024 * 1024;
const MAX_UPLOAD: u64 = 650 * 1024 * 1024;
const POLL_EVERY: Duration = Duration::from_secs(8);
const POLL_FOR: Duration = Duration::from_secs(150);

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VtResult {
    pub state: String,
    pub name: String,
    pub sha256: String,
    pub link: String,
    pub malicious: u64,
    pub suspicious: u64,
    pub harmless: u64,
    pub undetected: u64,
    pub engines: u64,
    pub label: Option<String>,
}

pub fn link(sha: &str) -> String {
    format!("https://www.virustotal.com/gui/file/{sha}")
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).context("could not open the file")?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

pub fn read_stats(sha: &str, name: &str, attrs: &Value, state: &str) -> VtResult {
    let stats = attrs
        .get("last_analysis_stats")
        .or_else(|| attrs.get("stats"))
        .cloned()
        .unwrap_or_default();
    let n = |k: &str| stats.get(k).and_then(Value::as_u64).unwrap_or(0);
    let malicious = n("malicious");
    let suspicious = n("suspicious");
    let harmless = n("harmless");
    let undetected = n("undetected");
    let engines = malicious
        + suspicious
        + harmless
        + undetected
        + n("timeout")
        + n("type-unsupported")
        + n("failure");
    VtResult {
        state: state.into(),
        name: name.into(),
        sha256: sha.into(),
        link: link(sha),
        malicious,
        suspicious,
        harmless,
        undetected,
        engines,
        label: attrs
            .get("popular_threat_classification")
            .and_then(|c| c.get("suggested_threat_label"))
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn check_status(resp: &reqwest::Response) -> Result<()> {
    match resp.status().as_u16() {
        200..=299 => Ok(()),
        401 | 403 => bail!("VirusTotal did not accept the API key. Check it in Settings → VirusTotal"),
        429 => bail!("the VirusTotal API limit was reached (the free key allows 4 requests a minute and 500 a day). Try again later"),
        code => bail!("VirusTotal answered with HTTP {code}"),
    }
}

pub async fn lookup(
    client: &reqwest::Client,
    key: &str,
    sha: &str,
    name: &str,
) -> Result<VtResult> {
    let resp = client
        .get(format!("{API}/files/{sha}"))
        .header("x-apikey", key)
        .send()
        .await
        .context("could not reach VirusTotal")?;
    if resp.status().as_u16() == 404 {
        return Ok(VtResult {
            state: "not_found".into(),
            name: name.into(),
            sha256: sha.into(),
            link: link(sha),
            ..Default::default()
        });
    }
    check_status(&resp)?;
    let body: Value = resp.json().await?;
    let attrs = body
        .pointer("/data/attributes")
        .cloned()
        .unwrap_or_default();
    Ok(read_stats(sha, name, &attrs, "found"))
}

async fn multipart(
    client: &reqwest::Client,
    key: &str,
    url: &str,
    path: &Path,
    name: &str,
) -> Result<Value> {
    let boundary = format!("mdm{}", uuid::Uuid::new_v4().simple());
    let safe: String = name
        .chars()
        .map(|c| if c == '"' || c.is_control() { '_' } else { c })
        .collect();
    let head = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{safe}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    );
    let tail = format!("\r\n--{boundary}--\r\n");
    let size = head.len() as u64 + tokio::fs::metadata(path).await?.len() + tail.len() as u64;
    let file = tokio::fs::File::open(path).await?;
    let stream =
        futures_util::stream::once(
            async move { Ok::<_, std::io::Error>(bytes::Bytes::from(head)) },
        )
        .chain(tokio_util::io::ReaderStream::new(file))
        .chain(futures_util::stream::once(async move {
            Ok(bytes::Bytes::from(tail))
        }));
    let resp = client
        .post(url)
        .header("x-apikey", key)
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header("content-length", size)
        .body(reqwest::Body::wrap_stream(stream))
        .timeout(Duration::from_secs(1800))
        .send()
        .await
        .context("the upload to VirusTotal failed")?;
    check_status(&resp)?;
    Ok(resp.json().await?)
}

pub async fn upload(
    client: &reqwest::Client,
    key: &str,
    path: &Path,
    sha: &str,
    name: &str,
) -> Result<VtResult> {
    let size = std::fs::metadata(path)?.len();
    if size > MAX_UPLOAD {
        bail!("VirusTotal accepts files up to 650 MB");
    }
    let url = if size > DIRECT_LIMIT {
        let resp = client
            .get(format!("{API}/files/upload_url"))
            .header("x-apikey", key)
            .send()
            .await?;
        check_status(&resp)?;
        let v: Value = resp.json().await?;
        v.get("data")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| anyhow!("VirusTotal did not give an upload address"))?
    } else {
        format!("{API}/files")
    };
    let sent = multipart(client, key, &url, path, name).await?;
    let analysis = sent
        .pointer("/data/id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("VirusTotal did not confirm the upload"))?
        .to_string();
    let started = std::time::Instant::now();
    while started.elapsed() < POLL_FOR {
        tokio::time::sleep(POLL_EVERY).await;
        let resp = client
            .get(format!("{API}/analyses/{analysis}"))
            .header("x-apikey", key)
            .send()
            .await?;
        if resp.status().as_u16() == 429 {
            continue;
        }
        check_status(&resp)?;
        let v: Value = resp.json().await?;
        let attrs = v.pointer("/data/attributes").cloned().unwrap_or_default();
        if attrs.get("status").and_then(Value::as_str) == Some("completed") {
            return Ok(read_stats(sha, name, &attrs, "found"));
        }
    }
    Ok(VtResult {
        state: "queued".into(),
        name: name.into(),
        sha256: sha.into(),
        link: link(sha),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_analysis_stats() {
        let attrs = serde_json::json!({
            "last_analysis_stats": { "malicious": 5, "suspicious": 1, "harmless": 0, "undetected": 60, "timeout": 2 },
            "popular_threat_classification": { "suggested_threat_label": "trojan.test/x" }
        });
        let r = read_stats("ab", "a.exe", &attrs, "found");
        assert_eq!(
            (r.malicious, r.suspicious, r.undetected, r.engines),
            (5, 1, 60, 68)
        );
        assert_eq!(r.label.as_deref(), Some("trojan.test/x"));
        assert_eq!(r.link, "https://www.virustotal.com/gui/file/ab");
    }

    #[test]
    fn hashes_files() {
        let p = std::env::temp_dir().join(format!("mdm-vt-{}.txt", std::process::id()));
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(
            sha256_file(&p).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(&p);
    }
}
