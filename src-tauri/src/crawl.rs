use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use url::Url;

const MAX_PAGE: usize = 8 * 1024 * 1024;
pub const MAX_FOUND: usize = 5000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FoundLink {
    pub url: String,
    pub tag: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PageScan {
    pub url: String,
    pub title: Option<String>,
    pub is_file: bool,
    pub content_type: Option<String>,
    pub links: Vec<FoundLink>,
}

static TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<([a-z][a-z0-9-]*)\b([^>]*)>").unwrap());
static ATTR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)([a-z][a-z0-9_:-]*)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+))"#).unwrap()
});
static PLAIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:https?:\\?/\\?/|magnet:\?)[^\s"'<>`\\()\[\]{}]+"#).unwrap()
});
static TITLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<title[^>]*>(.*?)</title>").unwrap());
static ANCHOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<a\b([^>]*)>(.*?)</a>").unwrap());
static STRIP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").unwrap());

const LINK_ATTRS: &[&str] = &[
    "href",
    "src",
    "data-src",
    "data-href",
    "data-url",
    "data-file",
    "data-download",
    "data-original",
    "data-lazy-src",
    "poster",
    "action",
];

pub fn is_html(content_type: Option<&str>) -> bool {
    content_type
        .map(|c| {
            let c = c.to_ascii_lowercase();
            c.starts_with("text/html") || c.contains("xhtml")
        })
        .unwrap_or(false)
}

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ if name.starts_with("#x") || name.starts_with("#X") => {
                u32::from_str_radix(&name[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if name.starts_with('#') => name[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn clean_text(s: &str) -> Option<String> {
    let text = decode_entities(&STRIP.replace_all(s, " "));
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    Some(text.chars().take(140).collect())
}

fn resolve(base: &Url, raw: &str) -> Option<String> {
    let raw = decode_entities(raw.trim()).replace("\\/", "/");
    if raw.is_empty() || raw.starts_with('#') {
        return None;
    }
    let lower = raw.to_ascii_lowercase();
    if lower.starts_with("magnet:?") {
        return Some(raw);
    }
    if [
        "javascript:",
        "mailto:",
        "tel:",
        "data:",
        "blob:",
        "about:",
        "sms:",
    ]
    .iter()
    .any(|p| lower.starts_with(p))
    {
        return None;
    }
    let mut url = base.join(&raw).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    url.set_fragment(None);
    Some(url.to_string())
}

fn srcset_urls(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(',')
        .filter_map(|part| part.split_whitespace().next())
}

pub fn extract(page: &Url, html: &str) -> (Option<String>, Vec<FoundLink>) {
    let title = TITLE.captures(html).and_then(|c| clean_text(&c[1]));
    let mut base = page.clone();
    let mut found: Vec<FoundLink> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut push =
        |found: &mut Vec<FoundLink>, url: Option<String>, tag: &str, text: Option<String>| {
            let Some(url) = url else {
                return;
            };
            if found.len() >= MAX_FOUND || url == page.as_str() || !seen.insert(url.clone()) {
                return;
            }
            found.push(FoundLink {
                url,
                tag: tag.to_string(),
                text,
            });
        };

    let mut texts: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for cap in TAG.captures_iter(html) {
        let tag = cap[1].to_ascii_lowercase();
        if tag == "base" {
            for a in ATTR.captures_iter(&cap[2]) {
                if a[1].eq_ignore_ascii_case("href") {
                    let v = a
                        .get(2)
                        .or(a.get(3))
                        .or(a.get(4))
                        .map(|m| m.as_str())
                        .unwrap_or("");
                    if let Ok(b) = page.join(&decode_entities(v.trim())) {
                        base = b;
                    }
                }
            }
        }
    }
    for cap in ANCHOR.captures_iter(html) {
        let Some(text) = clean_text(&cap[2]) else {
            continue;
        };
        for a in ATTR.captures_iter(&cap[1]) {
            if a[1].eq_ignore_ascii_case("href") {
                let v = a
                    .get(2)
                    .or(a.get(3))
                    .or(a.get(4))
                    .map(|m| m.as_str())
                    .unwrap_or("");
                if let Some(u) = resolve(&base, v) {
                    texts.entry(u).or_insert(text.clone());
                }
            }
        }
    }

    for cap in TAG.captures_iter(html) {
        let tag = cap[1].to_ascii_lowercase();
        if tag == "base" {
            continue;
        }
        let mut meta_prop = String::new();
        let mut meta_content = None;
        for a in ATTR.captures_iter(&cap[2]) {
            let name = a[1].to_ascii_lowercase();
            let value = a
                .get(2)
                .or(a.get(3))
                .or(a.get(4))
                .map(|m| m.as_str())
                .unwrap_or("");
            if name == "srcset" || name == "data-srcset" {
                for u in srcset_urls(value) {
                    push(&mut found, resolve(&base, u), &tag, None);
                }
            } else if tag == "meta" && (name == "property" || name == "name" || name == "itemprop")
            {
                meta_prop = value.to_ascii_lowercase();
            } else if tag == "meta" && name == "content" {
                meta_content = Some(value.to_string());
            } else if LINK_ATTRS.contains(&name.as_str()) {
                let url = resolve(&base, value);
                let text = url.as_ref().and_then(|u| texts.get(u).cloned());
                push(&mut found, url, &tag, text);
            }
        }
        if let Some(content) = meta_content {
            let wanted = [
                "og:video",
                "og:audio",
                "og:image",
                "twitter:image",
                "twitter:player",
                "contenturl",
                "embedurl",
            ];
            if wanted.iter().any(|w| meta_prop.starts_with(w)) {
                push(&mut found, resolve(&base, &content), "meta", None);
            }
        }
    }

    let unescaped = html.replace("\\/", "/");
    for m in PLAIN.find_iter(&unescaped) {
        let raw = m.as_str().trim_end_matches(['.', ',', ';', ':', '!', '?']);
        push(&mut found, resolve(&base, raw), "text", None);
    }
    (title, found)
}

pub fn rule_links(page: &Url, html: &str, pattern: &str) -> anyhow::Result<Vec<FoundLink>> {
    let re = regex::RegexBuilder::new(pattern)
        .size_limit(2 << 20)
        .build()
        .map_err(|e| anyhow::anyhow!("the link pattern is not valid: {e}"))?;
    let html = html.replace("\\/", "/");
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for cap in re.captures_iter(&html) {
        let raw = cap
            .get(1)
            .or(cap.get(0))
            .map(|m| m.as_str())
            .unwrap_or_default();
        let Some(url) = resolve(page, raw) else {
            continue;
        };
        if seen.insert(url.clone()) {
            out.push(FoundLink {
                url,
                tag: "rule".into(),
                text: None,
            });
        }
        if out.len() >= MAX_FOUND {
            break;
        }
    }
    Ok(out)
}

pub async fn fetch_html(
    client: &reqwest::Client,
    url: &str,
    headers: reqwest::header::HeaderMap,
    secs: u64,
) -> anyhow::Result<(Url, Option<String>, String)> {
    let mut resp = client
        .get(url)
        .headers(headers)
        .timeout(std::time::Duration::from_secs(secs.clamp(3, 300)))
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("the page answered {status}");
    }
    let page = resp.url().clone();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let mut body: Vec<u8> = Vec::new();
    if is_html(content_type.as_deref()) {
        while let Some(chunk) = resp.chunk().await? {
            body.extend_from_slice(&chunk);
            if body.len() > MAX_PAGE {
                break;
            }
        }
    }
    Ok((
        page,
        content_type,
        String::from_utf8_lossy(&body).into_owned(),
    ))
}

pub async fn scan_with_rule(
    client: &reqwest::Client,
    url: &str,
    headers: reqwest::header::HeaderMap,
    rule: Option<&crate::settings::SiteRule>,
    secs: u64,
) -> anyhow::Result<PageScan> {
    let Some(rule) = rule else {
        return scan(client, url, headers, secs).await;
    };
    let (page, content_type, html) = fetch_html(client, url, headers.clone(), secs).await?;
    if !is_html(content_type.as_deref()) {
        return Ok(PageScan {
            url: page.to_string(),
            is_file: true,
            content_type,
            ..Default::default()
        });
    }
    let (title, generic) = extract(&page, &html);
    let mut found = rule_links(&page, &html, &rule.pattern)?;
    if rule.follow {
        let mut next = Vec::new();
        for link in found.iter().take(20) {
            if let Ok((p, ct, h)) = fetch_html(client, &link.url, headers.clone(), secs).await {
                if is_html(ct.as_deref()) {
                    next.extend(rule_links(&p, &h, &rule.pattern).unwrap_or_default());
                } else {
                    next.push(link.clone());
                }
            }
        }
        let mut seen = HashSet::new();
        next.retain(|l| seen.insert(l.url.clone()));
        found = next;
    }
    if !rule.only_rule {
        let known: HashSet<String> = found.iter().map(|l| l.url.clone()).collect();
        found.extend(generic.into_iter().filter(|l| !known.contains(&l.url)));
    }
    found.truncate(MAX_FOUND);
    Ok(PageScan {
        url: page.to_string(),
        title,
        is_file: false,
        content_type,
        links: found,
    })
}

pub async fn scan(
    client: &reqwest::Client,
    url: &str,
    headers: reqwest::header::HeaderMap,
    secs: u64,
) -> anyhow::Result<PageScan> {
    let mut resp = client
        .get(url)
        .headers(headers)
        .timeout(std::time::Duration::from_secs(secs.clamp(3, 300)))
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("the page answered {status}");
    }
    let page = resp.url().clone();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    if !is_html(content_type.as_deref()) {
        return Ok(PageScan {
            url: page.to_string(),
            is_file: true,
            content_type,
            ..Default::default()
        });
    }
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_PAGE {
            break;
        }
    }
    let html = String::from_utf8_lossy(&body);
    let (title, links) = extract(&page, &html);
    Ok(PageScan {
        url: page.to_string(),
        title,
        is_file: false,
        content_type,
        links,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(html: &str) -> Vec<String> {
        let page = Url::parse("https://example.com/dir/page.html").unwrap();
        extract(&page, html).1.into_iter().map(|l| l.url).collect()
    }

    #[test]
    fn finds_links_in_attributes_and_text() {
        let html = r##"<html><head><title>My &amp; Page</title>
            <link rel="stylesheet" href="/style.css"></head><body>
            <a href="files/a.zip">Archive A</a>
            <a href='https://cdn.example.org/b.mp4#t=3'>B</a>
            <img src="img/c.png" srcset="img/c-2x.png 2x, img/c-3x.png 3x">
            <video poster="p.jpg"><source src="v.webm"></video>
            <a href="javascript:void(0)">x</a><a href="mailto:a@b.c">m</a><a href="#top">t</a>
            <script>var u = "https:\/\/files.example.net\/d.pdf";</script>
            Mirror: https://mirror.example.com/e.rar.
            <a href="magnet:?xt=urn:btih:abc&amp;dn=f">magnet</a>
            </body></html>"##;
        let page = Url::parse("https://example.com/dir/page.html").unwrap();
        let (title, links) = extract(&page, html);
        assert_eq!(title.as_deref(), Some("My & Page"));
        let got: Vec<&str> = links.iter().map(|l| l.url.as_str()).collect();
        for want in [
            "https://example.com/style.css",
            "https://example.com/dir/files/a.zip",
            "https://cdn.example.org/b.mp4",
            "https://example.com/dir/img/c.png",
            "https://example.com/dir/img/c-2x.png",
            "https://example.com/dir/img/c-3x.png",
            "https://example.com/dir/p.jpg",
            "https://example.com/dir/v.webm",
            "https://files.example.net/d.pdf",
            "https://mirror.example.com/e.rar",
            "magnet:?xt=urn:btih:abc&dn=f",
        ] {
            assert!(got.contains(&want), "missing {want} in {got:?}");
        }
        assert!(!got
            .iter()
            .any(|u| u.starts_with("javascript") || u.starts_with("mailto")));
        assert_eq!(got.len(), got.iter().collect::<HashSet<_>>().len());
        let a = links.iter().find(|l| l.url.ends_with("a.zip")).unwrap();
        assert_eq!(a.text.as_deref(), Some("Archive A"));
        assert_eq!(a.tag, "a");
    }

    #[test]
    fn site_rules_pick_links_with_a_pattern() {
        let page = Url::parse("https://files.example.com/folder/1").unwrap();
        let html = r#"<a href="/get?id=7">x</a> <script>dl("https:\/\/cdn.example.com\/a.zip")</script> <a href="/get?id=7">again</a>"#;
        let found = rule_links(&page, html, r#"href="(/get\?id=\d+)""#).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].url, "https://files.example.com/get?id=7");
        assert_eq!(found[0].tag, "rule");
        let whole = rule_links(&page, html, r"https://cdn\.example\.com/[\w./]+").unwrap();
        assert_eq!(whole[0].url, "https://cdn.example.com/a.zip");
        assert!(rule_links(&page, html, "(").is_err());
    }

    #[test]
    fn respects_base_href() {
        let got = urls(r#"<base href="https://other.example.com/x/"><a href="y.zip">y</a>"#);
        assert!(got.contains(&"https://other.example.com/x/y.zip".to_string()));
    }

    #[test]
    fn decodes_entities() {
        assert_eq!(
            decode_entities("a&amp;b&#47;c&#x41;&unknown;"),
            "a&b/cA&unknown;"
        );
    }

    #[test]
    fn detects_html() {
        assert!(is_html(Some("text/html; charset=utf-8")));
        assert!(is_html(Some("application/xhtml+xml")));
        assert!(!is_html(Some("application/zip")));
        assert!(!is_html(None));
    }
}
