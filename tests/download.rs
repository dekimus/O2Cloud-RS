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
    mount_metadata_for(server, "a.txt", CONTENT.len() as u64, "xyz").await;
}

async fn mount_metadata_for(server: &MockServer, name: &str, size: u64, key: &str) {
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
                "id": "100", "name": name, "size": size,
                "url": format!("/sapi/download/file?action=get&k={key}")
            } ] }
        })))
        .mount(server)
        .await;
}

fn options() -> DownloadOptions {
    DownloadOptions {
        show_progress: false,
        force: false,
        ..Default::default()
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
        ..Default::default()
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

#[tokio::test]
async fn download_empty_file() {
    let dir = tempfile::tempdir().unwrap();

    let server = MockServer::start().await;
    mount_metadata_for(&server, "vacio.bin", 0, "empty").await;
    Mock::given(method("GET"))
        .and(path("/sapi/download/file"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(Vec::<u8>::new()))
        .mount(&server)
        .await;

    let outcome = service::download(&client(&server.uri()), "100", dir.path(), false, &options())
        .await
        .unwrap();
    assert_eq!(outcome.size, 0);
    assert_eq!(std::fs::read(dir.path().join("vacio.bin")).unwrap(), b"");
    assert!(!dir.path().join("vacio.bin.part").exists());
}

#[tokio::test]
async fn download_large_file_streams_integrally() {
    let dir = tempfile::tempdir().unwrap();
    let content = vec![0x42_u8; 3 * 1024 * 1024]; // 3 MiB

    let server = MockServer::start().await;
    mount_metadata_for(&server, "grande.bin", content.len() as u64, "big").await;
    Mock::given(method("GET"))
        .and(path("/sapi/download/file"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(content.clone()))
        .mount(&server)
        .await;

    let outcome = service::download(&client(&server.uri()), "100", dir.path(), false, &options())
        .await
        .unwrap();
    assert_eq!(outcome.size, content.len() as u64);
    assert_eq!(
        std::fs::read(dir.path().join("grande.bin")).unwrap(),
        content
    );
}
