use std::sync::Arc;

use aes::cipher::{BlockDecrypt, KeyInit, KeyIvInit, StreamCipher, StreamCipherSeek};
use aes::Aes128;
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use reqwest::header::CONTENT_TYPE;
use reqwest::Client;
use serde_json::Value;
use url::Url;

use crate::engine::ChunkTransform;

pub struct Resolved {
    pub url: String,
    pub filename: Option<String>,
    pub size: Option<u64>,
    pub transform: Option<Arc<dyn ChunkTransform>>,
    pub service: Option<&'static str>,
}

impl Resolved {
    fn direct(url: impl Into<String>, service: Option<&'static str>) -> Self {
        Self {
            url: url.into(),
            filename: None,
            size: None,
            transform: None,
            service,
        }
    }
}

pub fn service_name(url: &str) -> Option<&'static str> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.trim_start_matches("www.");
    match host {
        "drive.google.com" | "docs.google.com" | "drive.usercontent.google.com" => {
            Some("Google Drive")
        }
        "mega.nz" | "mega.co.nz" | "mega.io" => Some("MEGA"),
        "dropbox.com" | "dl.dropboxusercontent.com" => Some("Dropbox"),
        "mediafire.com" => Some("MediaFire"),
        "pixeldrain.com" => Some("Pixeldrain"),
        "github.com" => Some("GitHub"),
        _ => None,
    }
}

pub async fn resolve(client: &Client, url: &str) -> Result<Resolved> {
    let Ok(parsed) = Url::parse(url) else {
        return Ok(Resolved::direct(url, None));
    };
    let host = parsed
        .host_str()
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_string();
    match host.as_str() {
        "mega.nz" | "mega.co.nz" | "mega.io" => resolve_mega(client, &parsed).await,
        "drive.google.com" | "docs.google.com" => resolve_google(client, &parsed).await,
        "dropbox.com" => {
            let mut u = parsed.clone();
            let pairs: Vec<(String, String)> = parsed
                .query_pairs()
                .filter(|(k, _)| k != "dl")
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            u.query_pairs_mut()
                .clear()
                .extend_pairs(pairs)
                .append_pair("dl", "1");
            Ok(Resolved::direct(u.to_string(), Some("Dropbox")))
        }
        "pixeldrain.com" => {
            let segs: Vec<&str> = parsed
                .path_segments()
                .map(|s| s.collect())
                .unwrap_or_default();
            match segs.as_slice() {
                ["u", id, ..] | ["api", "file", id, ..] => Ok(Resolved::direct(
                    format!("https://pixeldrain.com/api/file/{id}?download"),
                    Some("Pixeldrain"),
                )),
                _ => Ok(Resolved::direct(url, Some("Pixeldrain"))),
            }
        }
        "github.com" => {
            let segs: Vec<&str> = parsed
                .path_segments()
                .map(|s| s.collect())
                .unwrap_or_default();
            if segs.len() > 4 && segs[2] == "blob" {
                let raw = format!(
                    "https://raw.githubusercontent.com/{}/{}/{}",
                    segs[0],
                    segs[1],
                    segs[3..].join("/")
                );
                return Ok(Resolved::direct(raw, Some("GitHub")));
            }
            Ok(Resolved::direct(url, None))
        }
        "mediafire.com" => resolve_mediafire(client, url).await,
        _ => Ok(Resolved::direct(url, None)),
    }
}

fn google_file_id(url: &Url) -> Option<String> {
    let segs: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
    if let Some(pos) = segs.iter().position(|s| *s == "d") {
        if let Some(id) = segs.get(pos + 1) {
            return Some(id.to_string());
        }
    }
    url.query_pairs()
        .find(|(k, _)| k == "id")
        .map(|(_, v)| v.into_owned())
}

async fn resolve_google(client: &Client, url: &Url) -> Result<Resolved> {
    let Some(id) = google_file_id(url) else {
        return Ok(Resolved::direct(url.as_str(), Some("Google Drive")));
    };
    let path = url.path();
    let export = if path.starts_with("/document/") {
        Some(("document", "docx"))
    } else if path.starts_with("/spreadsheets/") {
        Some(("spreadsheets", "xlsx"))
    } else if path.starts_with("/presentation/") {
        Some(("presentation", "pptx"))
    } else {
        None
    };
    if let Some((kind, fmt)) = export {
        return Ok(Resolved::direct(
            format!("https://docs.google.com/{kind}/d/{id}/export?format={fmt}"),
            Some("Google Drive"),
        ));
    }

    let direct =
        format!("https://drive.usercontent.google.com/download?id={id}&export=download&confirm=t");
    let resp = client.get(&direct).send().await?;
    let is_html = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.starts_with("text/html"))
        .unwrap_or(false);
    if !is_html {
        drop(resp);
        return Ok(Resolved::direct(direct, Some("Google Drive")));
    }
    let html = resp.text().await.unwrap_or_default();
    if let Some(form_url) = google_form_url(&html) {
        return Ok(Resolved::direct(form_url, Some("Google Drive")));
    }
    if html.contains("Quota exceeded") || html.contains("Too many users") {
        bail!("Google Drive: download quota exceeded for this file, try again later");
    }
    if html.contains("ServiceLogin") || html.contains("accounts.google.com") {
        bail!("Google Drive: this file is private. Use the browser extension so your session cookies are sent along");
    }
    Ok(Resolved::direct(direct, Some("Google Drive")))
}

fn google_form_url(html: &str) -> Option<String> {
    let form_start = html.find("id=\"download-form\"")?;
    let form_tag_start = html[..form_start].rfind("<form")?;
    let form_end = html[form_start..].find("</form>")? + form_start;
    let form = &html[form_tag_start..form_end];
    let action = attr(form, "action")?;
    let mut url = Url::parse(&html_unescape(&action)).ok()?;
    let mut rest = form;
    while let Some(i) = rest.find("<input") {
        let tag_end = rest[i..].find('>').map(|e| i + e).unwrap_or(rest.len());
        let tag = &rest[i..tag_end];
        if let (Some(name), Some(value)) = (attr(tag, "name"), attr(tag, "value")) {
            url.query_pairs_mut()
                .append_pair(&html_unescape(&name), &html_unescape(&value));
        }
        rest = &rest[tag_end..];
    }
    Some(url.to_string())
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

async fn resolve_mediafire(client: &Client, url: &str) -> Result<Resolved> {
    let html = client.get(url).send().await?.text().await?;
    if let Some(i) = html.find("data-scrambled-url=\"") {
        let start = i + "data-scrambled-url=\"".len();
        if let Some(end) = html[start..].find('"') {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(&html[start..start + end])
                .ok()
                .and_then(|b| String::from_utf8(b).ok());
            if let Some(link) = decoded {
                return Ok(Resolved::direct(link, Some("MediaFire")));
            }
        }
    }
    let button = html.find("id=\"downloadButton\"").unwrap_or(0);
    let link = html[button..]
        .find("href=\"https://download")
        .map(|i| button + i + "href=\"".len())
        .and_then(|start| html[start..].find('"').map(|end| &html[start..start + end]))
        .ok_or_else(|| anyhow!("MediaFire: could not find the download link on the page"))?;
    Ok(Resolved::direct(html_unescape(link), Some("MediaFire")))
}

fn mega_link(url: &Url) -> Option<(String, String)> {
    let fragment = url.fragment().unwrap_or_default();
    if let Some(rest) = fragment.strip_prefix('!') {
        let mut parts = rest.splitn(2, '!');
        let id = parts.next()?.to_string();
        let key = parts.next()?.to_string();
        return Some((id, key));
    }
    let segs: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
    if segs.first() == Some(&"file") {
        let id = segs.get(1)?.to_string();
        return Some((id, fragment.to_string()));
    }
    None
}

fn b64url(s: &str) -> Result<Vec<u8>> {
    let cleaned: String = s
        .trim()
        .trim_end_matches('=')
        .chars()
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            c => c,
        })
        .collect();
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cleaned)
        .context("invalid base64 in MEGA link")
}

pub struct MegaCipher {
    key: [u8; 16],
    nonce: [u8; 8],
}

type Aes128Ctr = ctr::Ctr128BE<Aes128>;

impl ChunkTransform for MegaCipher {
    fn apply(&self, data: &mut [u8], offset: u64) {
        let mut iv = [0u8; 16];
        iv[..8].copy_from_slice(&self.nonce);
        let mut cipher = Aes128Ctr::new(&self.key.into(), &iv.into());
        cipher.seek(offset);
        cipher.apply_keystream(data);
    }
}

async fn resolve_mega(client: &Client, url: &Url) -> Result<Resolved> {
    let (id, key) = mega_link(url).ok_or_else(|| {
        anyhow!("unsupported MEGA link (only file links with a key are supported, folders are not)")
    })?;
    let raw = b64url(&key)?;
    if raw.len() != 32 {
        bail!("MEGA link key is incomplete");
    }
    let mut k = [0u8; 16];
    for i in 0..16 {
        k[i] = raw[i] ^ raw[i + 16];
    }
    let mut nonce = [0u8; 8];
    nonce.copy_from_slice(&raw[16..24]);

    let body = serde_json::json!([{ "a": "g", "g": 1, "ssl": 2, "p": id }]);
    let seq = crate::util::now_ms() % 1_000_000_000;
    let resp: Value = client
        .post(format!("https://g.api.mega.co.nz/cs?id={seq}"))
        .json(&body)
        .send()
        .await?
        .json()
        .await
        .context("unexpected answer from MEGA")?;
    let entry = match &resp {
        Value::Array(items) => items.first().cloned().unwrap_or(Value::Null),
        Value::Number(n) => bail!("MEGA error {n}"),
        other => other.clone(),
    };
    if let Some(code) = entry.as_i64() {
        bail!(match code {
            -9 => "MEGA: file not found or removed".to_string(),
            -11 => "MEGA: access denied".to_string(),
            -16 => "MEGA: file is blocked".to_string(),
            -17 | -18 => "MEGA: temporarily unavailable or transfer quota exceeded".to_string(),
            c => format!("MEGA error {c}"),
        });
    }
    let link = entry
        .get("g")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("MEGA did not return a download URL (transfer quota?)"))?
        .to_string();
    let size = entry.get("s").and_then(Value::as_u64);
    let filename = entry
        .get("at")
        .and_then(Value::as_str)
        .and_then(|at| mega_decrypt_attributes(at, &k).ok());
    Ok(Resolved {
        url: link,
        filename,
        size,
        transform: Some(Arc::new(MegaCipher { key: k, nonce })),
        service: Some("MEGA"),
    })
}

fn mega_decrypt_attributes(at: &str, key: &[u8; 16]) -> Result<String> {
    let mut data = b64url(at)?;
    if data.len() % 16 != 0 {
        bail!("bad attribute length");
    }
    let cipher = Aes128::new(key.into());
    let mut prev = [0u8; 16];
    for block in data.chunks_mut(16) {
        let mut current = [0u8; 16];
        current.copy_from_slice(block);
        let ga = aes::Block::from_mut_slice(block);
        cipher.decrypt_block(ga);
        for (b, p) in block.iter_mut().zip(prev.iter()) {
            *b ^= p;
        }
        prev = current;
    }
    let text = String::from_utf8_lossy(&data);
    let json = text
        .strip_prefix("MEGA")
        .ok_or_else(|| anyhow!("wrong MEGA key"))?
        .trim_end_matches('\0');
    let v: Value = serde_json::from_str(json)?;
    v.get("n")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("no file name"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mega_links() {
        let u = Url::parse("https://mega.nz/file/AbCd1234#keykeykey").unwrap();
        assert_eq!(mega_link(&u), Some(("AbCd1234".into(), "keykeykey".into())));
        let u = Url::parse("https://mega.nz/#!AbCd1234!keykeykey").unwrap();
        assert_eq!(mega_link(&u), Some(("AbCd1234".into(), "keykeykey".into())));
    }

    #[test]
    fn mega_ctr_is_seekable() {
        let c = MegaCipher {
            key: [7; 16],
            nonce: [3; 8],
        };
        let mut whole = vec![0u8; 100];
        c.apply(&mut whole, 0);
        let mut tail = vec![0u8; 63];
        c.apply(&mut tail, 37);
        assert_eq!(&whole[37..], &tail[..]);
    }

    #[test]
    fn google_ids() {
        let u = Url::parse("https://drive.google.com/file/d/1AbC/view?usp=sharing").unwrap();
        assert_eq!(google_file_id(&u).as_deref(), Some("1AbC"));
        let u = Url::parse("https://drive.google.com/open?id=XYZ").unwrap();
        assert_eq!(google_file_id(&u).as_deref(), Some("XYZ"));
    }

    #[test]
    fn google_form() {
        let html = r#"<form id="download-form" action="https://drive.usercontent.google.com/download" method="get"><input type="hidden" name="id" value="abc"><input type="hidden" name="confirm" value="t"><input type="hidden" name="uuid" value="u-1"></form>"#;
        let url = google_form_url(html).unwrap();
        assert_eq!(
            url,
            "https://drive.usercontent.google.com/download?id=abc&confirm=t&uuid=u-1"
        );
    }
}
