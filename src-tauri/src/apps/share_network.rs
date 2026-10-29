//! Shares one file over the local network: a temporary HTTP server bound to the
//! LAN, reachable through an unguessable link (shown as text and QR code).

use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use qrcode::QrCode;
use qrcode::render::svg;
use serde::Serialize;
use tauri::State;
use tiny_http::{Header, Request, Response, Server};

use super::error::{AppError, AppResult, ErrorCode};
use super::file_io::FileInfo;

struct ActiveShare {
    server: Arc<Server>,
    worker: Option<JoinHandle<()>>,
    downloads: Arc<AtomicU32>,
    info: ShareInfo,
}

impl Drop for ActiveShare {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Default)]
pub struct ShareState(Mutex<Option<ActiveShare>>);

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ShareInfo {
    url: String,
    qr_svg: String,
    file: FileInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareStatus {
    active: bool,
    downloads: u32,
    info: Option<ShareInfo>,
}

/// Address other devices can reach: the interface that holds the default route.
fn lan_ip() -> AppResult<IpAddr> {
    netdev::get_default_interface()
        .ok()
        .and_then(|iface| iface.ipv4.first().map(|net| IpAddr::V4(net.addr())))
        .ok_or_else(|| AppError::new(ErrorCode::Unreachable, "no local network connection"))
}

fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

/// Small landing page for phones: file name, size and a download button.
fn landing_page(file: &FileInfo, download_path: &str) -> String {
    let name = html_escape(&file.name);
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{name}</title>
<style>body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#fee8be;font-family:system-ui,sans-serif;color:#512c4c}}
main{{background:#fff4dd;padding:32px;border-radius:28px;box-shadow:8px 10px 20px rgba(81,44,76,.18),inset 6px 6px 12px #fff;text-align:center;max-width:320px;margin:16px}}
h1{{font-size:1.2rem;overflow-wrap:anywhere}}p{{color:#7a4f73}}a{{display:inline-block;margin-top:12px;padding:12px 24px;border-radius:999px;background:#b4501f;color:#fff;text-decoration:none;font-weight:700}}</style></head>
<body><main><p>Minos</p><h1>{name}</h1><p>{size} bytes</p><a href="{download_path}">Download</a></main></body></html>"#,
        size = file.size,
    )
}

fn handle(request: Request, token: &str, path: &PathBuf, file: &FileInfo, downloads: &AtomicU32) {
    let url = request
        .url()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string();
    let page = format!("/{token}");
    let download = format!("/{token}/{}", percent_encode(&file.name));

    let result = if url == page || url == format!("{page}/") {
        request.respond(
            Response::from_string(landing_page(file, &download))
                .with_header(header("Content-Type", "text/html; charset=utf-8")),
        )
    } else if url == download {
        match fs::File::open(path) {
            Ok(handle) => {
                downloads.fetch_add(1, Ordering::Relaxed);
                let ascii_name: String = file
                    .name
                    .chars()
                    .map(|c| {
                        if c.is_ascii_graphic() || c == ' ' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .filter(|c| *c != '"')
                    .collect();
                request.respond(
                    Response::from_file(handle)
                        .with_header(header("Content-Type", "application/octet-stream"))
                        .with_header(header(
                            "Content-Disposition",
                            &format!(
                                "attachment; filename=\"{ascii_name}\"; filename*=UTF-8''{}",
                                percent_encode(&file.name)
                            ),
                        )),
                )
            }
            Err(_) => request
                .respond(Response::from_string("File no longer available").with_status_code(410)),
        }
    } else {
        // Anything without the token gets nothing: the link is the only key.
        request.respond(Response::from_string("Not found").with_status_code(404))
    };
    let _ = result;
}

#[tauri::command]
pub async fn share_start(state: State<'_, ShareState>, path: String) -> AppResult<ShareInfo> {
    let file = FileInfo::read(std::path::Path::new(&path))?;
    let ip = lan_ip()?;
    let server = Arc::new(
        Server::http((ip, 0)).map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))?,
    );
    let port = server
        .server_addr()
        .to_ip()
        .map(|addr| addr.port())
        .ok_or_else(|| AppError::new(ErrorCode::Io, "couldn't read the server port"))?;

    let token = uuid::Uuid::new_v4().simple().to_string();
    let url = format!("http://{ip}:{port}/{token}");
    let qr_svg = QrCode::new(url.as_bytes())
        .map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))?
        .render::<svg::Color>()
        .min_dimensions(220, 220)
        .dark_color(svg::Color("#2a1627"))
        .light_color(svg::Color("#ffffff"))
        .build();

    let downloads = Arc::new(AtomicU32::new(0));
    let worker = {
        let server = Arc::clone(&server);
        let downloads = Arc::clone(&downloads);
        let file = file.clone();
        let path = PathBuf::from(&path);
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                handle(request, &token, &path, &file, &downloads);
            }
        })
    };

    let info = ShareInfo { url, qr_svg, file };
    let mut active = state
        .0
        .lock()
        .map_err(|e| AppError::new(ErrorCode::Io, e.to_string()))?;
    // Replacing the previous share drops it, which stops its server.
    *active = Some(ActiveShare {
        server,
        worker: Some(worker),
        downloads,
        info: info.clone(),
    });
    Ok(info)
}

#[tauri::command]
pub async fn share_status(state: State<'_, ShareState>) -> AppResult<ShareStatus> {
    let active = state
        .0
        .lock()
        .map_err(|e| AppError::new(ErrorCode::Io, e.to_string()))?;
    Ok(match active.as_ref() {
        Some(share) => ShareStatus {
            active: true,
            downloads: share.downloads.load(Ordering::Relaxed),
            info: Some(share.info.clone()),
        },
        None => ShareStatus {
            active: false,
            downloads: 0,
            info: None,
        },
    })
}

#[tauri::command]
pub async fn share_stop(state: State<'_, ShareState>) -> AppResult<()> {
    let previous = state
        .0
        .lock()
        .map_err(|e| AppError::new(ErrorCode::Io, e.to_string()))?
        .take();
    drop(previous); // stops the server outside the lock
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    fn get(addr: std::net::SocketAddr, path: &str) -> String {
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(stream, "GET {path} HTTP/1.0\r\nHost: test\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn serves_only_with_the_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hola señor.txt");
        fs::write(&path, "contenido").unwrap();
        let file = FileInfo::read(&path).unwrap();

        let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
        let addr = server.server_addr().to_ip().unwrap();
        let downloads = Arc::new(AtomicU32::new(0));
        let worker = {
            let (server, downloads, file, path) = (
                Arc::clone(&server),
                Arc::clone(&downloads),
                file.clone(),
                path.clone(),
            );
            std::thread::spawn(move || {
                for request in server.incoming_requests() {
                    handle(request, "secret", &path, &file, &downloads);
                }
            })
        };

        let root = get(addr, "/");
        assert!(
            root.starts_with("HTTP/1.1 404") || root.starts_with("HTTP/1.0 404"),
            "{root}"
        );
        assert!(get(addr, "/secret").contains("hola señor.txt"));
        let body = get(
            addr,
            &format!("/secret/{}", percent_encode("hola señor.txt")),
        );
        assert!(body.contains("attachment;") && body.ends_with("contenido"));
        assert_eq!(downloads.load(Ordering::Relaxed), 1);

        server.unblock();
        worker.join().unwrap();
    }
}
