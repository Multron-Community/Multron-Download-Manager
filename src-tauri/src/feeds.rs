use std::sync::LazyLock;

use anyhow::{bail, Result};
use regex::Regex;
use serde::Serialize;
use url::Url;

use crate::rss::RssItem;

const YT_FEED: &str = "https://www.youtube.com/feeds/videos.xml";

static CHANNEL_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^UC[\w-]{22}$").unwrap());
static PAGE_CHANNEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:"externalId"|"channelId"|"browseId")\s*:\s*"(UC[\w-]{22})"|youtube\.com/channel/(UC[\w-]{22})"#)
        .unwrap()
});

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedFeed {
    pub url: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Podcast {
    pub name: String,
    pub author: String,
    pub feed_url: String,
    pub artwork: Option<String>,
    pub episodes: Option<u64>,
}

fn youtube_host(host: &str) -> bool {
    let h = host.trim_start_matches("www.").trim_start_matches("m.");
    h == "youtube.com" || h == "music.youtube.com" || h == "youtu.be"
}

pub fn spotify(url: &Url) -> bool {
    url.host_str()
        .is_some_and(|h| h == "spotify.com" || h.ends_with(".spotify.com") || h == "spotify.link")
}

pub enum YouTube {
    Feed(String),
    Lookup(String),
    Video,
}

pub fn youtube_kind(url: &Url) -> Option<YouTube> {
    if !youtube_host(url.host_str().unwrap_or_default()) {
        return None;
    }
    if let Some((_, list)) = url.query_pairs().find(|(k, _)| k == "list") {
        if !list.is_empty()
            && list
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Some(YouTube::Feed(format!("{YT_FEED}?playlist_id={list}")));
        }
    }
    let parts: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();
    if let Some(id) = parts.iter().find(|p| CHANNEL_ID.is_match(p)) {
        return Some(YouTube::Feed(format!("{YT_FEED}?channel_id={id}")));
    }
    match parts.first() {
        Some(p) if p.starts_with('@') => Some(YouTube::Lookup(url.to_string())),
        Some(&"c") | Some(&"user") if parts.len() > 1 => Some(YouTube::Lookup(url.to_string())),
        Some(&"feeds") => Some(YouTube::Feed(url.to_string())),
        _ => Some(YouTube::Video),
    }
}

pub fn channel_in_page(html: &str) -> Option<String> {
    let canonical = Regex::new(
        r#"<link rel="canonical" href="https://www\.youtube\.com/channel/(UC[\w-]{22})""#,
    )
    .ok()?;
    if let Some(c) = canonical.captures(html) {
        return Some(c[1].to_string());
    }
    PAGE_CHANNEL
        .captures(html)
        .and_then(|c| c.get(1).or(c.get(2)))
        .map(|m| m.as_str().to_string())
}

pub async fn resolve(client: &reqwest::Client, input: &str) -> Result<ResolvedFeed> {
    let input = input.trim();
    let Ok(url) = Url::parse(input) else {
        bail!("enter a full address that starts with https://");
    };
    if !matches!(url.scheme(), "http" | "https") {
        bail!("enter a full address that starts with https://");
    }
    if spotify(&url) {
        bail!("Spotify does not offer feeds, and its music is copy-protected and may not be downloaded. Podcasts that are also on Spotify can be found with Search podcasts");
    }
    match youtube_kind(&url) {
        None => Ok(ResolvedFeed {
            url: input.to_string(),
            name: String::new(),
        }),
        Some(YouTube::Feed(feed)) => Ok(ResolvedFeed {
            url: feed,
            name: String::new(),
        }),
        Some(YouTube::Video) => bail!("this is a single video. Paste the link of a channel, an artist or a playlist to follow its new videos"),
        Some(YouTube::Lookup(page)) => {
            let html = client
                .get(&page)
                .header(reqwest::header::COOKIE, "SOCS=CAI")
                .header(reqwest::header::ACCEPT_LANGUAGE, "en")
                .timeout(std::time::Duration::from_secs(20))
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            let Some(id) = channel_in_page(&html) else {
                bail!("could not find the channel on this page. Open the channel and copy its address again");
            };
            Ok(ResolvedFeed {
                url: format!("{YT_FEED}?channel_id={id}"),
                name: String::new(),
            })
        }
    }
}

pub async fn search_podcasts(client: &reqwest::Client, term: &str) -> Result<Vec<Podcast>> {
    let term = term.trim();
    if term.is_empty() {
        return Ok(Vec::new());
    }
    let mut url = Url::parse("https://itunes.apple.com/search")?;
    url.query_pairs_mut()
        .append_pair("media", "podcast")
        .append_pair("entity", "podcast")
        .append_pair("limit", "30")
        .append_pair("term", &term.chars().take(100).collect::<String>());
    let body: serde_json::Value = client
        .get(url)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(podcasts_from(&body))
}

pub fn podcasts_from(body: &serde_json::Value) -> Vec<Podcast> {
    let text = |v: &serde_json::Value, k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string()
    };
    body.get("results")
        .and_then(|r| r.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|r| {
                    let feed_url = text(r, "feedUrl");
                    if !(feed_url.starts_with("https://") || feed_url.starts_with("http://")) {
                        return None;
                    }
                    Some(Podcast {
                        name: text(r, "collectionName"),
                        author: text(r, "artistName"),
                        feed_url,
                        artwork: Some(text(r, "artworkUrl100"))
                            .filter(|a| a.starts_with("https://")),
                        episodes: r.get("trackCount").and_then(|n| n.as_u64()),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn title_filter(feed_url: &str) -> Option<String> {
    let fragment = Url::parse(feed_url).ok()?.fragment()?.to_string();
    let value = fragment.strip_prefix("only=")?;
    let decoded = percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .to_lowercase();
    (!decoded.trim().is_empty()).then_some(decoded)
}

pub fn only_matching(feed_url: &str, items: Vec<RssItem>) -> Vec<RssItem> {
    match title_filter(feed_url) {
        Some(prefix) => items
            .into_iter()
            .filter(|i| i.title.to_lowercase().starts_with(&prefix))
            .collect(),
        None => items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(input: &str) -> Option<String> {
        match youtube_kind(&Url::parse(input).unwrap()) {
            Some(YouTube::Feed(f)) => Some(f),
            _ => None,
        }
    }

    #[test]
    fn turns_youtube_links_into_feeds() {
        assert_eq!(
            feed("https://www.youtube.com/playlist?list=PLabc_-1").as_deref(),
            Some("https://www.youtube.com/feeds/videos.xml?playlist_id=PLabc_-1")
        );
        assert_eq!(
            feed("https://music.youtube.com/playlist?list=OLAK5uy_abc").as_deref(),
            Some("https://www.youtube.com/feeds/videos.xml?playlist_id=OLAK5uy_abc")
        );
        assert_eq!(
            feed("https://www.youtube.com/channel/UCabcdefghijklmnopqrstuv/videos").as_deref(),
            Some("https://www.youtube.com/feeds/videos.xml?channel_id=UCabcdefghijklmnopqrstuv")
        );
        assert_eq!(
            feed("https://music.youtube.com/channel/UCabcdefghijklmnopqrstuv").as_deref(),
            Some("https://www.youtube.com/feeds/videos.xml?channel_id=UCabcdefghijklmnopqrstuv")
        );
        assert!(matches!(
            youtube_kind(&Url::parse("https://www.youtube.com/@someone").unwrap()),
            Some(YouTube::Lookup(_))
        ));
        assert!(matches!(
            youtube_kind(&Url::parse("https://www.youtube.com/watch?v=abc").unwrap()),
            Some(YouTube::Video)
        ));
        assert!(youtube_kind(&Url::parse("https://example.com/rss").unwrap()).is_none());
        assert!(spotify(
            &Url::parse("https://open.spotify.com/show/abc").unwrap()
        ));
    }

    #[test]
    fn finds_the_channel_in_a_page() {
        let html = r#"<html><link rel="canonical" href="https://www.youtube.com/channel/UCabcdefghijklmnopqrstuv"></html>"#;
        assert_eq!(
            channel_in_page(html).as_deref(),
            Some("UCabcdefghijklmnopqrstuv")
        );
        let json = r#"{"metadata":{"externalId":"UCzzzzzzzzzzzzzzzzzzzzzz"}}"#;
        assert_eq!(
            channel_in_page(json).as_deref(),
            Some("UCzzzzzzzzzzzzzzzzzzzzzz")
        );
        assert!(channel_in_page("<html></html>").is_none());
    }

    #[test]
    fn reads_podcast_search_results() {
        let body = serde_json::json!({"results": [
            {"collectionName": "Show", "artistName": "Host", "feedUrl": "https://example.com/feed.xml", "artworkUrl100": "https://example.com/a.jpg", "trackCount": 12},
            {"collectionName": "No feed"}
        ]});
        let found = podcasts_from(&body);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].feed_url, "https://example.com/feed.xml");
        assert_eq!(found[0].episodes, Some(12));
    }

    #[test]
    fn filters_feed_items_by_title() {
        let item = |t: &str| RssItem {
            title: t.into(),
            ..Default::default()
        };
        let items = vec![
            item("Ubuntu 26.04 - Desktop"),
            item("Kubuntu 26.04"),
            item("Linux Mint 22.3"),
        ];
        let url = "https://fosstorrents.com/feed/torrents.xml#only=Ubuntu%20";
        let kept: Vec<String> = only_matching(url, items.clone())
            .into_iter()
            .map(|i| i.title)
            .collect();
        assert_eq!(kept, vec!["Ubuntu 26.04 - Desktop"]);
        assert_eq!(only_matching("https://x.org/feed.xml", items).len(), 3);
    }
}

static FEED_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<link\b[^>]*>").unwrap());
static ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?is)([a-z-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap());

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FoundFeed {
    pub url: String,
    pub title: String,
}

pub fn feeds_in_page(page: &Url, html: &str) -> Vec<FoundFeed> {
    let mut out: Vec<FoundFeed> = Vec::new();
    for tag in FEED_LINK.find_iter(html) {
        let mut rel = String::new();
        let mut kind = String::new();
        let mut href = String::new();
        let mut title = String::new();
        for a in ATTR.captures_iter(tag.as_str()) {
            let v = a
                .get(2)
                .or(a.get(3))
                .map(|m| m.as_str())
                .unwrap_or_default();
            match a[1].to_ascii_lowercase().as_str() {
                "rel" => rel = v.to_ascii_lowercase(),
                "type" => kind = v.to_ascii_lowercase(),
                "href" => href = crate::crawl::decode_entities(v),
                "title" => title = crate::crawl::decode_entities(v),
                _ => {}
            }
        }
        if !rel.split_whitespace().any(|r| r == "alternate")
            || !(kind.contains("rss") || kind.contains("atom"))
        {
            continue;
        }
        if let Ok(u) = page.join(href.trim()) {
            if matches!(u.scheme(), "http" | "https") && !out.iter().any(|f| f.url == u.as_str()) {
                out.push(FoundFeed {
                    url: u.to_string(),
                    title: title.trim().to_string(),
                });
            }
        }
    }
    out
}

fn looks_like_feed(body: &str) -> bool {
    let head: String = body
        .chars()
        .take(2000)
        .collect::<String>()
        .to_ascii_lowercase();
    head.contains("<rss") || head.contains("<feed") || head.contains("<rdf:rdf")
}

pub async fn discover(client: &reqwest::Client, input: &str) -> Result<Vec<FoundFeed>> {
    let input = input.trim();
    let with_scheme = if input.contains("://") {
        input.to_string()
    } else {
        format!("https://{input}")
    };
    let Ok(page) = Url::parse(&with_scheme) else {
        bail!("enter a website address, for example example.com");
    };
    if !matches!(page.scheme(), "http" | "https") || page.host_str().is_none() {
        bail!("enter a website address, for example example.com");
    }
    if spotify(&page) {
        bail!("Spotify does not offer feeds, and its music is copy-protected and may not be downloaded. Podcasts that are also on Spotify can be found with Search podcasts");
    }
    if let Some(YouTube::Feed(_) | YouTube::Lookup(_)) = youtube_kind(&page) {
        let r = resolve(client, page.as_str()).await?;
        return Ok(vec![FoundFeed {
            url: r.url,
            title: "YouTube".into(),
        }]);
    }
    let get = |u: Url| async move {
        let resp = client
            .get(u.clone())
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await?
            .error_for_status()?;
        let final_url = resp.url().clone();
        let body: String = resp.text().await?.chars().take(4 * 1024 * 1024).collect();
        Ok::<_, anyhow::Error>((final_url, body))
    };
    let (final_url, body) = get(page.clone()).await?;
    if looks_like_feed(&body) {
        return Ok(vec![FoundFeed {
            url: final_url.to_string(),
            title: String::new(),
        }]);
    }
    let mut found = feeds_in_page(&final_url, &body);
    if found.is_empty() {
        for path in [
            "/feed",
            "/rss",
            "/rss.xml",
            "/feed.xml",
            "/atom.xml",
            "/index.xml",
        ] {
            let Ok(u) = final_url.join(path) else {
                continue;
            };
            if let Ok((at, text)) = get(u).await {
                if looks_like_feed(&text) {
                    found.push(FoundFeed {
                        url: at.to_string(),
                        title: String::new(),
                    });
                    break;
                }
            }
        }
    }
    found.truncate(30);
    Ok(found)
}

#[cfg(test)]
mod discover_tests {
    use super::*;

    #[test]
    fn finds_feed_links_in_a_page() {
        let page = Url::parse("https://blog.example.com/posts/").unwrap();
        let html = r#"<head><link rel="alternate" type="application/rss+xml" title="Posts &amp; news" href="/feed.xml">
            <link rel="alternate" type="application/atom+xml" href="https://blog.example.com/atom">
            <link rel="stylesheet" href="/s.css"><link rel="alternate" hreflang="de" href="/de/"></head>"#;
        let found = feeds_in_page(&page, html);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].url, "https://blog.example.com/feed.xml");
        assert_eq!(found[0].title, "Posts & news");
        assert_eq!(found[1].url, "https://blog.example.com/atom");
        assert!(looks_like_feed(
            "<?xml version=\"1.0\"?><rss version=\"2.0\">"
        ));
        assert!(!looks_like_feed("<html><body>"));
    }
}
