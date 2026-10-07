//! Tests de integración de la descarga (streaming + rename atómico).

use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use o2cloud::config::AppConfig;
use o2cloud::service::{self, DownloadOptions};
use o2cloud::{Error, O2CloudClient, Session};

const COOKIE: &str = "JSESSIONID=abc123; validationKey=0123456789abcdef0123456789abcdef";
const CONTENT: &[u8] = b"hello world";

fn client(base_url: &str) -> O2CloudClient {
    let config = AppConfig::with_base_urls(base_url, base_url, Some("test".into()));
    let session = Session::from_cookie_header(COOKIE).unwrap();
    O2CloudClient::new(&config, Some(session)).unwrap()
}

async fn mount_metadata(server: &MockServer) {
    // Listado sparse (para resolver el mediatype del id).
    Mock::given(method("GET"))
        .and(path("/sapi/media"))
        .and(query_param("action", "get"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "media": [ { "id": "100", "mediatype": "file" } ], "more": false }
        })))
        .mount(server)
        .await;

    // Detalle con la URL firmada relativa.
    Mock::given(method("POST"))
        .and(path("/sapi/media/file"))
        .and(query_param("action", "get"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "files": [ {
                "id": "100", "name": "a.txt", "size": CONTENT.len(),
                "url": "/sapi/download/file?action=get&k=xyz"
            } ] }
        })))
        .mount(server)
        .await;
}

fn options() -> DownloadOptions {
    DownloadOptions {
        show_progress: false,
        force: false,
    }
}

#[tokio::test]
async fn download_streams_and_renames_atomically() {
    let dir = tempfile::tempdir().unwrap();

    let server = MockServer::start().await;
    mount_metadata(&server).await;
    Mock::given(method("GET"))
        .and(path("/sapi/download/file"))
        .and(query_param("k", "xyz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(CONTENT))
        .mount(&server)
        .await;

    let outcome = service::download(&client(&server.uri()), "100", dir.path(), false, &options())
        .await
        .unwrap();

    assert_eq!(outcome.name, "a.txt");
    assert_eq!(outcome.size, CONTENT.len() as u64);
    let dest = dir.path().join("a.txt");
    assert_eq!(std::fs::read(&dest).unwrap(), CONTENT);
    // No debe quedar el fichero parcial.
    assert!(!dir.path().join("a.txt.part").exists());
}

#[tokio::test]
async fn download_refuses_existing_without_force() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("a.txt");
    std::fs::write(&existing, b"old").unwrap();

    let server = MockServer::start().await;
    mount_metadata(&server).await;

    let err = service::download(&client(&server.uri()), "100", dir.path(), false, &options())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Conflict(_)), "got {err:?}");
    assert_eq!(std::fs::read(&existing).unwrap(), b"old");
}

#[tokio::test]
async fn download_overwrites_with_force() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("a.txt");
    std::fs::write(&existing, b"old").unwrap();

    let server = MockServer::start().await;
    mount_metadata(&server).await;
    Mock::given(method("GET"))
        .and(path("/sapi/download/file"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(CONTENT))
        .mount(&server)
        .await;

    let opts = DownloadOptions {
        show_progress: false,
        force: true,
    };
    service::download(&client(&server.uri()), "100", dir.path(), false, &opts)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&existing).unwrap(), CONTENT);
}

#[tokio::test]
async fn download_server_error_leaves_no_file() {
    let dir = tempfile::tempdir().unwrap();

    let server = MockServer::start().await;
    mount_metadata(&server).await;
    Mock::given(method("GET"))
        .and(path("/sapi/download/file"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let err = service::download(&client(&server.uri()), "100", dir.path(), false, &options())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Server(_)), "got {err:?}");
    assert!(!dir.path().join("a.txt").exists());
    assert!(!dir.path().join("a.txt.part").exists());
}
