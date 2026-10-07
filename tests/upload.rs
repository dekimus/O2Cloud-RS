//! Tests de integración de la subida (multipart + validación) con wiremock.

use std::time::Duration;

use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use o2cloud::config::AppConfig;
use o2cloud::service::{self, UploadOptions};
use o2cloud::{Id, O2CloudClient, Session};

const COOKIE: &str = "JSESSIONID=abc123; validationKey=0123456789abcdef0123456789abcdef";

fn client(base_url: &str) -> O2CloudClient {
    let config = AppConfig::with_base_urls(base_url, base_url, Some("test".into()));
    let session = Session::from_cookie_header(COOKIE).unwrap();
    O2CloudClient::new(&config, Some(session)).unwrap()
}

fn fast_options() -> UploadOptions {
    UploadOptions {
        show_progress: false,
        max_polls: 3,
        initial_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(1),
    }
}

#[tokio::test]
async fn upload_sends_multipart_metadata_and_file() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("a.txt");
    std::fs::write(&file_path, b"hello world").unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sapi/upload"))
        .and(query_param("action", "save"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "success": true, "id": "42", "etag": "etag-1", "status": "U", "type": "file" }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/sapi/media"))
        .and(query_param("action", "get-validation-status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "ids": [ { "id": "42", "status": "U" } ] }
        })))
        .mount(&server)
        .await;

    let outcome = service::upload(
        &client(&server.uri()),
        &file_path,
        &Id::from("1"),
        &fast_options(),
    )
    .await
    .unwrap();

    assert_eq!(outcome.id.as_str(), "42");
    assert_eq!(outcome.name, "a.txt");
    assert_eq!(outcome.size, 11);
    assert_eq!(outcome.etag.as_deref(), Some("etag-1"));
    assert_eq!(outcome.validation_status.as_deref(), Some("U"));
    assert!(outcome.validated);

    // Inspeccionar la petición multipart realmente enviada.
    let requests = server.received_requests().await.unwrap();
    let upload = requests
        .iter()
        .find(|r| r.url.path() == "/sapi/upload")
        .expect("upload request");
    assert_eq!(upload.headers.get("x-deviceid").unwrap(), "o2cloud-cli");
    let content_type = upload
        .headers
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.starts_with("multipart/form-data"),
        "{content_type}"
    );

    let body = String::from_utf8_lossy(&upload.body);
    assert!(body.contains("\"name\":\"a.txt\""), "{body}");
    assert!(body.contains("\"folder\":\"1\""), "{body}");
    assert!(body.contains("\"creationdate\":\""), "{body}");
    assert!(body.contains("hello world"), "{body}");
}

#[tokio::test]
async fn upload_without_validation_is_pending() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("b.bin");
    std::fs::write(&file_path, vec![0u8; 2048]).unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sapi/upload"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true, "id": "7", "etag": "e7", "status": "U", "type": "file"
        })))
        .mount(&server)
        .await;
    // La validación nunca confirma "U".
    Mock::given(method("POST"))
        .and(path("/sapi/media"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "ids": [ { "id": "7", "status": "P" } ] }
        })))
        .mount(&server)
        .await;

    let outcome = service::upload(
        &client(&server.uri()),
        &file_path,
        &Id::from("1"),
        &fast_options(),
    )
    .await
    .unwrap();

    assert_eq!(outcome.id.as_str(), "7");
    assert!(!outcome.validated);
    assert_eq!(outcome.validation_status.as_deref(), Some("P"));
}

#[tokio::test]
async fn upload_positional_id_in_response() {
    // El host de subida puede devolver el objeto sin envelope `data`.
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("c.txt");
    std::fs::write(&file_path, b"x").unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sapi/upload"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true, "id": "99", "etag": "e99", "status": "U", "type": "file"
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/sapi/media"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "ids": [ { "id": "99", "status": "U" } ] }
        })))
        .mount(&server)
        .await;

    let outcome = service::upload(
        &client(&server.uri()),
        &file_path,
        &Id::from("1"),
        &fast_options(),
    )
    .await
    .unwrap();
    assert_eq!(outcome.id.as_str(), "99");
    assert!(outcome.validated);
}
