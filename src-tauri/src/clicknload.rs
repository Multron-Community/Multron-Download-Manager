use std::collections::HashMap;
use std::sync::Arc;

use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};
use axum::extract::{Form, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use base64::Engine;

use crate::manager::{Manager, Pending};
use crate::model::{BatchItem, PageBatch};

pub const PORT: u16 = 9666;
const MAX_LINKS: usize = 2000;
const MIN_GAP: std::time::Duration = std::time::Duration::from_millis(1500);

static LAST: parking_lot::Mutex<Option<std::time::Instant>> = parking_lot::Mutex::new(None);

pub fn local_host(headers: &HeaderMap) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    let name = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(&host);
    matches!(name, "127.0.0.1" | "localhost" | "[::1]")
}

fn sender(headers: &HeaderMap, form: &HashMap<String, String>) -> String {
    headers
        .get(header::ORIGIN)
        .or_else(|| headers.get(header::REFERER))
        .and_then(|h| h.to_str().ok())
        .map(str::to_string)
        .or_else(|| form.get("source").cloned())
        .unwrap_or_default()
}

fn too_soon() -> bool {
    let mut last = LAST.lock();
    let now = std::time::Instant::now();
    if last.is_some_and(|t| now.duration_since(t) < MIN_GAP) {
        return true;
    }
    *last = Some(now);
    false
}

pub fn start(mgr: Arc<Manager>) {
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(("127.0.0.1", PORT)).await {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("Click'n'Load could not listen on 127.0.0.1:{PORT} (is JDownloader running?): {e}");
                return;
            }
        };
        let app = Router::new()
            .route("/", get(alive))
            .route("/jdcheck.js", get(jdcheck))
            .route("/crossdomain.xml", get(crossdomain))
            .route("/flash", get(alive).post(alive))
            .route("/flash/add", post(add))
            .route("/flash/addcrypted2", post(add_crypted))
            .with_state(mgr);
        if let Err(e) = axum::serve(listener, app).await {
            tracing::warn!("Click'n'Load stopped: {e}");
        }
    });
}

fn text(body: &'static str, kind: &'static str) -> Response {
    let mut r = (StatusCode::OK, body).into_response();
    r.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(kind));
    r.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    r
}

async fn alive() -> Response {
    text("JDownloader\r\n", "text/plain")
}

async fn jdcheck() -> Response {
    text(
        "jdownloader=true;\nvar version='17461';\n",
        "text/javascript",
    )
}

async fn crossdomain() -> Response {
    text(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE cross-domain-policy SYSTEM \"http://www.macromedia.com/xml/dtds/cross-domain-policy.dtd\">\n<cross-domain-policy>\n<allow-access-from domain=\"*\" />\n</cross-domain-policy>\n",
        "text/xml",
    )
}

pub fn links_in(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in text.split(|c: char| c.is_whitespace() || c == '"' || c == '<' || c == '>') {
        let w = word.trim_matches(|c| c == '\0' || c == ',' || c == ';');
        let lower = w.to_ascii_lowercase();
        if (lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("magnet:?"))
            && !out.iter().any(|x| x == w)
        {
            out.push(w.to_string());
            if out.len() >= MAX_LINKS {
                break;
            }
        }
    }
    out
}

pub fn key_from_jk(jk: &str) -> Option<[u8; 16]> {
    let start = jk.find("return")? + "return".len();
    let rest = jk[start..].trim_start();
    let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let inner = &rest[1..];
    let end = inner.find(quote)?;
    let bytes = hex::decode(inner[..end].trim()).ok()?;
    bytes.get(..16)?.try_into().ok()
}

pub fn decrypt(crypted: &str, key: [u8; 16]) -> Option<String> {
    let cleaned: String = crypted.chars().filter(|c| !c.is_whitespace()).collect();
    let data = base64::engine::general_purpose::STANDARD
        .decode(cleaned.replace(' ', "+"))
        .ok()?;
    if data.is_empty() || data.len() % 16 != 0 {
        return None;
    }
    let cipher = aes::Aes128::new(GenericArray::from_slice(&key));
    let mut prev = key;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(16) {
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.decrypt_block(&mut block);
        out.extend(block.iter().zip(prev.iter()).map(|(a, b)| a ^ b));
        prev.copy_from_slice(chunk);
    }
    let s = String::from_utf8_lossy(&out)
        .trim_end_matches('\0')
        .to_string();
    Some(s)
}

fn passwords_in(form: &HashMap<String, String>) -> Vec<String> {
    form.get("passwords")
        .map(|p| {
            p.lines()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn hand_over(
    mgr: &Manager,
    headers: &HeaderMap,
    links: Vec<String>,
    form: &HashMap<String, String>,
) -> Response {
    let s = mgr.settings();
    let links: Vec<String> = if s.block_private_links {
        links
            .into_iter()
            .filter(|l| !crate::util::private_network_link(l))
            .collect()
    } else {
        links
    };
    if links.is_empty() || !s.click_n_load || !local_host(headers) {
        return text("failed\r\n", "text/plain");
    }
    let from = sender(headers, form);
    if s.click_n_load_known_only && !is_crypter(&from) {
        tracing::info!("Click'n'Load from {from} was ignored: not a known link-protection site");
        return text("failed\r\n", "text/plain");
    }
    if too_soon() {
        return text("failed\r\n", "text/plain");
    }
    let passwords = passwords_in(form);
    if !passwords.is_empty() {
        mgr.remember_extract_passwords(&passwords);
    }
    let source = form
        .get("source")
        .or_else(|| form.get("referer"))
        .cloned()
        .unwrap_or_default();
    let title = form
        .get("package")
        .cloned()
        .filter(|p| !p.trim().is_empty())
        .or_else(|| {
            url::Url::parse(&source)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
        })
        .unwrap_or_else(|| "Click'n'Load".into());
    let items = links
        .into_iter()
        .map(|url| BatchItem {
            kind: if url.to_ascii_lowercase().starts_with("magnet:") {
                "torrent".into()
            } else {
                "auto".into()
            },
            media_type: "file".into(),
            name: url
                .rsplit('/')
                .find(|s| !s.is_empty())
                .filter(|_| url.starts_with("http"))
                .map(|s| s.split('?').next().unwrap_or(s).to_string()),
            referrer: (!source.is_empty()).then(|| source.clone()),
            url,
            ..Default::default()
        })
        .collect();
    mgr.push_pending(Pending::Batch {
        batch: Box::new(PageBatch {
            page_url: source,
            page_title: Some(title),
            items,
            user_agent: None,
            cookies: Vec::new(),
        }),
    });
    text("success\r\n", "text/plain")
}

async fn add(
    State(mgr): State<Arc<Manager>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let links = links_in(form.get("urls").map(String::as_str).unwrap_or_default());
    hand_over(&mgr, &headers, links, &form)
}

async fn add_crypted(
    State(mgr): State<Arc<Manager>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let key = form.get("jk").and_then(|jk| key_from_jk(jk));
    let plain = match (key, form.get("crypted")) {
        (Some(k), Some(c)) => decrypt(c, k),
        _ => None,
    };
    let Some(plain) = plain else {
        return text("failed\r\n", "text/plain");
    };
    hand_over(&mgr, &headers, links_in(&plain), &form)
}

pub fn captcha_in(html: &str) -> Option<&'static str> {
    let l = html.to_ascii_lowercase();
    if l.contains("cf-turnstile") || l.contains("challenges.cloudflare.com/turnstile") {
        Some("Cloudflare Turnstile")
    } else if l.contains("cf-chl-")
        || l.contains("challenge-platform")
        || l.contains("just a moment...")
    {
        Some("Cloudflare")
    } else if l.contains("g-recaptcha")
        || l.contains("google.com/recaptcha")
        || l.contains("recaptcha/api")
    {
        Some("reCAPTCHA")
    } else if l.contains("h-captcha") || l.contains("hcaptcha.com") {
        Some("hCaptcha")
    } else if l.contains("solvemedia")
        || l.contains("circlecaptcha")
        || l.contains("cutcaptcha")
        || l.contains("friendlycaptcha")
        || l.contains("frc-captcha")
        || (l.contains("captcha") && (l.contains("<form") || l.contains("<input")))
    {
        Some("CAPTCHA")
    } else {
        None
    }
}

const CRYPTERS: &[&str] = &[
    "filecrypt.cc",
    "filecrypt.co",
    "keeplinks.org",
    "keeplinks.co",
    "linkcrypt.ws",
    "share-links.biz",
    "safelinking.net",
    "protect-links.net",
    "links-protect.com",
    "relink.to",
    "relink.us",
    "ncrypt.in",
    "lix.in",
    "linksafe.me",
    "linkbucks.com",
    "ouo.io",
    "ouo.press",
    "shrinkme.io",
    "shorte.st",
    "adf.ly",
    "linkvertise.com",
    "dl-protect.link",
    "dl-protect.info",
    "hide.cx",
    "mirrorcreator.com",
    "multiup.io",
    "multiup.org",
    "zero.sh",
    "linkshrink.net",
    "1fichier.com/dir",
];

pub fn is_crypter(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = u.host_str() else {
        return false;
    };
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    let path = u.path().to_ascii_lowercase();
    CRYPTERS.iter().any(|c| match c.split_once('/') {
        Some((h, p)) => host == h && path.starts_with(&format!("/{p}")),
        None => host == *c || host.ends_with(&format!(".{c}")),
    })
}

pub enum Opened {
    Links(Vec<String>),
    Captcha(&'static str),
    Nothing,
}

pub async fn open_crypter(client: &reqwest::Client, url: &str) -> anyhow::Result<Opened> {
    let mut resp = client.get(url).send().await?;
    let page = resp.url().clone();
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        body.extend_from_slice(&chunk);
        if body.len() > 2 * 1024 * 1024 {
            break;
        }
    }
    let html = String::from_utf8_lossy(&body);
    if let Some(kind) = captcha_in(&html) {
        return Ok(Opened::Captcha(kind));
    }
    let host = page
        .host_str()
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_string();
    let links: Vec<String> = links_in(&html.replace("&amp;", "&"))
        .into_iter()
        .filter(|l| {
            let Ok(u) = url::Url::parse(l) else {
                return l.starts_with("magnet:");
            };
            let h = u.host_str().unwrap_or_default().trim_start_matches("www.");
            !h.is_empty()
                && h != host
                && !h.ends_with(&format!(".{host}"))
                && !is_crypter(l)
                && !NOISE
                    .iter()
                    .any(|n| h == *n || h.ends_with(&format!(".{n}")))
                && !matches!(
                    crate::util::extension(u.path()).as_str(),
                    "js" | "css"
                        | "png"
                        | "jpg"
                        | "jpeg"
                        | "gif"
                        | "svg"
                        | "ico"
                        | "webp"
                        | "woff"
                        | "woff2"
                )
        })
        .collect();
    Ok(if links.is_empty() {
        Opened::Nothing
    } else {
        Opened::Links(links)
    })
}

const NOISE: &[&str] = &[
    "google.com",
    "googleapis.com",
    "gstatic.com",
    "googletagmanager.com",
    "google-analytics.com",
    "doubleclick.net",
    "facebook.com",
    "twitter.com",
    "x.com",
    "cloudflare.com",
    "cloudflareinsights.com",
    "jsdelivr.net",
    "cdnjs.cloudflare.com",
    "w3.org",
    "schema.org",
    "recaptcha.net",
    "hcaptcha.com",
    "jquery.com",
    "fontawesome.com",
];

pub fn batch_for(page: &str, links: Vec<String>) -> PageBatch {
    let host = url::Url::parse(page)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default();
    PageBatch {
        page_url: page.to_string(),
        page_title: Some(host),
        items: links
            .into_iter()
            .map(|url| BatchItem {
                kind: if url.starts_with("magnet:") {
                    "torrent".into()
                } else {
                    "auto".into()
                },
                media_type: "file".into(),
                referrer: Some(page.to_string()),
                url,
                ..Default::default()
            })
            .collect(),
        user_agent: None,
        cookies: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::BlockEncrypt;

    fn encrypt(plain: &str, key: [u8; 16]) -> String {
        let cipher = aes::Aes128::new(GenericArray::from_slice(&key));
        let mut data = plain.as_bytes().to_vec();
        while data.len() % 16 != 0 {
            data.push(0);
        }
        let mut prev = key;
        let mut out = Vec::new();
        for chunk in data.chunks(16) {
            let mut block = GenericArray::clone_from_slice(chunk);
            for (b, p) in block.iter_mut().zip(prev.iter()) {
                *b ^= p;
            }
            cipher.encrypt_block(&mut block);
            prev.copy_from_slice(&block);
            out.extend_from_slice(&block);
        }
        base64::engine::general_purpose::STANDARD.encode(out)
    }

    #[test]
    fn decrypts_click_n_load_v2_links() {
        let jk = "function f(){ return '31323334353637383930616263646566';}";
        let key = key_from_jk(jk).unwrap();
        assert_eq!(&key, b"1234567890abcdef");
        let crypted = encrypt(
            "https://host.example/a.part1.rar\r\nhttps://host.example/a.part2.rar",
            key,
        );
        let plain = decrypt(&crypted, key).unwrap();
        assert_eq!(
            links_in(&plain),
            vec![
                "https://host.example/a.part1.rar",
                "https://host.example/a.part2.rar"
            ]
        );
        assert!(key_from_jk("function f(){ return 5; }").is_none());
        assert!(decrypt("not base64!", key).is_none());
    }

    #[test]
    fn only_answers_requests_for_the_local_address() {
        let mut h = HeaderMap::new();
        assert!(!local_host(&h));
        h.insert(header::HOST, HeaderValue::from_static("127.0.0.1:9666"));
        assert!(local_host(&h));
        h.insert(header::HOST, HeaderValue::from_static("localhost:9666"));
        assert!(local_host(&h));
        h.insert(header::HOST, HeaderValue::from_static("evil.example:9666"));
        assert!(!local_host(&h));
    }

    #[test]
    fn finds_captchas_and_crypter_sites() {
        assert_eq!(
            captcha_in("<div class=\"g-recaptcha\" data-sitekey=x>"),
            Some("reCAPTCHA")
        );
        assert_eq!(
            captcha_in("<div class='cf-turnstile'></div>"),
            Some("Cloudflare Turnstile")
        );
        assert_eq!(
            captcha_in("<script src=https://hcaptcha.com/1/api.js>"),
            Some("hCaptcha")
        );
        assert_eq!(captcha_in("<p>no captcha talk here</p>"), None);
        assert!(is_crypter("https://www.filecrypt.cc/Container/ABC.html"));
        assert!(is_crypter("https://ouo.io/xyz"));
        assert!(is_crypter("https://1fichier.com/dir/abc"));
        assert!(!is_crypter("https://1fichier.com/?abc"));
        assert!(!is_crypter("https://example.com/file.zip"));
    }
}
