//! A tiny HTTP/1.1 server for tests: one thread, one connection at a time,
//! `Connection: close`, answers from a closure. Enough to play a WebDAV
//! server with or without `Range` support, and to count what was sent.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

pub struct Request {
    pub method: String,
    /// As sent, still percent-encoded.
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_ascii_lowercase()).map(String::as_str)
    }
}

pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn new(status: u16, body: Vec<u8>) -> Self {
        Reply { status, headers: Vec::new(), body }
    }
    pub fn status(status: u16) -> Self {
        Reply::new(status, Vec::new())
    }
    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }
}

/// A file served the way a static server does: `Range: bytes=a-b` gets a 206
/// when `ranges`, the whole file otherwise.
pub fn serve_file(request: &Request, bytes: &[u8], ranges: bool) -> Reply {
    if request.method == "HEAD" {
        return Reply::status(200).header("Content-Length", bytes.len().to_string());
    }
    let range = request.header("range").filter(|_| ranges).and_then(|value| {
        let (from, to) = value.strip_prefix("bytes=")?.split_once('-')?;
        let len = bytes.len() as u64;
        let (from, to) = if from.is_empty() {
            let suffix: u64 = to.parse().ok()?;
            (len.saturating_sub(suffix), len - 1)
        } else {
            let from: u64 = from.parse().ok()?;
            let to: u64 = if to.is_empty() { len - 1 } else { to.parse::<u64>().ok()?.min(len - 1) };
            (from, to)
        };
        (from <= to && to < len).then_some((from, to))
    });
    match range {
        Some((from, to)) => Reply::new(206, bytes[from as usize..=to as usize].to_vec())
            .header("Content-Range", format!("bytes {from}-{to}/{}", bytes.len()))
            .header("Accept-Ranges", "bytes"),
        None => Reply::new(200, bytes.to_vec()),
    }
}

pub struct TestServer {
    port: u16,
    stop: Arc<AtomicBool>,
    /// Body bytes sent so far, for "did not download the whole file".
    pub sent: Arc<AtomicU64>,
    pub requests: Arc<AtomicU64>,
}

impl TestServer {
    pub fn start(handler: impl Fn(&Request) -> Reply + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let sent = Arc::new(AtomicU64::new(0));
        let requests = Arc::new(AtomicU64::new(0));
        let (stop_flag, sent_count, request_count) = (stop.clone(), sent.clone(), requests.clone());
        std::thread::spawn(move || {
            while !stop_flag.load(Ordering::Relaxed) {
                let Ok((stream, _)) = listener.accept() else {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                };
                stream.set_nonblocking(false).unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || line.is_empty() {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or_default().to_string();
                let path = parts.next().unwrap_or_default().to_string();
                let mut headers = HashMap::new();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = header.split_once(':') {
                        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
                    }
                }
                let length: usize = headers.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                request_count.fetch_add(1, Ordering::Relaxed);
                let reply = handler(&Request { method: method.clone(), path, headers, body });
                let mut stream = stream;
                let length = reply
                    .headers
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .map_or_else(|| reply.body.len().to_string(), |(_, value)| value.clone());
                let mut head = format!("HTTP/1.1 {} X\r\nConnection: close\r\nContent-Length: {length}\r\n", reply.status);
                for (name, value) in &reply.headers {
                    if !name.eq_ignore_ascii_case("content-length") {
                        head.push_str(&format!("{name}: {value}\r\n"));
                    }
                }
                head.push_str("\r\n");
                let _ = stream.write_all(head.as_bytes());
                if method != "HEAD" {
                    if stream.write_all(&reply.body).is_ok() {
                        sent_count.fetch_add(reply.body.len() as u64, Ordering::Relaxed);
                    }
                }
                let _ = stream.flush();
            }
        });
        TestServer { port, stop, sent, requests }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
