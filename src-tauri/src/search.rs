use std::time::Duration;

use anyhow::{bail, Result};
use futures_util::future::join_all;
use serde::Serialize;
use serde_json::Value;

use crate::settings::Settings;

pub const TORRENTS_CSV: &str = "torrents-csv";
pub const ARCHIVE: &str = "archive";
pub const BUILTIN: [(&str, &str, &str); 2] = [
    (
        TORRENTS_CSV,
        "Torrents-CSV",
        "https://torrents-csv.com/service/search?q={query}&size=100",
    ),
    (
        ARCHIVE,
        "Internet Archive",
        "https://archive.org/advancedsearch.php?q=title%3A({query})+AND+btih%3A*&fl%5B%5D=identifier&fl%5B%5D=title&fl%5B%5D=item_size&fl%5B%5D=publicdate&rows=100&output=json",
    ),
];
const TRACKERS: [&str; 4] = [
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.demonii.com:1337/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://exodus.desync.com:6969/announce",
];
const MAX_RESULTS: usize = 100;
const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub site: String,
    pub title: String,
    pub download: String,
    pub page: Option<String>,
    pub size: Option<u64>,
    pub seeders: Option<u64>,
    pub leechers: Option<u64>,
    pub published: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct SiteError {
    pub site: String,
    pub error: String,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct SearchOutcome {
    pub results: Vec<SearchResult>,
    pub errors: Vec<SiteError>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SiteView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub builtin: bool,
    pub enabled: bool,
}

pub fn sites(s: &Settings) -> Vec<SiteView> {
    let mut out: Vec<SiteView> = BUILTIN
        .iter()
        .map(|(id, name, url)| SiteView {
            id: id.to_string(),
            name: name.to_string(),
            url: url.to_string(),
            builtin: true,
            enabled: !s.search_disabled.iter().any(|d| d == id),
        })
        .collect();
    out.extend(s.search_sites.iter().map(|c| SiteView {
        id: c.url.clone(),
        name: c.name.clone(),
        url: c.url.clone(),
        builtin: false,
        enabled: c.enabled,
    }));
    out
}

pub fn valid_template(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && url.contains("{query}")
        && url::Url::parse(&url.replace("{query}", "x")).is_ok()
}

pub fn fill(template: &str, query: &str) -> String {
    let encoded: String = url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
    template.replace("{query}", &encoded)
}

fn magnet(hash: &str, name: &str) -> String {
    let mut m = format!("magnet:?xt=urn:btih:{hash}");
    let dn: String = url::form_urlencoded::byte_serialize(name.as_bytes()).collect();
    m.push_str("&dn=");
    m.push_str(&dn);
    for t in TRACKERS {
        let tr: String = url::form_urlencoded::byte_serialize(t.as_bytes()).collect();
        m.push_str("&tr=");
        m.push_str(&tr);
    }
    m
}

fn num(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

fn text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Value::Array(a) => a.first().and_then(text),
        _ => None,
    }
}

pub fn parse_torrents_csv(v: &Value) -> Vec<SearchResult> {
    let list = v
        .get("torrents")
        .and_then(Value::as_array)
        .or_else(|| v.as_array());
    let Some(list) = list else { return Vec::new() };
    list.iter()
        .filter_map(|t| {
            let hash = t.get("infohash").and_then(Value::as_str)?.trim();
            if hash.len() != 40 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let title = t.get("name").and_then(text)?;
            Some(SearchResult {
                site: TORRENTS_CSV.into(),
                download: magnet(hash, &title),
                title,
                page: None,
                size: t.get("size_bytes").and_then(num),
                seeders: t.get("seeders").and_then(num),
                leechers: t.get("leechers").and_then(num),
                published: t
                    .get("created_unix")
                    .and_then(Value::as_i64)
                    .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
                    .map(|d| d.format("%Y-%m-%d").to_string()),
            })
        })
        .collect()
}

pub fn parse_archive(v: &Value) -> Vec<SearchResult> {
    let Some(docs) = v.pointer("/response/docs").and_then(Value::as_array) else {
        return Vec::new();
    };
    docs.iter()
        .filter_map(|d| {
            let id = d.get("identifier").and_then(Value::as_str)?.trim();
            if id.is_empty()
                || !id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            {
                return None;
            }
            Some(SearchResult {
                site: ARCHIVE.into(),
                title: d
                    .get("title")
                    .and_then(text)
                    .unwrap_or_else(|| id.to_string()),
                download: format!("https://archive.org/download/{id}/{id}_archive.torrent"),
                page: Some(format!("https://archive.org/details/{id}")),
                size: d.get("item_size").and_then(num),
                seeders: None,
                leechers: None,
                published: d
                    .get("publicdate")
                    .and_then(text)
                    .map(|p| p.chars().take(10).collect()),
            })
        })
        .collect()
}

pub fn parse_feed(site: &str, body: &str) -> Vec<SearchResult> {
    crate::rss::parse(body)
        .1
        .into_iter()
        .map(|i| SearchResult {
            site: site.to_string(),
            title: i.title,
            download: i.download,
            page: i.page,
            size: i.size,
            seeders: i.seeders,
            leechers: i.leechers,
            published: i.published,
        })
        .collect()
}

async fn query_site(
    client: &reqwest::Client,
    site: &SiteView,
    query: &str,
) -> Result<Vec<SearchResult>> {
    let resp = client
        .get(fill(&site.url, query))
        .timeout(TIMEOUT)
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        bail!("the site answered HTTP {status}");
    }
    let body = resp.text().await?;
    let mut found = match site.id.as_str() {
        TORRENTS_CSV => parse_torrents_csv(&serde_json::from_str(&body)?),
        ARCHIVE => parse_archive(&serde_json::from_str(&body)?),
        _ => {
            let trimmed = body.trim_start();
            if !trimmed.starts_with('<') {
                bail!("the site did not answer with an RSS or Torznab feed");
            }
            parse_feed(&site.id, &body)
        }
    };
    found.truncate(MAX_RESULTS);
    Ok(found)
}

pub async fn search(
    client: &reqwest::Client,
    s: &Settings,
    query: &str,
    only: &[String],
) -> SearchOutcome {
    let query = query.trim();
    let mut out = SearchOutcome::default();
    if query.is_empty() {
        return out;
    }
    let chosen: Vec<SiteView> = sites(s)
        .into_iter()
        .filter(|v| v.enabled && (only.is_empty() || only.contains(&v.id)))
        .collect();
    let answers = join_all(chosen.iter().map(|site| query_site(client, site, query))).await;
    for (site, answer) in chosen.iter().zip(answers) {
        match answer {
            Ok(found) => out.results.extend(found),
            Err(e) => out.errors.push(SiteError {
                site: site.id.clone(),
                error: format!("{e:#}"),
            }),
        }
    }
    out.results
        .sort_by_key(|r| std::cmp::Reverse(r.seeders.unwrap_or(0)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_torrents_csv_answers() {
        let v: Value = serde_json::from_str(r#"{"torrents":[{"infohash":"029830a40e0fd9eca4cabb8f8c0e24a9e8c5a03f","name":"Debian 13 DVD","size_bytes":4392928,"created_unix":1746104157,"seeders":17,"leechers":1},{"infohash":"bad","name":"x"}]}"#).unwrap();
        let r = parse_torrents_csv(&v);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title, "Debian 13 DVD");
        assert_eq!(r[0].seeders, Some(17));
        assert_eq!(r[0].published.as_deref(), Some("2025-05-01"));
        assert!(r[0].download.starts_with(
            "magnet:?xt=urn:btih:029830a40e0fd9eca4cabb8f8c0e24a9e8c5a03f&dn=Debian+13+DVD&tr="
        ));
        let old: Value = serde_json::from_str(
            r#"[{"infohash":"029830a40e0fd9eca4cabb8f8c0e24a9e8c5a03f","name":"Old"}]"#,
        )
        .unwrap();
        assert_eq!(parse_torrents_csv(&old).len(), 1);
    }

    #[test]
    fn reads_internet_archive_answers() {
        let v: Value = serde_json::from_str(r#"{"response":{"docs":[{"identifier":"linux-debianmanager","title":"Debian manager","item_size":2347201367,"publicdate":"2023-10-18T00:13:52Z"},{"identifier":"../evil"}]}}"#).unwrap();
        let r = parse_archive(&v);
        assert_eq!(r.len(), 1);
        assert_eq!(
            r[0].download,
            "https://archive.org/download/linux-debianmanager/linux-debianmanager_archive.torrent"
        );
        assert_eq!(r[0].published.as_deref(), Some("2023-10-18"));
        assert_eq!(r[0].size, Some(2347201367));
    }

    #[test]
    fn reads_torznab_and_nyaa_style_feeds() {
        let xml = r#"<rss xmlns:torznab="http://torznab.com/schemas/2015/feed" xmlns:nyaa="https://nyaa.si/xmlns/nyaa"><channel><title>x</title>
<item><title>Fedora 43</title><link>https://idx.example/f.torrent</link><torznab:attr name="seeders" value="40"/><torznab:attr name="peers" value="55"/><torznab:attr name="size" value="2048"/></item>
<item><title>Arch</title><link>https://idx.example/a.torrent</link><nyaa:seeders>12</nyaa:seeders><nyaa:leechers>3</nyaa:leechers><nyaa:size>1.5 GiB</nyaa:size></item>
</channel></rss>"#;
        let r = parse_feed("custom", xml);
        assert_eq!(r.len(), 2);
        assert_eq!(
            (r[0].seeders, r[0].leechers, r[0].size),
            (Some(40), Some(15), Some(2048))
        );
        assert_eq!((r[1].seeders, r[1].leechers), (Some(12), Some(3)));
        assert_eq!(r[1].size, Some(1610612736));
    }

    #[tokio::test]
    #[ignore]
    async fn live_search_on_built_in_sites() {
        let client = reqwest::Client::builder()
            .user_agent("MultronDM")
            .build()
            .unwrap();
        let out = search(&client, &Settings::default(), "debian", &[]).await;
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        for site in [TORRENTS_CSV, ARCHIVE] {
            let n = out.results.iter().filter(|r| r.site == site).count();
            println!("{site}: {n}");
            assert!(n > 0);
        }
        println!(
            "{:?}",
            out.results
                .iter()
                .take(3)
                .map(|r| (&r.title, r.seeders))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn fills_search_templates() {
        assert!(valid_template("https://idx.example/api?t=search&q={query}"));
        assert!(!valid_template("https://idx.example/api?q="));
        assert!(!valid_template("file:///c/{query}"));
        assert_eq!(
            fill("https://idx.example/s?q={query}", " debian 13 & more "),
            "https://idx.example/s?q=debian+13+%26+more"
        );
    }
}
