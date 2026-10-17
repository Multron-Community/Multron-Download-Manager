use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::str::FromStr;

use anyhow::{bail, Context, Result};
use serde::Serialize;

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct FilterStatus {
    pub rules: usize,
    pub loaded_at: Option<i64>,
    pub error: Option<String>,
}

pub async fn build(
    client: &reqwest::Client,
    sources: &[String],
    manual: &str,
    out: &Path,
) -> Result<usize> {
    let mut lines: Vec<String> = Vec::new();
    for src in sources.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let raw = if src.starts_with("http://") || src.starts_with("https://") {
            client
                .get(src)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?
                .to_vec()
        } else {
            tokio::fs::read(src.trim_start_matches("file://"))
                .await
                .with_context(|| format!("could not read {src}"))?
        };
        let text = tokio::task::spawn_blocking(move || decode(raw)).await??;
        let before = lines.len();
        append_ranges(&text, &mut lines);
        if lines.len() == before {
            bail!("no IP ranges found in {src}");
        }
    }
    append_ranges(manual, &mut lines);
    if let Some(parent) = out.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(out, lines.join("\n")).await?;
    Ok(lines.len())
}

fn decode(raw: Vec<u8>) -> Result<String> {
    if raw.starts_with(&[0x1f, 0x8b]) {
        let mut s = String::new();
        flate2::read::GzDecoder::new(&raw[..]).read_to_string(&mut s)?;
        return Ok(s);
    }
    if raw.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(raw))?;
        let mut out = String::new();
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            if entry.is_file() {
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                out.push_str(&String::from_utf8_lossy(&buf));
                out.push('\n');
            }
        }
        return Ok(out);
    }
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

fn append_ranges(text: &str, out: &mut Vec<String>) {
    for line in text.lines() {
        if let Some((a, b)) = parse_line(line) {
            out.push(format!("mdm:{a}-{b}"));
        }
    }
}

fn parse_ip(s: &str) -> Option<IpAddr> {
    let s = s.trim();
    if let Ok(ip) = IpAddr::from_str(s) {
        return Some(ip);
    }
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() == 4 {
        let mut o = [0u8; 4];
        for (i, p) in parts.iter().enumerate() {
            o[i] = p.parse().ok()?;
        }
        return Some(IpAddr::V4(Ipv4Addr::from(o)));
    }
    None
}

pub fn parse_line(line: &str) -> Option<(IpAddr, IpAddr)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
        return None;
    }
    let core = if line.contains(',') {
        line.split(',').next()?
    } else {
        line
    };
    if let Some(dash) = core.rfind('-') {
        let (left, right) = (&core[..dash], &core[dash + 1..]);
        let end = parse_ip(right)?;
        let start = parse_ip(left).or_else(|| {
            left.char_indices()
                .filter(|(_, c)| *c == ':')
                .find_map(|(i, _)| parse_ip(&left[i + 1..]))
        })?;
        return (start.is_ipv4() == end.is_ipv4()).then_some((start, end));
    }
    if let Some((addr, len)) = core.split_once('/') {
        let len: u32 = len.trim().parse().ok()?;
        return match parse_ip(addr)? {
            IpAddr::V4(a) if len <= 32 => {
                let mask = if len == 0 { 0 } else { u32::MAX << (32 - len) };
                let start = a.to_bits() & mask;
                Some((
                    IpAddr::V4(Ipv4Addr::from_bits(start)),
                    IpAddr::V4(Ipv4Addr::from_bits(start | !mask)),
                ))
            }
            IpAddr::V6(a) if len <= 128 => {
                let mask = if len == 0 {
                    0
                } else {
                    u128::MAX << (128 - len)
                };
                let start = a.to_bits() & mask;
                Some((
                    IpAddr::V6(Ipv6Addr::from_bits(start)),
                    IpAddr::V6(Ipv6Addr::from_bits(start | !mask)),
                ))
            }
            _ => None,
        };
    }
    let ip = parse_ip(core)?;
    Some((ip, ip))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(s: &str) -> Option<(String, String)> {
        parse_line(s).map(|(a, b)| (a.to_string(), b.to_string()))
    }

    #[test]
    fn formats() {
        assert_eq!(
            r("001.002.004.000 - 001.002.004.255 , 000 , Bad"),
            Some(("1.2.4.0".into(), "1.2.4.255".into()))
        );
        assert_eq!(
            r("Some org: inc:10.0.0.0-10.0.0.255"),
            Some(("10.0.0.0".into(), "10.0.0.255".into()))
        );
        assert_eq!(
            r("192.168.0.0/16"),
            Some(("192.168.0.0".into(), "192.168.255.255".into()))
        );
        assert_eq!(r("8.8.8.8"), Some(("8.8.8.8".into(), "8.8.8.8".into())));
        assert_eq!(
            r("v6:2001:db8::1-2001:db8::ff"),
            Some(("2001:db8::1".into(), "2001:db8::ff".into()))
        );
        assert_eq!(r("# comment"), None);
    }
}
