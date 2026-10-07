//! Casos límite de errores: HTTP 400, cuota SAPI y sesión expirada.

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use o2cloud::config::AppConfig;
use o2cloud::service;
use o2cloud::{Error, O2CloudClient, Session};

const COOKIE: &str = "JSESSIONID=abc123; validationKey=0123456789abcdef0123456789abcdef";

fn client(base_url: &str) -> O2CloudClient {
    let config = AppConfig::with_base_urls(base_url, base_url, Some("test".into()));
    let session = Session::from_cookie_header(COOKIE).unwrap();
    O2CloudClient::new(&config, Some(session)).unwrap()
}

#[tokio::test]
async fn maps_400_with_sapi_error_envelope() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sapi/media"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": { "code": "COM-1008", "message": "bad date format" },
            "responsetime": 1
        })))
        .mount(&server)
        .await;

    let err = client(&server.uri()).quota().get().await.unwrap_err();
    match err {
        Error::Sapi { code, .. } => assert_eq!(code, "COM-1008"),
        other => panic!("expected Sapi error, got {other:?}"),
    }
}

#[tokio::test]
async fn maps_400_without_envelope_to_invalid_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(400).set_body_string("<html>bad request</html>"))
        .mount(&server)
        .await;

    let err = client(&server.uri()).quota().get().await.unwrap_err();
    assert!(matches!(err, Error::InvalidResponse(_)), "got {err:?}");
}

#[tokio::test]
async fn quota_sapi_error_maps_to_quota_exceeded() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sapi/media"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "error": { "code": "QUO-100", "message": "not enough space" },
            "responsetime": 1
        })))
        .mount(&server)
        .await;

    let err = client(&server.uri()).quota().get().await.unwrap_err();
    assert!(matches!(err, Error::QuotaExceeded(_)), "got {err:?}");
    assert_eq!(err.exit_code(), 7);
}

#[tokio::test]
async fn security_sapi_error_maps_to_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "error": { "code": "SEC-100", "message": "invalid session" },
            "responsetime": 1
        })))
        .mount(&server)
        .await;

    let err = client(&server.uri()).quota().get().await.unwrap_err();
    assert!(matches!(err, Error::Unauthorized(_)), "got {err:?}");
}

#[tokio::test]
async fn status_reports_expired_session() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sapi/profile"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let err = service::status(&client(&server.uri())).await.unwrap_err();
    assert!(matches!(err, Error::Unauthorized(_)), "got {err:?}");
    assert_eq!(err.exit_code(), 2);
}

#[tokio::test]
async fn status_without_session_is_unauthenticated() {
    let config = AppConfig::with_base_urls("http://127.0.0.1:1", "http://127.0.0.1:1", None);
    let client = O2CloudClient::new(&config, None).unwrap();
    let info = service::status(&client).await.unwrap();
    assert!(!info.authenticated);
}
