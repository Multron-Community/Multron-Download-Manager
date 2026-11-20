use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

use crate::settings::RssRule;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RssItem {
    pub id: String,
    pub title: String,
    pub download: String,
    pub page: Option<String>,
    pub published: Option<String>,
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seeders: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leechers: Option<u64>,
}

#[derive(Default)]
struct Draft {
    id: String,
    title: String,
    link: String,
    enclosure: String,
    magnet: String,
    published: String,
    size: Option<u64>,
    seeders: Option<u64>,
    leechers: Option<u64>,
    peers: Option<u64>,
}

pub fn human_size(v: &str) -> Option<u64> {
    let v = v.trim();
    let split = v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))?;
    let n: f64 = v[..split].replace(',', ".").trim().parse().ok()?;
    let unit = v[split..].trim().to_ascii_lowercase();
    let mul: f64 = match unit.as_str() {
        "b" | "bytes" => 1.0,
        "kb" => 1e3,
        "kib" => 1024.0,
        "mb" => 1e6,
        "mib" => 1048576.0,
        "gb" => 1e9,
        "gib" => 1073741824.0,
        "tb" => 1e12,
        "tib" => 1099511627776.0,
        _ => return None,
    };
    (n.is_finite() && n >= 0.0).then(|| (n * mul).round() as u64)
}

fn archive_torrent(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://archive.org/details/")
        .or_else(|| url.strip_prefix("http://archive.org/details/"))?;
    let id = rest.split(['/', '?', '#']).next()?;
    (!id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)))
    .then(|| format!("https://archive.org/download/{id}/{id}_archive.torrent"))
}

impl Draft {
    fn finish(mut self) -> Option<RssItem> {
        if self.magnet.is_empty() && self.enclosure.is_empty() {
            if let Some(t) = archive_torrent(&self.link) {
                self.enclosure = t;
            }
        }
        let is_torrent =
            |u: &str| u.starts_with("magnet:") || u.to_ascii_lowercase().contains(".torrent");
        let download = [&self.magnet, &self.enclosure, &self.link]
            .into_iter()
            .find(|u| !u.is_empty() && is_torrent(u))
            .or_else(|| {
                [&self.enclosure, &self.link]
                    .into_iter()
                    .find(|u| !u.is_empty())
            })?
            .clone();
        let page = (!self.link.is_empty() && self.link != download).then(|| self.link.clone());
        let id = if self.id.is_empty() {
            download.clone()
        } else {
            self.id
        };
        Some(RssItem {
            id,
            title: self.title.trim().to_string(),
            download,
            page,
            published: (!self.published.is_empty()).then_some(self.published),
            size: self.size,
            seeders: self.seeders,
            leechers: self.leechers.or_else(|| {
                self.peers
                    .map(|p| p.saturating_sub(self.seeders.unwrap_or(0)))
            }),
        })
    }
}

fn local(name: &[u8]) -> String {
    let n = String::from_utf8_lossy(name).to_string();
    n.rsplit(':').next().unwrap_or(&n).to_ascii_lowercase()
}

fn attr(e: &BytesStart, key: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        (local(a.key.as_ref()) == key).then(|| {
            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map(|v| v.into_owned())
                .unwrap_or_default()
        })
    })
}

pub fn parse(xml: &str) -> (Option<String>, Vec<RssItem>) {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut items = Vec::new();
    let mut feed_title: Option<String> = None;
    let mut draft: Option<Draft> = None;
    let mut path: Vec<String> = Vec::new();
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = local(e.name().as_ref());
                if name == "item" || name == "entry" {
                    draft = Some(Draft::default());
                }
                if let Some(d) = draft.as_mut() {
                    if name == "link" {
                        if let Some(href) = attr(&e, "href") {
                            let rel = attr(&e, "rel").unwrap_or_default();
                            if rel == "enclosure" {
                                d.enclosure = href;
                            } else if d.link.is_empty() {
                                d.link = href;
                            }
                        }
                    }
                }
                path.push(name);
                text.clear();
            }
            Ok(Event::Empty(e)) => {
                let name = local(e.name().as_ref());
                if let Some(d) = draft.as_mut() {
                    match name.as_str() {
                        "enclosure" => {
                            if let Some(url) = attr(&e, "url") {
                                d.enclosure = url;
                            }
                            if let Some(len) = attr(&e, "length").and_then(|l| l.parse().ok()) {
                                d.size = Some(len);
                            }
                        }
                        "link" => {
                            if let Some(href) = attr(&e, "href") {
                                if attr(&e, "rel").as_deref() == Some("enclosure") {
                                    d.enclosure = href;
                                } else if d.link.is_empty() {
                                    d.link = href;
                                }
                            }
                        }
                        "attr" => {
                            let key = attr(&e, "name").unwrap_or_default();
                            let value = attr(&e, "value").unwrap_or_default();
                            match key.as_str() {
                                "magneturl" => d.magnet = value,
                                "size" => d.size = value.parse().ok(),
                                "seeders" => d.seeders = value.parse().ok(),
                                "leechers" => d.leechers = value.parse().ok(),
                                "peers" => d.peers = value.parse().ok(),
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if let Ok(s) = t.decode() {
                    text.push_str(
                        &quick_xml::escape::unescape(&s)
                            .map(|c| c.into_owned())
                            .unwrap_or_else(|_| s.into_owned()),
                    );
                }
            }
            Ok(Event::CData(t)) => text.push_str(&String::from_utf8_lossy(&t)),
            Ok(Event::GeneralRef(r)) => {
                if let Ok(name) = r.decode() {
                    let resolved = quick_xml::escape::unescape(&format!("&{name};"))
                        .map(|c| c.into_owned())
                        .unwrap_or_default();
                    text.push_str(&resolved);
                }
            }
            Ok(Event::End(e)) => {
                let name = local(e.name().as_ref());
                let value = std::mem::take(&mut text).trim().to_string();
                path.pop();
                if name == "item" || name == "entry" {
                    if let Some(item) = draft.take().and_then(Draft::finish) {
                        items.push(item);
                    }
                    continue;
                }
                match draft.as_mut() {
                    Some(d) => match name.as_str() {
                        "title" => d.title = value,
                        "link" if !value.is_empty() && d.link.is_empty() => d.link = value,
                        "guid" | "id" => d.id = value,
                        "pubdate" | "published" | "updated" | "date" => d.published = value,
                        "magneturi" => d.magnet = value,
                        "infohash"
                            if d.magnet.is_empty()
                                && value.len() == 40
                                && value.chars().all(|c| c.is_ascii_hexdigit()) =>
                        {
                            d.magnet = format!("magnet:?xt=urn:btih:{}", value.to_ascii_lowercase())
                        }
                        "contentlength" | "size" => {
                            d.size = value.parse().ok().or_else(|| human_size(&value)).or(d.size)
                        }
                        "seeders" => d.seeders = value.parse().ok().or(d.seeders),
                        "leechers" => d.leechers = value.parse().ok().or(d.leechers),
                        _ => {}
                    },
                    None => {
                        if name == "title" && feed_title.is_none() && !value.is_empty() {
                            feed_title = Some(value);
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    (feed_title, items)
}

pub fn matches(rule: &RssRule, feed_url: &str, title: &str) -> bool {
    if !rule.enabled {
        return false;
    }
    if !rule.feeds.is_empty() && !rule.feeds.iter().any(|f| f == feed_url) {
        return false;
    }
    let title_lc = title.to_lowercase();
    let check = |pattern: &str, empty: bool| -> bool {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            return empty;
        }
        if rule.use_regex {
            regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .map(|r| r.is_match(title))
                .unwrap_or(false)
        } else {
            pattern.split('|').any(|alt| {
                let words: Vec<String> = alt.split_whitespace().map(str::to_lowercase).collect();
                !words.is_empty() && words.iter().all(|w| title_lc.contains(w))
            })
        }
    };
    check(&rule.must_contain, true) && !check(&rule.must_not_contain, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_org_items_download_their_torrent() {
        let xml = r#"<rss><channel><item><title>Some OS</title><link>https://archive.org/details/funbot-os-linux</link></item>
            <item><title>Film</title><link>https://archive.org/details/film1</link><enclosure url="https://archive.org/download/film1/format=MPEG4" type="video/h264"/></item></channel></rss>"#;
        let items = parse(xml).1;
        assert_eq!(
            items[0].download,
            "https://archive.org/download/funbot-os-linux/funbot-os-linux_archive.torrent"
        );
        assert_eq!(
            items[0].page.as_deref(),
            Some("https://archive.org/details/funbot-os-linux")
        );
        assert_eq!(
            items[1].download,
            "https://archive.org/download/film1/format=MPEG4"
        );
    }

    #[test]
    fn turns_an_info_hash_into_a_magnet() {
        let xml = r#"<rss><channel><item><title>Data set</title><infohash>9C11420AFBBB9000229D77EE0E1E6B128B94E20A</infohash><link>https://academictorrents.com/details/9c11</link></item></channel></rss>"#;
        let items = parse(xml).1;
        assert_eq!(
            items[0].download,
            "magnet:?xt=urn:btih:9c11420afbbb9000229d77ee0e1e6b128b94e20a"
        );
        assert_eq!(
            items[0].page.as_deref(),
            Some("https://academictorrents.com/details/9c11")
        );
    }

    #[test]
    fn reads_rss_with_magnets_and_enclosures() {
        let xml = r#"<?xml version="1.0"?>
<rss version="2.0" xmlns:torrent="http://xmlns.ezrss.it/0.1/">
  <channel>
    <title>Linux ISOs</title>
    <item>
      <title>Ubuntu 26.04 &amp; friends</title>
      <link>https://example.org/ubuntu</link>
      <guid>u-1</guid>
      <torrent:magnetURI><![CDATA[magnet:?xt=urn:btih:abc]]></torrent:magnetURI>
    </item>
    <item>
      <title>Fedora 44</title>
      <enclosure url="https://example.org/fedora.torrent" length="2048" type="application/x-bittorrent"/>
    </item>
  </channel>
</rss>"#;
        let (title, items) = parse(xml);
        assert_eq!(title.as_deref(), Some("Linux ISOs"));
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Ubuntu 26.04 & friends");
        assert_eq!(items[0].download, "magnet:?xt=urn:btih:abc");
        assert_eq!(items[0].page.as_deref(), Some("https://example.org/ubuntu"));
        assert_eq!(items[1].download, "https://example.org/fedora.torrent");
        assert_eq!(items[1].size, Some(2048));
    }

    #[test]
    fn reads_atom() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>A</title>
<entry><title>Debian 14</title><id>d-14</id><link rel="enclosure" href="https://x.org/debian.torrent"/><link href="https://x.org/debian"/></entry></feed>"#;
        let (_, items) = parse(xml);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].download, "https://x.org/debian.torrent");
        assert_eq!(items[0].id, "d-14");
    }

    #[test]
    fn rules() {
        let mut r = RssRule {
            enabled: true,
            must_contain: "ubuntu 1080p|fedora".into(),
            must_not_contain: "beta".into(),
            ..Default::default()
        };
        assert!(matches(&r, "f", "Ubuntu Desktop 1080p"));
        assert!(matches(&r, "f", "Fedora 44"));
        assert!(!matches(&r, "f", "Fedora 44 Beta"));
        assert!(!matches(&r, "f", "Ubuntu 720p"));
        r.use_regex = true;
        r.must_contain = r"^debian \d+$".into();
        r.must_not_contain = String::new();
        assert!(matches(&r, "f", "Debian 14"));
        r.feeds = vec!["other".into()];
        assert!(!matches(&r, "f", "Debian 14"));
    }
}
