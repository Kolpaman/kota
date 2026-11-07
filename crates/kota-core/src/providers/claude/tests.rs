use std::fs;
use std::path::Path;

use chrono::{Duration, Utc};
use serde_json::{Value, json};

use super::*;
use crate::testutil::{FakeServer, ctx};

const FIXTURE: &str = include_str!("../../../tests/fixtures/claude_usage_2026-11-06.json");

fn write_creds(dir: &Path, access: &str, refresh: &str, expires_in: Duration) {
    let v = json!({
        "claudeAiOauth": {
            "accessToken": access,
            "refreshToken": refresh,
            "expiresAt": (Utc::now() + expires_in).timestamp_millis(),
            "scopes": ["user:inference", "user:profile"],
            "subscriptionType": "pro",
            "rateLimitTier": "default_claude_ai"
        },
        "mcpOAuth": {"srv": "keep"}
    });
    fs::write(dir.join(auth::CREDENTIALS_FILE), v.to_string()).unwrap();
}

fn read_creds(dir: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(dir.join(auth::CREDENTIALS_FILE)).unwrap()).unwrap()
}

/// `/usage` yalnızca `accepted` token ile 200 döner; `/token` verilen yanıtı döner.
fn server(accepted: &'static str, token_status: u16, token_body: &'static str) -> FakeServer {
    FakeServer::start(move |r| match r.path.as_str() {
        "/usage" if r.header("authorization") == Some(&format!("Bearer {accepted}")) => (200, FIXTURE.to_string()),
        "/usage" => (
            401,
            r#"{"type":"error","error":{"type":"authentication_error"}}"#.to_string(),
        ),
        "/token" => (token_status, token_body.to_string()),
        _ => (404, "{}".to_string()),
    })
}

fn provider(dir: &Path, srv: &FakeServer, refresh: bool) -> ClaudeProvider {
    ClaudeProvider::new(dir.to_path_buf(), refresh, false).with_endpoints(srv.url("/usage"), srv.url("/token"))
}

#[tokio::test]
async fn fetches_usage_with_valid_token() {
    let dir = tempfile::tempdir().unwrap();
    write_creds(dir.path(), "a1", "r1", Duration::hours(3));
    let srv = server("a1", 500, "{}");
    let r = provider(dir.path(), &srv, true).fetch(&ctx(Utc::now())).await;
    assert_eq!(r.status, Status::Ok, "{:?}", r.message);
    assert_eq!(r.plan.as_deref(), Some("Pro"));
    assert_eq!(r.windows.len(), 2);
    assert_eq!(r.source, Some(Source::Api));
    let req = &srv.requests()[0];
    assert_eq!(req.header("anthropic-beta"), Some(BETA_HEADER));
    assert_eq!(srv.count("/token"), 0);
}

#[tokio::test]
async fn refreshes_expired_token_and_writes_back() {
    let dir = tempfile::tempdir().unwrap();
    write_creds(dir.path(), "a1", "r1", Duration::minutes(-5));
    let srv = server(
        "a2",
        200,
        r#"{"access_token":"a2","refresh_token":"r2","expires_in":28800,"token_type":"Bearer"}"#,
    );
    let r = provider(dir.path(), &srv, true).fetch(&ctx(Utc::now())).await;
    assert_eq!(r.status, Status::Ok, "{:?}", r.message);

    let token_req = srv.requests().into_iter().find(|r| r.path == "/token").unwrap();
    assert_eq!(token_req.method, "POST");
    let body: Value = serde_json::from_str(&token_req.body).unwrap();
    assert_eq!(body["grant_type"], "refresh_token");
    assert_eq!(body["refresh_token"], "r1");
    assert_eq!(body["client_id"], auth::CLIENT_ID);
    assert_eq!(body["scope"], "user:inference user:profile");

    let saved = read_creds(dir.path());
    assert_eq!(saved["claudeAiOauth"]["accessToken"], "a2");
    assert_eq!(saved["claudeAiOauth"]["refreshToken"], "r2");
    assert_eq!(saved["claudeAiOauth"]["subscriptionType"], "pro");
    assert_eq!(saved["mcpOAuth"]["srv"], "keep");
    let exp = saved["claudeAiOauth"]["expiresAt"].as_i64().unwrap();
    assert!(exp > (Utc::now() + Duration::hours(7)).timestamp_millis());
    // Kilitler bırakılmış olmalı.
    assert!(!dir.path().join(auth::LOCK_DIR).exists());
    assert!(!auth::legacy_lock_path(dir.path()).exists());
}

#[tokio::test]
async fn adopts_token_already_refreshed_by_another_process() {
    let dir = tempfile::tempdir().unwrap();
    // Kota girişte a1 gördü, ama dosyada artık Claude Code'un yazdığı a2 var.
    write_creds(dir.path(), "a2", "r2", Duration::hours(8));
    let srv = server("a2", 500, "{}");
    let entry =
        auth::parse(&json!({"claudeAiOauth": {"accessToken": "a1", "refreshToken": "r1", "expiresAt": 1}})).unwrap();
    let http = reqwest::Client::new();
    let path = dir.path().join(auth::CREDENTIALS_FILE);
    let token_url = srv.url("/token");
    let out = auth::refresh(auth::RefreshRequest {
        http: &http,
        claude_dir: dir.path(),
        creds_path: &path,
        token_url: &token_url,
        entry: &entry,
        force: false,
    })
    .await;
    match out {
        RefreshOutcome::Adopted(c) => assert_eq!(c.access_token.expose(), "a2"),
        other => panic!("expected Adopted, got {other:?}"),
    }
    assert_eq!(srv.count("/token"), 0, "must not spend a refresh token");
}

#[tokio::test]
async fn waits_while_claude_code_holds_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    write_creds(dir.path(), "a1", "r1", Duration::minutes(-1));
    fs::create_dir(dir.path().join(auth::LOCK_DIR)).unwrap(); // Claude Code yeniliyor
    let srv = server("a2", 500, "{}");
    let d = dir.path().to_path_buf();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        write_creds(&d, "a2", "r2", Duration::hours(8));
        fs::remove_dir(d.join(auth::LOCK_DIR)).unwrap();
    });
    let r = provider(dir.path(), &srv, true).fetch(&ctx(Utc::now())).await;
    writer.join().unwrap();
    assert_eq!(r.status, Status::Ok, "{:?}", r.message);
    assert_eq!(srv.count("/token"), 0);
}

#[tokio::test]
async fn invalid_grant_requires_relogin_and_keeps_file() {
    let dir = tempfile::tempdir().unwrap();
    write_creds(dir.path(), "a1", "r1", Duration::minutes(-1));
    let before = fs::read_to_string(dir.path().join(auth::CREDENTIALS_FILE)).unwrap();
    let srv = server(
        "a2",
        400,
        r#"{"error":"invalid_grant","error_description":"Refresh token expired"}"#,
    );
    let r = provider(dir.path(), &srv, true).fetch(&ctx(Utc::now())).await;
    assert_eq!(r.status, Status::Expired);
    assert_eq!(r.code.as_deref(), Some("relogin_required"));
    assert_eq!(
        fs::read_to_string(dir.path().join(auth::CREDENTIALS_FILE)).unwrap(),
        before
    );
    assert!(!dir.path().join(auth::LOCK_DIR).exists());
}

#[tokio::test]
async fn refresh_disabled_reports_expired_without_network() {
    let dir = tempfile::tempdir().unwrap();
    write_creds(dir.path(), "a1", "r1", Duration::minutes(-1));
    let srv = server("a1", 500, "{}");
    let r = provider(dir.path(), &srv, false).fetch(&ctx(Utc::now())).await;
    assert_eq!(r.status, Status::Expired);
    assert_eq!(r.code.as_deref(), Some("token_expired"));
    assert!(srv.requests().is_empty());
}

#[tokio::test]
async fn rejected_token_is_refreshed_once() {
    let dir = tempfile::tempdir().unwrap();
    // Süresi dolmamış ama sunucu reddediyor (iptal edilmiş).
    write_creds(dir.path(), "a1", "r1", Duration::hours(2));
    let srv = server(
        "a2",
        200,
        r#"{"access_token":"a2","refresh_token":"r2","expires_in":3600}"#,
    );
    let r = provider(dir.path(), &srv, true).fetch(&ctx(Utc::now())).await;
    assert_eq!(r.status, Status::Ok, "{:?}", r.message);
    assert_eq!(srv.count("/token"), 1);
    assert_eq!(srv.count("/usage"), 2);
}

#[tokio::test]
async fn rate_limit_backs_off() {
    let dir = tempfile::tempdir().unwrap();
    write_creds(dir.path(), "a1", "r1", Duration::hours(2));
    let srv = FakeServer::start(|_| (429, r#"{"error":"rate_limited"}"#.to_string()));
    let p = provider(dir.path(), &srv, true);
    let now = Utc::now();
    let r1 = p.fetch(&ctx(now)).await;
    assert_eq!(r1.status, Status::Error);
    assert_eq!(r1.code.as_deref(), Some("rate_limited"));
    let r2 = p.fetch(&ctx(now + Duration::seconds(30))).await;
    assert_eq!(r2.code.as_deref(), Some("rate_limited"));
    assert_eq!(srv.count("/usage"), 1, "second fetch must not hit the API");
}

#[tokio::test]
async fn not_connected_without_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server("a1", 500, "{}");
    let r = provider(dir.path(), &srv, true).fetch(&ctx(Utc::now())).await;
    assert_eq!(r.status, Status::NotConnected);
    assert_eq!(
        r.login_command.as_deref(),
        Some(&["claude".to_string(), "auth".into(), "login".into()][..])
    );
    assert!(srv.requests().is_empty());
}
