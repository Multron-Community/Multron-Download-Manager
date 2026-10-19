use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path as AxPath, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::manager::{BridgeStatus, Manager, Pending};
use crate::model::AddRequest;

const EXTENSION_SCHEMES: [&str; 4] = [
    "chrome-extension://",
    "moz-extension://",
    "safari-web-extension://",
    "extension://",
];

fn origin(headers: &HeaderMap) -> Option<String> {
    headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim_end_matches('/').to_string())
}

fn is_extension(origin: &str) -> bool {
    EXTENSION_SCHEMES.iter().any(|s| origin.starts_with(s))
}

pub fn restart(mgr: Arc<Manager>) {
    let token = CancellationToken::new();
    {
        let mut current = mgr.bridge_cancel.lock();
        current.cancel();
        *current = token.clone();
    }
    let settings = mgr.settings();
    if !settings.bridge_enabled {
        *mgr.bridge.lock() = BridgeStatus {
            running: false,
            port: settings.bridge_port,
            error: None,
        };
        return;
    }
    tauri::async_runtime::spawn(serve(mgr, settings.bridge_port, token));
}

async fn serve(mgr: Arc<Manager>, port: u16, cancel: CancellationToken) {
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
        Ok(l) => l,
        Err(e) => {
            *mgr.bridge.lock() = BridgeStatus {
                running: false,
                port,
                error: Some(format!("could not listen on 127.0.0.1:{port}: {e}")),
            };
            return;
        }
    };
    *mgr.bridge.lock() = BridgeStatus {
        running: true,
        port,
        error: None,
    };
    let app = Router::new()
        .route("/api/ping", get(ping).post(ping))
        .route("/api/pair", post(pair))
        .route("/api/download", post(download))
        .route("/api/batch", post(batch))
        .route("/stream/{token}/{hash}/{file}", get(stream))
        .layer(middleware::from_fn(guard))
        .with_state(mgr.clone());
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(cancel.clone().cancelled_owned())
        .await;
    let mut status = mgr.bridge.lock();
    if status.port == port {
        status.running = false;
        if let Err(e) = result {
            status.error = Some(e.to_string());
        }
    }
}

async fn guard(req: Request, next: Next) -> Response {
    let origin = origin(req.headers());
    if let Some(o) = &origin {
        if !is_extension(o) {
            return (StatusCode::FORBIDDEN, Json(json!({ "error": "forbidden" }))).into_response();
        }
    }
    let (method, path) = (req.method().clone(), req.uri().path().to_string());
    let mut resp = if req.method() == Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(req).await
    };
    tracing::debug!(
        "bridge {method} {path} from {} -> {}",
        origin.as_deref().unwrap_or("no origin"),
        resp.status().as_u16()
    );
    if let Some(o) = origin.and_then(|o| HeaderValue::from_str(&o).ok()) {
        let h = resp.headers_mut();
        h.insert("access-control-allow-origin", o);
        h.insert(
            "access-control-allow-methods",
            HeaderValue::from_static("GET, POST, OPTIONS"),
        );
        h.insert(
            "access-control-allow-headers",
            HeaderValue::from_static("content-type"),
        );
        h.insert("vary", HeaderValue::from_static("origin"));
    }
    resp
}

fn paired(mgr: &Manager, origin: Option<&str>) -> bool {
    let s = mgr.settings();
    if !s.bridge_require_pairing {
        return true;
    }
    origin
        .map(|o| s.trusted_extensions.iter().any(|t| t == o))
        .unwrap_or(false)
}

async fn ping(State(mgr): State<Arc<Manager>>, headers: HeaderMap) -> Json<Value> {
    let o = origin(&headers);
    let s = mgr.settings();
    let uninstall = o
        .as_deref()
        .map(|o| mgr.uninstall_requests.lock().remove(o))
        .unwrap_or(false);
    let is_paired = paired(&mgr, o.as_deref());
    if let Some(o) = o
        .as_deref()
        .filter(|o| !is_paired && !uninstall && is_extension(o))
    {
        if mgr.pair_asked.lock().insert(o.to_string()) {
            mgr.push_pending(Pending::Pair {
                origin: o.to_string(),
            });
        }
    }
    Json(json!({
        "name": "multron",
        "version": env!("CARGO_PKG_VERSION"),
        "uninstall": uninstall,
        "paired": is_paired,
        "askSha256": s.ask_sha256,
        "mediaEnabled": s.media_enabled,
        "torrentEnabled": s.torrent_enabled,
    }))
}

async fn pair(State(mgr): State<Arc<Manager>>, headers: HeaderMap) -> (StatusCode, Json<Value>) {
    let Some(o) = origin(&headers) else {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "missing origin" })),
        );
    };
    if paired(&mgr, Some(&o)) {
        return (StatusCode::OK, Json(json!({ "paired": true })));
    }
    mgr.push_pending(Pending::Pair { origin: o });
    (
        StatusCode::ACCEPTED,
        Json(
            json!({ "paired": false, "message": "Approve the connection in Multron Download Manager" }),
        ),
    )
}

async fn stream(
    State(mgr): State<Arc<Manager>>,
    AxPath((token, hash, file)): AxPath<(String, String, usize)>,
    headers: HeaderMap,
) -> Response {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    if token != mgr.stream_token() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let (mut reader, len, name) = match mgr.open_stream(&hash, file).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::NOT_FOUND, format!("{e:#}")).into_response(),
    };
    if len == 0 {
        return StatusCode::NO_CONTENT.into_response();
    }
    let range = headers
        .get(axum::http::header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="))
        .and_then(|v| v.split(',').next())
        .and_then(|v| {
            let (a, b) = v.split_once('-')?;
            match (a.trim().parse::<u64>().ok(), b.trim().parse::<u64>().ok()) {
                (Some(start), end) => Some((start, end.unwrap_or(len - 1).min(len - 1))),
                (None, Some(suffix)) => Some((len.saturating_sub(suffix), len - 1)),
                _ => None,
            }
        });
    if let Some((start, end)) = range {
        if start > end || start >= len {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header("content-range", format!("bytes */{len}"))
                .body(Body::empty())
                .unwrap_or_else(|_| StatusCode::RANGE_NOT_SATISFIABLE.into_response());
        }
    }
    let (start, end) = range.unwrap_or((0, len - 1));
    if start > 0 && reader.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let body = Body::from_stream(tokio_util::io::ReaderStream::new(
        reader.take(end - start + 1),
    ));
    let mime = mime_guess::from_path(&name)
        .first_or_octet_stream()
        .to_string();
    let mut builder = Response::builder()
        .status(if range.is_some() {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header("content-type", mime)
        .header("accept-ranges", "bytes")
        .header("content-length", (end - start + 1).to_string());
    if range.is_some() {
        builder = builder.header("content-range", format!("bytes {start}-{end}/{len}"));
    }
    builder
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn batch(
    State(mgr): State<Arc<Manager>>,
    headers: HeaderMap,
    Json(batch): Json<crate::model::PageBatch>,
) -> (StatusCode, Json<Value>) {
    let o = origin(&headers);
    if !paired(&mgr, o.as_deref()) {
        if let Some(o) = o {
            mgr.push_pending(Pending::Pair { origin: o });
        }
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "pairing_required" })),
        );
    }
    if batch.items.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "nothing to download on this page" })),
        );
    }
    let count = batch.items.len();
    mgr.push_pending(Pending::Batch {
        batch: Box::new(batch),
    });
    (StatusCode::OK, Json(json!({ "ok": true, "items": count })))
}

async fn download(
    State(mgr): State<Arc<Manager>>,
    headers: HeaderMap,
    Json(req): Json<AddRequest>,
) -> (StatusCode, Json<Value>) {
    let o = origin(&headers);
    if !paired(&mgr, o.as_deref()) {
        if let Some(o) = o {
            mgr.push_pending(Pending::Pair { origin: o });
        }
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "pairing_required" })),
        );
    }
    if req.url.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing url" })),
        );
    }
    let s = mgr.settings();
    if s.confirm_browser_downloads || s.ask_sha256 {
        mgr.push_pending(Pending::Add {
            request: Box::new(req),
        });
        return (StatusCode::OK, Json(json!({ "ok": true, "confirm": true })));
    }
    match mgr.add(req).await {
        Ok(id) => (StatusCode::OK, Json(json!({ "ok": true, "id": id }))),
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": format!("{e:#}") })),
        ),
    }
}
