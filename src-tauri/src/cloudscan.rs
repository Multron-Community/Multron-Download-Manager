use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

pub const SERVER: &str = "wss://api.viruskov.com/ws";
pub const DEFAULT_LIMIT_MB: u32 = 15;
pub const MAX_LIMIT_MB: u32 = 100;
const PROTOCOL: u64 = 3;
const CLIENT: &str = "MultronDownloadManager";
const CHUNK: usize = 256 * 1024;
const MAX_MESSAGE: usize = 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RESULT_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, PartialEq)]
pub struct FileVerdict {
    pub verdict: String,
    pub detail: String,
}

impl FileVerdict {
    fn new(verdict: &str, detail: impl Into<String>) -> Self {
        Self {
            verdict: verdict.into(),
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub state: &'static str,
    pub detail: String,
}

pub async fn scan(files: &[PathBuf], limit_mb: u32) -> Summary {
    let limit = u64::from(limit_mb.clamp(1, MAX_LIMIT_MB)) * 1024 * 1024;
    match connect_tls(SERVER).await {
        Ok(stream) => match Session::open(stream, SERVER).await {
            Ok(mut session) => {
                let mut verdicts = Vec::new();
                for file in files {
                    verdicts.push((file.clone(), session.scan_file(file, limit).await));
                }
                session.close().await;
                summarize(&verdicts, limit_mb)
            }
            Err(e) => failed(&e),
        },
        Err(e) => failed(&e),
    }
}

fn failed(e: &anyhow::Error) -> Summary {
    Summary {
        state: "error",
        detail: format!("Multron Cloud: {e:#}"),
    }
}

async fn connect_tls(
    server: &str,
) -> Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>> {
    let url = url::Url::parse(server)?;
    if url.scheme() != "wss" {
        bail!("the cloud scan server must use wss://");
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("the cloud scan address has no host"))?
        .to_string();
    let port = url.port().unwrap_or(443);
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?;
    let config = rustls_platform_verifier::BuilderVerifierExt::with_platform_verifier(config)?
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let name = rustls::pki_types::ServerName::try_from(host.clone())?;
    tokio::time::timeout(CONNECT_TIMEOUT, async {
        let tcp = tokio::net::TcpStream::connect((host.as_str(), port)).await?;
        Ok::<_, anyhow::Error>(connector.connect(name, tcp).await?)
    })
    .await
    .map_err(|_| anyhow!("the cloud server did not answer in time"))?
}

struct Session<S> {
    ws: WebSocketStream<S>,
    check: bool,
    next_id: u64,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Session<S> {
    async fn open(stream: S, server: &str) -> Result<Self> {
        let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE))
            .max_frame_size(Some(MAX_MESSAGE));
        let (ws, _) = tokio::time::timeout(
            CONNECT_TIMEOUT,
            tokio_tungstenite::client_async_with_config(server, stream, Some(config)),
        )
        .await
        .map_err(|_| anyhow!("the cloud server did not answer in time"))??;
        let mut session = Self {
            ws,
            check: false,
            next_id: 0,
        };
        session
            .send(json!({ "type": "hello", "version": PROTOCOL, "client": CLIENT, "token": "" }))
            .await?;
        let reply = tokio::time::timeout(CONNECT_TIMEOUT, session.receive())
            .await
            .map_err(|_| anyhow!("the cloud server did not answer in time"))??;
        if reply["type"] != "hello_ok" {
            let why = reply["message"]
                .as_str()
                .or(reply["type"].as_str())
                .unwrap_or("no reason");
            bail!("the server rejected the connection: {why}");
        }
        session.check = reply["version"].as_u64().unwrap_or(2) >= 3;
        Ok(session)
    }

    async fn send(&mut self, value: Value) -> Result<()> {
        self.ws.send(Message::text(value.to_string())).await?;
        Ok(())
    }

    async fn receive(&mut self) -> Result<Value> {
        loop {
            let message = self
                .ws
                .next()
                .await
                .ok_or_else(|| anyhow!("the server closed the connection"))??;
            match message {
                Message::Text(text) => return Ok(serde_json::from_str(text.as_str())?),
                Message::Close(_) => bail!("the server closed the connection"),
                Message::Binary(_) => bail!("unexpected binary message from the server"),
                _ => {}
            }
        }
    }

    async fn answer(&mut self, id: u64) -> Result<Value> {
        tokio::time::timeout(RESULT_TIMEOUT, async {
            loop {
                let message = self.receive().await?;
                match message["id"].as_u64() {
                    Some(n) if n == id => return Ok(message),
                    None | Some(0) if message["type"] == "error" => {
                        bail!(
                            "server error: {}",
                            message["message"].as_str().unwrap_or("unknown")
                        )
                    }
                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| anyhow!("the cloud server did not answer in time"))?
    }

    async fn scan_file(&mut self, path: &Path, limit: u64) -> FileVerdict {
        match self.try_scan_file(path, limit).await {
            Ok(v) => v,
            Err(e) => FileVerdict::new("error", format!("{e:#}")),
        }
    }

    async fn try_scan_file(&mut self, path: &Path, limit: u64) -> Result<FileVerdict> {
        let (sha256, size) = hash_file(path)
            .await
            .with_context(|| format!("cannot read {}", path.display()))?;
        self.next_id += 1;
        let id = self.next_id;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let folder = normalize_folder(path);
        if self.check {
            self.send(json!({
                "type": "check",
                "items": [{ "id": id, "name": name, "size": size, "sha256": sha256, "folder": folder }]
            }))
            .await?;
            let answer = self.answer(id).await?;
            if answer["type"] != "need_upload" {
                return Ok(verdict_of(&answer));
            }
        }
        if size > limit {
            return Ok(FileVerdict::new(
                "skipped",
                format!("larger than the {} MB upload limit", limit / (1024 * 1024)),
            ));
        }
        let data = tokio::fs::read(path)
            .await
            .with_context(|| format!("cannot read {}", path.display()))?;
        if data.len() as u64 > limit {
            return Ok(FileVerdict::new(
                "skipped",
                format!("larger than the {} MB upload limit", limit / (1024 * 1024)),
            ));
        }
        let sha256 = hex::encode_upper(Sha256::digest(&data));
        self.send(json!({
            "type": "scan", "id": id, "name": name, "size": data.len(), "sha256": sha256, "folder": folder
        }))
        .await?;
        let ack = self.answer(id).await?;
        if ack["type"] != "send_file" {
            return Ok(verdict_of(&ack));
        }
        for chunk in data.chunks(CHUNK) {
            self.ws.send(Message::binary(chunk.to_vec())).await?;
        }
        drop(data);
        let reply = self.answer(id).await?;
        Ok(verdict_of(&reply))
    }

    async fn close(&mut self) {
        let _ = tokio::time::timeout(Duration::from_secs(3), self.ws.close(None)).await;
    }
}

async fn hash_file(path: &Path) -> Result<(String, u64)> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok((hex::encode_upper(hasher.finalize()), size))
}

fn verdict_of(reply: &Value) -> FileVerdict {
    if reply["type"] != "result" {
        return FileVerdict::new("error", reply["message"].as_str().unwrap_or("server error"));
    }
    let av = &reply["antivirus"];
    let verdict = av["verdict"]
        .as_str()
        .filter(|v| !v.is_empty())
        .unwrap_or("unknown")
        .to_ascii_lowercase();
    let threat = reply["threat"]["indicator"]["name"]
        .as_str()
        .or(reply["rule"]["name"].as_str())
        .unwrap_or("");
    let detections: Vec<String> = av["detections"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|d| {
                    let name = d["name"].as_str()?;
                    let layer = d["layer"].as_str().unwrap_or("");
                    Some(if layer.is_empty() {
                        name.to_string()
                    } else {
                        format!("{name} ({layer})")
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let detail = if !threat.is_empty() {
        threat.to_string()
    } else if !detections.is_empty() {
        detections.join(", ")
    } else {
        av["detail"].as_str().unwrap_or("").to_string()
    };
    FileVerdict::new(&verdict, detail)
}

fn summarize(verdicts: &[(PathBuf, FileVerdict)], limit_mb: u32) -> Summary {
    let named = |wanted: &[&str]| -> Vec<String> {
        verdicts
            .iter()
            .filter(|(_, v)| wanted.contains(&v.verdict.as_str()))
            .map(|(p, v)| {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if v.detail.is_empty() {
                    name
                } else {
                    format!("{name}: {}", v.detail)
                }
            })
            .collect()
    };
    let threats = named(&["malicious"]);
    if !threats.is_empty() {
        return Summary {
            state: "threat",
            detail: format!("Multron Cloud: {}", threats.join(", ")),
        };
    }
    let suspicious = named(&["suspicious"]);
    if !suspicious.is_empty() {
        return Summary {
            state: "suspicious",
            detail: format!("Multron Cloud: {}", suspicious.join(", ")),
        };
    }
    let clean = verdicts
        .iter()
        .filter(|(_, v)| matches!(v.verdict.as_str(), "clean" | "possible_clean"))
        .count();
    if clean == verdicts.len() && clean > 0 {
        return Summary {
            state: "clean",
            detail: "Multron Cloud".into(),
        };
    }
    let errors = named(&["error", "unknown"]);
    if !errors.is_empty() {
        return Summary {
            state: "error",
            detail: format!("Multron Cloud: {}", errors.join(", ")),
        };
    }
    Summary {
        state: "skipped",
        detail: format!(
            "Multron Cloud: {} of {} files are larger than the {limit_mb} MB upload limit",
            verdicts.len() - clean,
            verdicts.len()
        ),
    }
}

fn normalize_folder(path: &Path) -> String {
    let Some(dir) = path.parent() else {
        return String::new();
    };
    let dir = dir
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_string();
    let mut places: Vec<(&str, String)> = Vec::new();
    for (name, var) in [
        ("%TEMP%", "TEMP"),
        ("%LOCALAPPDATA%", "LOCALAPPDATA"),
        ("%APPDATA%", "APPDATA"),
        ("%USERPROFILE%", "USERPROFILE"),
        ("%USERPROFILE%", "HOME"),
    ] {
        if let Some(value) = std::env::var_os(var) {
            let value = value
                .to_string_lossy()
                .trim_end_matches(['\\', '/'])
                .to_string();
            if !value.is_empty() {
                places.push((name, value));
            }
        }
    }
    places.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    let folded = |s: &str| {
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s.to_string()
        }
    };
    let lower = folded(&dir);
    for (name, value) in &places {
        let v = folded(value);
        if lower == v {
            return name.to_string();
        }
        if lower.starts_with(&format!("{v}\\")) || lower.starts_with(&format!("{v}/")) {
            let rest = &dir[value.len()..];
            return format!("{name}{rest}").chars().take(260).collect();
        }
    }
    dir.rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .chars()
        .take(260)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mdm-cloud-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    async fn fake_server(
        known: Option<&'static str>,
    ) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(tcp).await.unwrap();
            let mut uploaded = Vec::new();
            let mut pending_id = 0;
            while let Some(Ok(msg)) = ws.next().await {
                match msg {
                    Message::Text(text) => {
                        let v: Value = serde_json::from_str(text.as_str()).unwrap();
                        let reply = match v["type"].as_str().unwrap() {
                            "hello" => {
                                assert_eq!(v["client"], CLIENT);
                                json!({ "type": "hello_ok", "version": 3 })
                            }
                            "check" => {
                                let item = &v["items"][0];
                                let id = item["id"].as_u64().unwrap();
                                assert!(item.get("path").is_none());
                                if Some(item["sha256"].as_str().unwrap()) == known {
                                    json!({ "type": "result", "id": id, "antivirus": { "verdict": "malicious" }, "threat": { "indicator": { "name": "Test.Eicar" } } })
                                } else {
                                    json!({ "type": "need_upload", "id": id })
                                }
                            }
                            "scan" => {
                                pending_id = v["id"].as_u64().unwrap();
                                json!({ "type": "send_file", "id": pending_id })
                            }
                            _ => continue,
                        };
                        ws.send(Message::text(reply.to_string())).await.unwrap();
                    }
                    Message::Binary(data) => {
                        uploaded.extend_from_slice(&data);
                        let reply = json!({ "type": "result", "id": pending_id, "antivirus": { "verdict": "clean" } });
                        ws.send(Message::text(reply.to_string())).await.unwrap();
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            uploaded
        });
        (format!("ws://{addr}/ws"), handle)
    }

    async fn run(server: &str, files: &[PathBuf], limit_mb: u32) -> Summary {
        let url = url::Url::parse(server).unwrap();
        let tcp = tokio::net::TcpStream::connect((url.host_str().unwrap(), url.port().unwrap()))
            .await
            .unwrap();
        let mut session = Session::open(tcp, server).await.unwrap();
        let limit = u64::from(limit_mb) * 1024 * 1024;
        let mut verdicts = Vec::new();
        for f in files {
            verdicts.push((f.clone(), session.scan_file(f, limit).await));
        }
        session.close().await;
        summarize(&verdicts, limit_mb)
    }

    #[tokio::test]
    async fn uploads_unknown_files_and_reports_clean() {
        let dir = temp("clean");
        let file = dir.join("setup.exe");
        std::fs::write(&file, b"hello from multron").unwrap();
        let (server, handle) = fake_server(None).await;
        let summary = run(&server, std::slice::from_ref(&file), 15).await;
        assert_eq!(summary.state, "clean");
        assert_eq!(handle.await.unwrap(), b"hello from multron");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn known_hashes_are_not_uploaded() {
        let dir = temp("known");
        let file = dir.join("bad.exe");
        std::fs::write(&file, b"bad").unwrap();
        let hash: &'static str =
            Box::leak(hex::encode_upper(Sha256::digest(b"bad")).into_boxed_str());
        let (server, handle) = fake_server(Some(hash)).await;
        let summary = run(&server, std::slice::from_ref(&file), 15).await;
        assert_eq!(summary.state, "threat");
        assert!(summary.detail.contains("Test.Eicar"));
        assert!(handle.await.unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn files_over_the_limit_are_not_uploaded() {
        let dir = temp("large");
        let file = dir.join("big.bin");
        std::fs::write(&file, vec![7u8; 1024 * 1024 + 1]).unwrap();
        let (server, handle) = fake_server(None).await;
        let summary = run(&server, std::slice::from_ref(&file), 1).await;
        assert_eq!(summary.state, "skipped");
        assert!(summary.detail.contains("1 MB"));
        assert!(handle.await.unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hides_the_user_name_in_folders() {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap();
        let path = PathBuf::from(&home).join("Downloads").join("a.exe");
        let folder = normalize_folder(&path);
        assert!(folder.starts_with("%USERPROFILE%"), "{folder}");
        assert!(!folder.contains(&home));
        let other = normalize_folder(Path::new("/srv/private/team/a.exe"));
        assert_eq!(other, "team");
    }

    #[test]
    fn rejects_plain_websocket_servers() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(connect_tls("ws://example.org/ws")).unwrap_err();
        assert!(err.to_string().contains("wss://"));
    }
}
