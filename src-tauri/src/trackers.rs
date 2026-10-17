use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use percent_encoding::{percent_encode, NON_ALPHANUMERIC};
use serde::Serialize;
use tokio::net::UdpSocket;

#[derive(Debug, Clone, PartialEq)]
pub enum Bencode<'a> {
    Int(i64),
    Bytes(&'a [u8]),
    List(Vec<Bencode<'a>>),
    Dict(Vec<(&'a [u8], Bencode<'a>)>),
}

impl<'a> Bencode<'a> {
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        let mut i = 0;
        parse_at(data, &mut i, 0)
    }

    pub fn get(&self, key: &str) -> Option<&Bencode<'a>> {
        match self {
            Bencode::Dict(items) => items
                .iter()
                .find(|(k, _)| *k == key.as_bytes())
                .map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Bencode::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&'a [u8]> {
        match self {
            Bencode::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<String> {
        self.as_bytes()
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }

    pub fn as_list(&self) -> Option<&[Bencode<'a>]> {
        match self {
            Bencode::List(v) => Some(v),
            _ => None,
        }
    }
}

fn parse_at<'a>(d: &'a [u8], i: &mut usize, depth: usize) -> Option<Bencode<'a>> {
    if depth > 64 {
        return None;
    }
    match *d.get(*i)? {
        b'i' => {
            let end = d[*i..].iter().position(|&c| c == b'e')? + *i;
            let v = std::str::from_utf8(&d[*i + 1..end]).ok()?.parse().ok()?;
            *i = end + 1;
            Some(Bencode::Int(v))
        }
        b'l' => {
            *i += 1;
            let mut items = Vec::new();
            while *d.get(*i)? != b'e' {
                items.push(parse_at(d, i, depth + 1)?);
            }
            *i += 1;
            Some(Bencode::List(items))
        }
        b'd' => {
            *i += 1;
            let mut items = Vec::new();
            while *d.get(*i)? != b'e' {
                let key = parse_at(d, i, depth + 1)?.as_bytes()?;
                let value = parse_at(d, i, depth + 1)?;
                items.push((key, value));
            }
            *i += 1;
            Some(Bencode::Dict(items))
        }
        b'0'..=b'9' => {
            let colon = d[*i..].iter().position(|&c| c == b':')? + *i;
            let len: usize = std::str::from_utf8(&d[*i..colon]).ok()?.parse().ok()?;
            let start = colon + 1;
            let end = start.checked_add(len)?;
            if end > d.len() {
                return None;
            }
            *i = end;
            Some(Bencode::Bytes(&d[start..end]))
        }
        _ => None,
    }
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TorrentMetaInfo {
    pub trackers: Vec<String>,
    pub piece_length: Option<u64>,
    pub private: bool,
    pub comment: Option<String>,
    pub created_by: Option<String>,
    pub creation_date: Option<i64>,
}

pub fn read_metainfo(torrent: &[u8]) -> TorrentMetaInfo {
    let Some(root) = Bencode::parse(torrent) else {
        return TorrentMetaInfo::default();
    };
    let mut trackers: Vec<String> = Vec::new();
    if let Some(list) = root.get("announce-list").and_then(Bencode::as_list) {
        for tier in list {
            for t in tier.as_list().unwrap_or_default() {
                if let Some(s) = t.as_str() {
                    trackers.push(s);
                }
            }
        }
    }
    if let Some(a) = root.get("announce").and_then(Bencode::as_str) {
        trackers.insert(0, a);
    }
    dedup(&mut trackers);
    let info = root.get("info");
    TorrentMetaInfo {
        trackers,
        piece_length: info
            .and_then(|i| i.get("piece length"))
            .and_then(Bencode::as_int)
            .map(|v| v as u64),
        private: info
            .and_then(|i| i.get("private"))
            .and_then(Bencode::as_int)
            == Some(1),
        comment: root.get("comment").and_then(Bencode::as_str),
        created_by: root.get("created by").and_then(Bencode::as_str),
        creation_date: root.get("creation date").and_then(Bencode::as_int),
    }
}

pub fn magnet_trackers(magnet: &str) -> Vec<String> {
    let Ok(url) = url::Url::parse(magnet) else {
        return Vec::new();
    };
    let mut v: Vec<String> = url
        .query_pairs()
        .filter(|(k, _)| k == "tr")
        .map(|(_, v)| v.into_owned())
        .collect();
    dedup(&mut v);
    v
}

pub fn magnet_info_hash(magnet: &str) -> Option<String> {
    let url = url::Url::parse(magnet).ok()?;
    url.query_pairs()
        .filter(|(k, _)| k == "xt")
        .find_map(|(_, v)| {
            v.strip_prefix("urn:btih:")
                .filter(|h| h.len() == 40)
                .map(str::to_ascii_lowercase)
        })
}

pub fn dedup(v: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    v.retain(|t| seen.insert(t.trim().to_string()));
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ScrapeStats {
    pub seeds: u32,
    pub leechers: u32,
    pub completed: u32,
}

pub async fn scrape(
    client: &reqwest::Client,
    tracker: &str,
    info_hash: &[u8; 20],
) -> Result<ScrapeStats> {
    let lower = tracker.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        scrape_http(client, tracker, info_hash).await
    } else if lower.starts_with("udp://") {
        scrape_udp(tracker, info_hash).await
    } else {
        bail!("unsupported tracker protocol")
    }
}

async fn scrape_http(
    client: &reqwest::Client,
    tracker: &str,
    info_hash: &[u8; 20],
) -> Result<ScrapeStats> {
    let (base, query) = match tracker.split_once('?') {
        Some((b, q)) => (b, Some(q)),
        None => (tracker, None),
    };
    let slash = base.rfind('/').ok_or_else(|| anyhow!("bad tracker URL"))?;
    let last = &base[slash + 1..];
    if !last.starts_with("announce") {
        bail!("tracker does not support scrape");
    }
    let mut url = format!(
        "{}/{}",
        &base[..slash],
        last.replacen("announce", "scrape", 1)
    );
    let encoded = percent_encode(info_hash, NON_ALPHANUMERIC).to_string();
    match query {
        Some(q) => url.push_str(&format!("?{q}&info_hash={encoded}")),
        None => url.push_str(&format!("?info_hash={encoded}")),
    }
    let body = client
        .get(&url)
        .timeout(Duration::from_secs(12))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let root = Bencode::parse(&body).ok_or_else(|| anyhow!("invalid scrape answer"))?;
    if let Some(reason) = root.get("failure reason").and_then(Bencode::as_str) {
        bail!(reason);
    }
    let files = root
        .get("files")
        .ok_or_else(|| anyhow!("empty scrape answer"))?;
    let entry = match files {
        Bencode::Dict(items) => items
            .iter()
            .find(|(k, _)| *k == info_hash.as_slice())
            .or_else(|| items.first())
            .map(|(_, v)| v),
        _ => None,
    }
    .ok_or_else(|| anyhow!("torrent not registered on tracker"))?;
    let n = |k: &str| entry.get(k).and_then(Bencode::as_int).unwrap_or(0).max(0) as u32;
    Ok(ScrapeStats {
        seeds: n("complete"),
        leechers: n("incomplete"),
        completed: n("downloaded"),
    })
}

async fn scrape_udp(tracker: &str, info_hash: &[u8; 20]) -> Result<ScrapeStats> {
    let url = url::Url::parse(tracker)?;
    let host = url.host_str().ok_or_else(|| anyhow!("missing host"))?;
    let port = url.port().ok_or_else(|| anyhow!("missing port"))?;
    let addr: SocketAddr = tokio::net::lookup_host((host, port))
        .await?
        .next()
        .ok_or_else(|| anyhow!("could not resolve {host}"))?;
    let bind: SocketAddr = if addr.is_ipv4() {
        "0.0.0.0:0".parse()?
    } else {
        "[::]:0".parse()?
    };
    let sock = UdpSocket::bind(bind).await?;
    sock.connect(addr).await?;

    let tid: u32 = rand_u32();
    let mut req = Vec::with_capacity(16);
    req.extend_from_slice(&0x0417_2710_1980u64.to_be_bytes());
    req.extend_from_slice(&0u32.to_be_bytes());
    req.extend_from_slice(&tid.to_be_bytes());
    let resp = udp_roundtrip(&sock, &req)
        .await
        .context("no answer from tracker")?;
    if resp.len() < 16 || u32::from_be_bytes(resp[0..4].try_into()?) != 0 {
        bail!(udp_error(&resp));
    }
    if u32::from_be_bytes(resp[4..8].try_into()?) != tid {
        bail!("transaction mismatch");
    }
    let conn_id = &resp[8..16];

    let tid2 = rand_u32();
    let mut req = Vec::with_capacity(36);
    req.extend_from_slice(conn_id);
    req.extend_from_slice(&2u32.to_be_bytes());
    req.extend_from_slice(&tid2.to_be_bytes());
    req.extend_from_slice(info_hash);
    let resp = udp_roundtrip(&sock, &req)
        .await
        .context("no scrape answer")?;
    if resp.len() < 20 || u32::from_be_bytes(resp[0..4].try_into()?) != 2 {
        bail!(udp_error(&resp));
    }
    let u = |o: usize| u32::from_be_bytes(resp[o..o + 4].try_into().unwrap_or_default());
    Ok(ScrapeStats {
        seeds: u(8),
        completed: u(12),
        leechers: u(16),
    })
}

fn udp_error(resp: &[u8]) -> String {
    if resp.len() > 8 && u32::from_be_bytes(resp[0..4].try_into().unwrap_or_default()) == 3 {
        String::from_utf8_lossy(&resp[8..]).into_owned()
    } else {
        "unexpected tracker answer".into()
    }
}

async fn udp_roundtrip(sock: &UdpSocket, req: &[u8]) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; 2048];
    for attempt in 0..3u64 {
        sock.send(req).await?;
        match tokio::time::timeout(Duration::from_secs(3 + attempt * 2), sock.recv(&mut buf)).await
        {
            Ok(Ok(n)) => return Ok(buf[..n].to_vec()),
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => continue,
        }
    }
    bail!("timed out")
}

fn rand_u32() -> u32 {
    let b = uuid::Uuid::new_v4();
    u32::from_be_bytes(b.as_bytes()[0..4].try_into().unwrap_or_default())
}

pub const PUBLIC_TRACKERS_URL: &str =
    "https://raw.githubusercontent.com/ngosang/trackerslist/master/trackers_best.txt";

pub async fn fetch_public_trackers(client: &reqwest::Client) -> Result<Vec<String>> {
    let text = client
        .get(PUBLIC_TRACKERS_URL)
        .timeout(Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let mut v: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.contains("://"))
        .map(str::to_string)
        .collect();
    dedup(&mut v);
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bencode_roundtrip() {
        let data = b"d8:announce14:http://t/annou4:infod12:piece lengthi16384e7:privatei1eee";
        let root = Bencode::parse(data).unwrap();
        assert_eq!(
            root.get("announce").unwrap().as_str().unwrap(),
            "http://t/annou"
        );
        let m = read_metainfo(data);
        assert_eq!(m.piece_length, Some(16384));
        assert!(m.private);
        assert_eq!(m.trackers, vec!["http://t/annou".to_string()]);
    }

    #[test]
    fn magnets() {
        let m = "magnet:?xt=urn:btih:ABCDEF0123456789ABCDEF0123456789ABCDEF01&tr=udp%3A%2F%2Ft1%3A80&tr=http%3A%2F%2Ft2%2Fannounce";
        assert_eq!(
            magnet_trackers(m),
            vec!["udp://t1:80", "http://t2/announce"]
        );
        assert_eq!(
            magnet_info_hash(m).as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef01")
        );
    }
}
