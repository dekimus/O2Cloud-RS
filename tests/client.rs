//! Tests de integración del cliente SAPI con un backend mockeado (wiremock).

use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use o2cloud::config::AppConfig;
use o2cloud::{Error, O2CloudClient, Session};

const COOKIE: &str = "JSESSIONID=abc123; validationKey=0123456789abcdef0123456789abcdef";

fn session() -> Session {
    Session::from_cookie_header(COOKIE).unwrap()
}

fn client(base_url: &str) -> O2CloudClient {
    let config = AppConfig::with_base_url(base_url, Some("test".into()));
    O2CloudClient::new(&config, Some(session())).unwrap()
}

#[tokio::test]
async fn quota_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sapi/media"))
        .and(query_param("action", "get-storage-space"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "quota": 1_099_511_627_776_i64,
                "free": 1_099_511_627_776_i64,
                "used": 0,
                "softdeleted": 0,
                "nolimit": false
            },
            "responsetime": 1
        })))
        .mount(&server)
        .await;

    let quota = client(&server.uri()).quota().get().await.unwrap();
    assert_eq!(quota.quota, 1_099_511_627_776);
    assert_eq!(quota.used, 0);
    assert!(!quota.nolimit);
}

#[tokio::test]
async fn sapi_error_in_http_200_is_mapped() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sapi/media"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "error": { "code": "MED-1000", "message": "not found", "parameters": [], "cause": "" },
            "responsetime": 1
        })))
        .mount(&server)
        .await;

    let err = client(&server.uri()).quota().get().await.unwrap_err();
    assert!(matches!(err, Error::NotFound(_)), "got: {err:?}");
}

#[tokio::test]
async fn unmapped_sapi_error_is_typed_as_sapi() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "error": { "code": "XYZ-9999", "message": "something broke" },
            "responsetime": 1
        })))
        .mount(&server)
        .await;

    let err = client(&server.uri()).quota().get().await.unwrap_err();
    match err {
        Error::Sapi { code, message } => {
            assert_eq!(code, "XYZ-9999");
            assert_eq!(message, "something broke");
        }
        other => panic!("expected Sapi error, got {other:?}"),
    }
}

#[tokio::test]
async fn maps_401_and_403_to_unauthorized() {
    for status in [401_u16, 403_u16] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let err = client(&server.uri()).quota().get().await.unwrap_err();
        assert!(
            matches!(err, Error::Unauthorized(_)),
            "status {status} -> {err:?}"
        );
    }
}

#[tokio::test]
async fn maps_404_to_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let err = client(&server.uri()).quota().get().await.unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}

#[tokio::test]
async fn maps_429_to_rate_limited() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    let err = client(&server.uri()).quota().get().await.unwrap_err();
    assert!(matches!(err, Error::RateLimited(_)));
}

#[tokio::test]
async fn maps_500_to_server_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let err = client(&server.uri()).quota().get().await.unwrap_err();
    assert!(matches!(err, Error::Server(_)));
}

#[tokio::test]
async fn attaches_cookie_and_validationkey() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sapi/media"))
        .and(query_param("action", "get-storage-space"))
        .and(query_param(
            "validationkey",
            "0123456789abcdef0123456789abcdef",
        ))
        .and(header("cookie", COOKIE))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "quota": 1, "free": 1, "used": 0, "softdeleted": 0, "nolimit": false }
        })))
        .mount(&server)
        .await;

    // Si la cookie o el query param no se enviaran, wiremock devolvería 404
    // y la operación fallaría.
    client(&server.uri()).quota().get().await.unwrap();
}

#[tokio::test]
async fn folder_list() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sapi/media/folder"))
        .and(query_param("action", "get"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "folders": [
                { "id": 1, "name": "/", "status": "V", "magic": false },
                { "id": 2, "name": "fotos", "magic": true, "parentid": 1 }
            ] }
        })))
        .mount(&server)
        .await;

    let folders = client(&server.uri()).folders().list().await.unwrap();
    assert_eq!(folders.len(), 2);
    assert_eq!(folders[0].name, "/");
    assert_eq!(folders[1].parentid.as_ref().map(Id::as_str), Some("1"));
}

use o2cloud::Id;

#[tokio::test]
async fn media_list_sparse_and_detail() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/sapi/media"))
        .and(query_param("action", "get"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "media": [ { "id": "100", "date": 1, "mediatype": "file", "status": "U" } ], "more": false }
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/sapi/media/file"))
        .and(query_param("action", "get"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "files": [ { "id": "100", "name": "a.txt", "size": 10 } ] }
        })))
        .mount(&server)
        .await;

    let listing = client(&server.uri()).media().list(100, None).await.unwrap();
    assert_eq!(listing.media.len(), 1);
    assert_eq!(listing.media[0].id.as_str(), "100");

    let detail = client(&server.uri())
        .media()
        .detail(&Id::from("100"), "file")
        .await
        .unwrap();
    assert_eq!(detail.name.as_deref(), Some("a.txt"));
    assert_eq!(detail.size, Some(10));
}

#[tokio::test]
async fn invalid_json_is_invalid_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>not json</html>"))
        .mount(&server)
        .await;
    let err = client(&server.uri()).quota().get().await.unwrap_err();
    assert!(matches!(err, Error::InvalidResponse(_)));
}

#[tokio::test]
async fn whoami_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sapi/profile"))
        .and(query_param("action", "get"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "userid": "12345", "email": "user@example.com" }
        })))
        .mount(&server)
        .await;

    let profile = client(&server.uri()).auth().whoami().await.unwrap();
    assert_eq!(profile["email"], "user@example.com");
}
