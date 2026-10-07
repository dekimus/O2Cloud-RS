//! Test de integración del árbol de carpetas (`tree`).

use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use o2cloud::config::AppConfig;
use o2cloud::service;
use o2cloud::{O2CloudClient, Session};

const COOKIE: &str = "JSESSIONID=abc123; validationKey=0123456789abcdef0123456789abcdef";

fn client(base_url: &str) -> O2CloudClient {
    let config = AppConfig::with_base_urls(base_url, base_url, Some("test".into()));
    let session = Session::from_cookie_header(COOKIE).unwrap();
    O2CloudClient::new(&config, Some(session)).unwrap()
}

async fn mount_folders(server: &MockServer, folders: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/sapi/media/folder"))
        .and(query_param("action", "get"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "data": { "folders": folders } })),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn tree_builds_hierarchy() {
    let server = MockServer::start().await;
    mount_folders(
        &server,
        json!([
            { "id": 1, "name": "/" },
            { "id": 2, "name": "fotos", "parentid": 1 },
            { "id": 3, "name": "2024", "parentid": 2 },
            { "id": 4, "name": "documentos", "parentid": 1 },
        ]),
    )
    .await;

    let tree = service::tree(&client(&server.uri()), "/").await.unwrap();
    assert!(tree.hierarchical);
    assert_eq!(tree.folder_count, 4);
    assert_eq!(tree.root.name, "/");
    // Orden alfabético: documentos, fotos.
    assert_eq!(tree.root.children.len(), 2);
    assert_eq!(tree.root.children[0].name, "documentos");
    assert_eq!(tree.root.children[1].name, "fotos");
    assert_eq!(tree.root.children[1].children[0].name, "2024");
}

#[tokio::test]
async fn tree_flat_fallback_without_parentid() {
    let server = MockServer::start().await;
    mount_folders(
        &server,
        json!([
            { "id": 1, "name": "/" },
            { "id": 2, "name": "fotos" },
            { "id": 3, "name": "documentos" },
        ]),
    )
    .await;

    let tree = service::tree(&client(&server.uri()), "/").await.unwrap();
    assert!(!tree.hierarchical);
    assert_eq!(tree.folder_count, 3);
    assert_eq!(tree.root.children.len(), 2);
}

#[tokio::test]
async fn tree_unknown_root_errors() {
    let server = MockServer::start().await;
    mount_folders(&server, json!([{ "id": 1, "name": "/" }])).await;

    let err = service::tree(&client(&server.uri()), "/noexiste")
        .await
        .unwrap_err();
    assert!(matches!(err, o2cloud::Error::NotFound(_)), "got {err:?}");
}
