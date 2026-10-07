//! Tests de integración de las operaciones de archivos (mkdir/cp/mv/rm).

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

async fn mount_folder_list(server: &MockServer, folders: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/sapi/media/folder"))
        .and(query_param("action", "get"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "data": { "folders": folders } })),
        )
        .mount(server)
        .await;
}

async fn mount_media_resolution(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/sapi/media"))
        .and(query_param("action", "get"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "media": [ { "id": "100", "mediatype": "file" } ], "more": false }
        })))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/sapi/media/file"))
        .and(query_param("action", "get"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "files": [ { "id": "100", "name": "a.txt", "mediatype": "file" } ] }
        })))
        .mount(server)
        .await;
}

fn bodies(requests: &[wiremock::Request], target_path: &str, action: &str) -> Vec<String> {
    requests
        .iter()
        .filter(|r| r.url.path() == target_path)
        .filter(|r| {
            r.url
                .query_pairs()
                .any(|(k, v)| k == "action" && v == action)
        })
        .map(|r| String::from_utf8_lossy(&r.body).into_owned())
        .collect()
}

#[tokio::test]
async fn mkdir_sends_save_with_parent() {
    let server = MockServer::start().await;
    mount_folder_list(&server, json!([{ "id": 1, "name": "/" }])).await;
    Mock::given(method("POST"))
        .and(path("/sapi/media/folder"))
        .and(query_param("action", "save"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "id": 5, "name": "nueva" }
        })))
        .mount(&server)
        .await;

    let created = service::mkdir(&client(&server.uri()), "/nueva")
        .await
        .unwrap();
    assert_eq!(created.id.as_str(), "5");
    assert_eq!(created.name, "nueva");

    let requests = server.received_requests().await.unwrap();
    let save = bodies(&requests, "/sapi/media/folder", "save");
    assert_eq!(save.len(), 1);
    assert!(save[0].contains("\"name\":\"nueva\""), "{}", save[0]);
    assert!(save[0].contains("\"parent\":\"1\""), "{}", save[0]);
}

#[tokio::test]
async fn remove_file_softdelete_by_id() {
    let server = MockServer::start().await;
    mount_folder_list(&server, json!([{ "id": 1, "name": "/" }])).await;
    mount_media_resolution(&server).await;
    Mock::given(method("POST"))
        .and(path("/sapi/media/file"))
        .and(query_param("action", "softdelete"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .mount(&server)
        .await;

    let target = service::resolve_removable(&client(&server.uri()), "100")
        .await
        .unwrap();
    assert!(matches!(target, service::Removable::File { .. }));
    service::remove(&client(&server.uri()), &target, false)
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let del = bodies(&requests, "/sapi/media/file", "softdelete");
    assert_eq!(del.len(), 1);
    assert!(del[0].contains("\"files\":[\"100\"]"), "{}", del[0]);
}

#[tokio::test]
async fn remove_file_permanent_uses_delete() {
    let server = MockServer::start().await;
    mount_folder_list(&server, json!([{ "id": 1, "name": "/" }])).await;
    mount_media_resolution(&server).await;
    Mock::given(method("POST"))
        .and(path("/sapi/media/file"))
        .and(query_param("action", "delete"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .mount(&server)
        .await;

    let target = service::resolve_removable(&client(&server.uri()), "100")
        .await
        .unwrap();
    service::remove(&client(&server.uri()), &target, true)
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(bodies(&requests, "/sapi/media/file", "delete").len(), 1);
}

#[tokio::test]
async fn remove_folder_uses_folder_endpoint() {
    let server = MockServer::start().await;
    mount_folder_list(&server, json!([{ "id": 100, "name": "fotos" }])).await;
    Mock::given(method("POST"))
        .and(path("/sapi/media/folder"))
        .and(query_param("action", "softdelete"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .mount(&server)
        .await;

    let target = service::resolve_removable(&client(&server.uri()), "100")
        .await
        .unwrap();
    assert!(matches!(target, service::Removable::Folder { .. }));
    assert_eq!(target.label(), "la carpeta 'fotos'");
    service::remove(&client(&server.uri()), &target, false)
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let del = bodies(&requests, "/sapi/media/folder", "softdelete");
    assert_eq!(del.len(), 1);
    assert!(del[0].contains("\"folders\":[\"100\"]"), "{}", del[0]);
}

#[tokio::test]
async fn copy_sends_server_side_copy_with_folder() {
    let server = MockServer::start().await;
    mount_folder_list(
        &server,
        json!([{ "id": 1, "name": "/" }, { "id": 2, "name": "backup" }]),
    )
    .await;
    mount_media_resolution(&server).await;
    Mock::given(method("POST"))
        .and(path("/sapi/media/file"))
        .and(query_param("action", "copy"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .mount(&server)
        .await;

    service::copy(&client(&server.uri()), "100", "/backup")
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let copy = bodies(&requests, "/sapi/media/file", "copy");
    assert_eq!(copy.len(), 1);
    assert!(copy[0].contains("\"files\":[\"100\"]"), "{}", copy[0]);
    assert!(copy[0].contains("\"folder\":\"2\""), "{}", copy[0]);
}

#[tokio::test]
async fn mv_sends_add_item() {
    let server = MockServer::start().await;
    mount_folder_list(
        &server,
        json!([{ "id": 1, "name": "/" }, { "id": 2, "name": "backup" }]),
    )
    .await;
    mount_media_resolution(&server).await;
    Mock::given(method("POST"))
        .and(path("/sapi/media/folder"))
        .and(query_param("action", "add-item"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .mount(&server)
        .await;

    service::move_item(&client(&server.uri()), "100", "/backup")
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let add = bodies(&requests, "/sapi/media/folder", "add-item");
    assert_eq!(add.len(), 1);
    assert!(add[0].contains("\"id\":\"2\""), "{}", add[0]);
    assert!(add[0].contains("\"items\":[\"100\"]"), "{}", add[0]);
}
