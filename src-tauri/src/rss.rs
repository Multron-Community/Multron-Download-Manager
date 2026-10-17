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
}

impl Draft {
    fn finish(self) -> Option<RssItem> {
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
                            if key == "magneturl" {
                                d.magnet = value;
                            } else if key == "size" {
                                d.size = value.parse().ok();
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
                        "contentlength" | "size" => d.size = value.parse().ok().or(d.size),
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
