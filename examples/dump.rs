//! Utilidad de depuración: imprime respuestas crudas del SAPI (redactadas).
//!
//! Uso: `cargo run --example dump` (requiere una sesión en el keyring).
//! Las URLs firmadas y otros valores sensibles se enmascaran.

use o2cloud::auth::{KeyringStore, SecretStore, Session};
use o2cloud::config::AppConfig;
use o2cloud::O2CloudClient;

/// Enmascara URLs/enlaces firmados en un JSON anidado.
fn redact(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if matches!(key.as_str(), "url" | "link" | "exportlink" | "getlink") {
                    *child = serde_json::Value::String("[REDACTED]".to_string());
                } else {
                    redact(child);
                }
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(redact),
        _ => {}
    }
}

fn print(title: &str, mut value: serde_json::Value) -> Result<(), serde_json::Error> {
    redact(&mut value);
    println!("=== {title} ===");
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::new(None);
    let store = KeyringStore::new(&config.keyring_service())?;
    let cookie = store.get_cookie()?.ok_or("no session in keyring")?;
    let session = Session::from_cookie_header(&cookie)?;
    let client = O2CloudClient::new(&config, Some(session))?;

    let folders: serde_json::Value = client
        .post_read(
            "/media/folder",
            &[("action", "get")],
            None::<&serde_json::Value>,
        )
        .await?;
    print("FOLDERS", folders)?;

    let media: serde_json::Value = client
        .get("/media", &[("action", "get"), ("limit", "5")])
        .await?;
    let first_id = media
        .get("media")
        .and_then(|m| m.as_array())
        .and_then(|a| a.first())
        .and_then(|item| item.get("id"))
        .and_then(|id| id.as_str())
        .map(str::to_string);
    print("MEDIA (sparse)", media)?;

    if let Some(id) = first_id {
        let detail: serde_json::Value = client
            .post_read(
                "/media/file",
                &[("action", "get")],
                Some(&serde_json::json!({ "data": { "files": [{ "id": id }] } })),
            )
            .await?;
        print(&format!("FILE DETAIL ({id})"), detail)?;
    }

    // Sonda opcional de `folder?action=save` (crea una carpeta): DUMP_MKDIR=<name>.
    if let Ok(name) = std::env::var("DUMP_MKDIR") {
        let parent = std::env::var("DUMP_MKDIR_PARENT").ok();
        let mut inner = serde_json::json!({ "name": name });
        if let Some(parent) = parent {
            inner["parentid"] = serde_json::json!(parent);
        }
        let resp: serde_json::Value = client
            .post_read(
                "/media/folder",
                &[("action", "save")],
                Some(&serde_json::json!({ "data": inner })),
            )
            .await?;
        print("SAVE RESPONSE", resp)?;
    }

    // Sonda opcional de GET arbitrario: DUMP_GET_PATH=/media?action=get&softdeleted=true.
    if let Ok(path) = std::env::var("DUMP_GET_PATH") {
        let resp: serde_json::Value = client.get(&path, &[]).await?;
        print("GET RESPONSE", resp)?;
    }

    // Sonda opcional de POST arbitrario para depurar el protocolo:
    // DUMP_POST_BODY=<json> [DUMP_POST_PATH=/media/folder] [DUMP_POST_ACTION=add-item].
    if let Ok(body) = std::env::var("DUMP_POST_BODY") {
        let action = std::env::var("DUMP_POST_ACTION").unwrap_or_else(|_| "add-item".to_string());
        let path = std::env::var("DUMP_POST_PATH").unwrap_or_else(|_| "/media/folder".to_string());
        let body_id = std::env::var("DUMP_POST_ID").ok();
        let value: serde_json::Value = serde_json::from_str(&body)?;
        let mut params: Vec<(&str, &str)> = vec![("action", action.as_str())];
        if let Some(id) = &body_id {
            params.push(("id", id.as_str()));
        }
        let resp: serde_json::Value = client.post(&path, &params, Some(&value)).await?;
        print("POST RESPONSE", resp)?;
    }

    Ok(())
}
