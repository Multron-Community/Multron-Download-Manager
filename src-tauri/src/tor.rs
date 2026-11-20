use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_PROXY: &str = "socks5h://127.0.0.1:9050";
const PORTS: [u16; 2] = [9050, 9150];
const CHECK_URL: &str = "https://check.torproject.org/api/ip";

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TorCheck {
    pub is_tor: bool,
    pub ip: String,
}

pub fn valid_proxy(proxy: &str) -> bool {
    let Ok(u) = url::Url::parse(proxy.trim()) else {
        return false;
    };
    matches!(u.scheme(), "socks5" | "socks5h") && u.host_str().is_some() && u.port().is_some()
}

pub async fn detect() -> Option<String> {
    for port in PORTS {
        let probe = tokio::time::timeout(
            Duration::from_millis(600),
            tokio::net::TcpStream::connect(("127.0.0.1", port)),
        )
        .await;
        if matches!(probe, Ok(Ok(_))) {
            return Some(format!("socks5h://127.0.0.1:{port}"));
        }
    }
    None
}

#[derive(Deserialize)]
struct Answer {
    #[serde(rename = "IsTor")]
    is_tor: bool,
    #[serde(rename = "IP")]
    ip: String,
}

pub async fn check(proxy: &str) -> Result<TorCheck> {
    if !valid_proxy(proxy) {
        bail!("enter a SOCKS5 address such as {DEFAULT_PROXY}");
    }
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(proxy.trim())?)
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(40))
        .build()?;
    let answer: Answer = client
        .get(CHECK_URL)
        .send()
        .await
        .context("Tor could not be reached. Start Tor Browser or the Tor service and try again")?
        .json()
        .await
        .context("the Tor check page answered with something unexpected")?;
    Ok(TorCheck {
        is_tor: answer.is_tor,
        ip: answer.ip,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_socks5_addresses() {
        assert!(valid_proxy(DEFAULT_PROXY));
        assert!(valid_proxy("socks5://127.0.0.1:9150"));
        assert!(!valid_proxy("http://127.0.0.1:8118"));
        assert!(!valid_proxy("socks5h://127.0.0.1"));
        assert!(!valid_proxy("not a url"));
    }
}
