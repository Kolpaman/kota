//! Ortak HTTP istemcisi ve hata sınıflandırması.
//!
//! Hata mesajlarına istek başlıkları asla girmez; yanıt gövdesinden yalnızca
//! kısa bir parça alınır (API hata gövdeleri token içermez).

use std::time::Duration;

use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::Value;

pub fn user_agent() -> String {
    format!("kota/{} (+https://github.com/ensignalp/kota)", crate::VERSION)
}

pub fn build_client() -> Client {
    Client::builder()
        .user_agent(user_agent())
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| Client::new())
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FetchError {
    #[error("network error: {0}")]
    Network(String),
    #[error("credentials rejected (HTTP {0})")]
    Unauthorized(u16),
    #[error("rate limited by the provider")]
    RateLimited { retry_after_secs: Option<u64> },
    #[error("HTTP {status}: {snippet}")]
    Http { status: u16, snippet: String },
    #[error("unexpected response: {0}")]
    Parse(String),
}

impl FetchError {
    /// [`crate::model::ProviderReport::code`] için makine okunur kod.
    pub fn code(&self) -> &'static str {
        match self {
            FetchError::Network(_) => "network",
            FetchError::Unauthorized(_) => "unauthorized",
            FetchError::RateLimited { .. } => "rate_limited",
            FetchError::Http { .. } => "http_error",
            FetchError::Parse(_) => "parse_error",
        }
    }
}

/// Gövdeden tek satırlık kısa parça.
pub fn snippet(body: &str) -> String {
    body.chars().take(160).collect::<String>().replace(['\n', '\r'], " ")
}

pub fn classify_status(status: StatusCode, headers: &HeaderMap, body: &str) -> FetchError {
    match status.as_u16() {
        401 => FetchError::Unauthorized(401),
        // JSON gövdeli 403 API'nin reddi; HTML gövdeli 403 ise çoğunlukla bir
        // WAF/Cloudflare sayfasıdır ve oturumla ilgisi yoktur.
        403 if body.trim_start().starts_with('{') => FetchError::Unauthorized(403),
        429 => FetchError::RateLimited {
            retry_after_secs: headers
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse().ok()),
        },
        s => FetchError::Http {
            status: s,
            snippet: snippet(body),
        },
    }
}

fn describe(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "timed out".to_string()
    } else if e.is_connect() {
        "could not connect".to_string()
    } else {
        e.to_string()
    }
}

/// İsteği gönderir; 2xx ise gövdeyi JSON olarak döndürür.
pub async fn send_json(req: RequestBuilder) -> Result<Value, FetchError> {
    let resp = req.send().await.map_err(|e| FetchError::Network(describe(&e)))?;
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.text().await.map_err(|e| FetchError::Network(describe(&e)))?;
    if !status.is_success() {
        return Err(classify_status(status, &headers, &body));
    }
    serde_json::from_str(&body).map_err(|e| FetchError::Parse(format!("{e} (body: {})", snippet(&body))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn classifies_statuses() {
        let mut h = HeaderMap::new();
        assert_eq!(
            classify_status(StatusCode::UNAUTHORIZED, &h, ""),
            FetchError::Unauthorized(401)
        );
        assert_eq!(
            classify_status(StatusCode::FORBIDDEN, &h, "{\"error\":{}}"),
            FetchError::Unauthorized(403)
        );
        assert!(matches!(
            classify_status(StatusCode::FORBIDDEN, &h, "<html>cf</html>"),
            FetchError::Http { status: 403, .. }
        ));
        h.insert(RETRY_AFTER, HeaderValue::from_static("42"));
        assert_eq!(
            classify_status(StatusCode::TOO_MANY_REQUESTS, &h, ""),
            FetchError::RateLimited {
                retry_after_secs: Some(42)
            }
        );
    }
}
