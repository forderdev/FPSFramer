//! Development / test server.
//!
//! Runs the real engine and serves the built UI over plain HTTP on localhost so the
//! interface can be exercised in a browser (and by automated end-to-end tests) without
//! the native shell. It is **not** part of the installed application.
//!
//! ```text
//! fpsframer-devserver [--demo] [--port 5757] [--ui-dir ui/dist] [--data-dir DIR]
//! ```
//!
//! * `POST /api/rpc`   – `{ "method": "...", "params": {...} }` → `{ "ok": ... }` or `{ "error": {...} }`
//! * `GET  /api/events` – server-sent events carrying the engine's `Event`s
//! * anything else     – static files from the UI folder

use fpsframer_core::storage::Paths;
use fpsframer_engine::{Engine, EngineOptions, RpcError};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

struct Args {
    demo: bool,
    port: u16,
    ui_dir: PathBuf,
    data_dir: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut a = Args { demo: false, port: 5757, ui_dir: PathBuf::from("ui/dist"), data_dir: None };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.as_str() {
            "--demo" => a.demo = true,
            "--port" => a.port = it.next().and_then(|v| v.parse().ok()).unwrap_or(5757),
            "--ui-dir" => a.ui_dir = it.next().map(PathBuf::from).unwrap_or(a.ui_dir),
            "--data-dir" => a.data_dir = it.next().map(PathBuf::from),
            other => eprintln!("ignoring unknown argument {other}"),
        }
    }
    a
}

struct Request {
    method: String,
    path: String,
    host: String,
    origin: Option<String>,
    body: Vec<u8>,
}

fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut r = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    r.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let (mut len, mut host, mut origin) = (0usize, String::new(), None);
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).ok()? == 0 {
            return None;
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            match k.trim().to_lowercase().as_str() {
                "content-length" => len = v.trim().parse().unwrap_or(0),
                "host" => host = v.trim().to_string(),
                "origin" => origin = Some(v.trim().to_string()),
                _ => {}
            }
        }
    }
    if len > 32 * 1024 * 1024 {
        return None;
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).ok()?;
    Some(Request { method, path, host, origin, body })
}

fn respond(stream: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn mime(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn serve_static(stream: &mut TcpStream, root: &Path, url_path: &str) {
    let clean = url_path.split(['?', '#']).next().unwrap_or("/");
    let rel = clean.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    let Ok(root_c) = root.canonicalize() else {
        return respond(stream, "500 Internal Server Error", "text/plain", b"UI folder not found. Run `npm run build` in ui/ first.");
    };
    let candidate = root_c.join(rel);
    let file = match candidate.canonicalize() {
        Ok(f) if f.starts_with(&root_c) && f.is_file() => f,
        _ => {
            // Single-page app fallback for extension-less paths.
            if Path::new(rel).extension().is_none() {
                root_c.join("index.html")
            } else {
                return respond(stream, "404 Not Found", "text/plain", b"not found");
            }
        }
    };
    match std::fs::read(&file) {
        Ok(b) => respond(stream, "200 OK", mime(&file), &b),
        Err(_) => respond(stream, "404 Not Found", "text/plain", b"not found"),
    }
}

fn events(mut stream: TcpStream, engine: Engine) {
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\n";
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }
    let rx = engine.subscribe();
    engine.set_ui_visible(true);
    loop {
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(ev) => {
                let Ok(json) = serde_json::to_string(&*ev) else { continue };
                if stream.write_all(format!("data: {json}\n\n").as_bytes()).and_then(|_| stream.flush()).is_err() {
                    break;
                }
            }
            Err(crossbeam_timeout) => {
                // Heartbeat so dead connections are noticed.
                let _ = crossbeam_timeout;
                if stream.write_all(b": ping\n\n").and_then(|_| stream.flush()).is_err() {
                    break;
                }
            }
        }
    }
    engine.set_ui_visible(false);
}

fn handle(mut stream: TcpStream, engine: Engine, ui_dir: PathBuf, port: u16) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let Some(req) = read_request(&stream) else { return };
    // Only the loopback names may be used – blocks DNS-rebinding style access from web pages.
    let ok_hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if !ok_hosts.contains(&req.host) {
        return respond(&mut stream, "403 Forbidden", "text/plain", b"bad host");
    }
    if let Some(o) = &req.origin {
        if o != &format!("http://127.0.0.1:{port}") && o != &format!("http://localhost:{port}") {
            return respond(&mut stream, "403 Forbidden", "text/plain", b"bad origin");
        }
    }
    let path = req.path.split('?').next().unwrap_or("/");
    match (req.method.as_str(), path) {
        ("POST", "/api/rpc") => {
            let out: Value = match serde_json::from_slice::<Value>(&req.body) {
                Ok(v) => {
                    let method = v.get("method").and_then(Value::as_str).unwrap_or("").to_string();
                    let params = v.get("params").cloned().unwrap_or(Value::Null);
                    match engine.call(&method, params) {
                        Ok(r) => json!({ "ok": r }),
                        Err(e) => json!({ "error": e }),
                    }
                }
                Err(e) => json!({ "error": RpcError::bad(format!("Invalid JSON: {e}")) }),
            };
            respond(&mut stream, "200 OK", "application/json", out.to_string().as_bytes());
        }
        ("GET", "/api/events") => events(stream, engine),
        ("GET", _) => serve_static(&mut stream, &ui_dir, &req.path),
        _ => respond(&mut stream, "405 Method Not Allowed", "text/plain", b"method not allowed"),
    }
}

fn main() {
    let args = parse_args();
    if let Some(d) = &args.data_dir {
        std::env::set_var("FPSFRAMER_DATA_DIR", d);
    }
    let engine = Engine::new(EngineOptions { paths: Paths::from_env(), demo: args.demo, version: format!("{}-dev", env!("CARGO_PKG_VERSION")) });
    engine.start();
    let listener = match TcpListener::bind(("127.0.0.1", args.port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Could not listen on 127.0.0.1:{}: {e}", args.port);
            std::process::exit(1);
        }
    };
    println!("FPSFramer dev server: http://127.0.0.1:{}  (demo: {})", args.port, args.demo);
    for conn in listener.incoming().flatten() {
        let (e, ui, port) = (engine.clone(), args.ui_dir.clone(), args.port);
        std::thread::spawn(move || handle(conn, e, ui, port));
    }
}
