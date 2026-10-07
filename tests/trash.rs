//! Tests de integración de la papelera (listar y vaciar).

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

async fn mount_trash(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/sapi/media/trash"))
        .and(query_param("action", "get"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "entries": [
                { "id": 10, "name": "vieja", "type": "folder", "date": 1 },
                { "id": "20", "name": "viejo.d", "type": "media", "date": 2 }
            ] }
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn trash_list_parses_entries() {
    let server = MockServer::start().await;
    mount_trash(&server).await;

    let entries = service::trash_list(&client(&server.uri())).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id.as_str(), "10");
    assert_eq!(entries[0].name, "vieja");
    assert_eq!(entries[0].kind.as_deref(), Some("folder"));
    assert_eq!(entries[1].kind.as_deref(), Some("media"));
}

#[tokio::test]
async fn trash_empty_deletes_each_entry_permanently() {
    let server = MockServer::start().await;
    mount_trash(&server).await;
    Mock::given(method("POST"))
        .and(path("/sapi/media/folder"))
        .and(query_param("action", "delete"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/sapi/media/file"))
        .and(query_param("action", "delete"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .mount(&server)
        .await;

    let removed = service::trash_empty(&client(&server.uri())).await.unwrap();
    assert_eq!(removed, 2);

    let requests = server.received_requests().await.unwrap();
    let folder_deletes = requests
        .iter()
        .filter(|r| {
            r.url.path() == "/sapi/media/folder"
                && r.url.query().is_some_and(|q| q.contains("action=delete"))
        })
        .count();
    let media_deletes = requests
        .iter()
        .filter(|r| {
            r.url.path() == "/sapi/media/file"
                && r.url.query().is_some_and(|q| q.contains("action=delete"))
        })
        .count();
    assert_eq!(folder_deletes, 1);
    assert_eq!(media_deletes, 1);
}
