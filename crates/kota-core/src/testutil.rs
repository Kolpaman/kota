//! Testler için küçük sahte HTTP sunucusu (bağlantı başına tek istek).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use chrono::{DateTime, Utc};

use crate::config::Thresholds;
use crate::providers::FetchContext;

#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_lowercase();
        self.headers.iter().find(|(k, _)| *k == name).map(|(_, v)| v.as_str())
    }
}

pub struct FakeServer {
    base: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl FakeServer {
    /// `responder` her istek için (durum kodu, JSON gövde) döndürür.
    pub fn start(responder: impl Fn(&Recorded) -> (u16, String) + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = requests.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let Ok(clone) = stream.try_clone() else { continue };
                let mut reader = BufReader::new(clone);
                let mut first = String::new();
                if reader.read_line(&mut first).unwrap_or(0) == 0 {
                    continue;
                }
                let mut parts = first.split_whitespace();
                let method = parts.next().unwrap_or_default().to_string();
                let path = parts.next().unwrap_or_default().to_string();
                let mut headers = Vec::new();
                let mut content_length = 0usize;
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 {
                        break;
                    }
                    let h = h.trim_end();
                    if h.is_empty() {
                        break;
                    }
                    if let Some((k, v)) = h.split_once(':') {
                        let (k, v) = (k.trim().to_lowercase(), v.trim().to_string());
                        if k == "content-length" {
                            content_length = v.parse().unwrap_or(0);
                        }
                        headers.push((k, v));
                    }
                }
                let mut body = vec![0; content_length];
                let _ = reader.read_exact(&mut body);
                let rec = Recorded {
                    method,
                    path,
                    headers,
                    body: String::from_utf8_lossy(&body).into_owned(),
                };
                let (status, resp) = responder(&rec);
                log.lock().unwrap().push(rec);
                let out = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{resp}",
                    resp.len()
                );
                let _ = stream.write_all(out.as_bytes());
                let _ = stream.flush();
            }
        });
        Self {
            base: format!("http://{addr}"),
            requests,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }

    pub fn count(&self, path: &str) -> usize {
        self.requests().iter().filter(|r| r.path == path).count()
    }
}

pub fn ctx(now: DateTime<Utc>) -> FetchContext {
    FetchContext {
        http: reqwest::Client::new(),
        thresholds: Thresholds::default(),
        now,
        today_start: now - chrono::Duration::hours(1),
    }
}
