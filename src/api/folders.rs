//! Operaciones de carpetas: listado, creación y borrado.

use serde::Deserialize;

use crate::api::client::O2CloudClient;
use crate::api::models::{Folder, Id};
use crate::error::Result;

/// Operaciones sobre el recurso `media/folder`.
#[derive(Debug)]
pub struct FoldersApi<'a> {
    pub(crate) client: &'a O2CloudClient,
}

/// Respuesta de listado: `data.folders[]`.
#[derive(Debug, Deserialize)]
struct FoldersResponse {
    #[serde(default)]
    folders: Vec<Folder>,
}

/// Respuesta de creación: el objeto de carpeta (o al menos su `id`).
#[derive(Debug, Deserialize)]
struct FolderCreated {
    id: Id,
}

impl FoldersApi<'_> {
    /// Lista las carpetas de la cuenta. `POST /sapi/media/folder?action=get`.
    pub async fn list(&self) -> Result<Vec<Folder>> {
        let resp: FoldersResponse = self
            .client
            .post_read(
                "/media/folder",
                &[("action", "get")],
                None::<&serde_json::Value>,
            )
            .await?;
        Ok(resp.folders)
    }

    /// Crea una carpeta. `POST /sapi/media/folder?action=save`.
    ///
    /// El cuerpo incluye `parent` cuando se indica (forma observada en el flujo
    /// web). Devuelve el id de la nueva carpeta.
    pub async fn create(&self, name: &str, parent_id: Option<&Id>) -> Result<Id> {
        let mut inner = serde_json::json!({ "name": name });
        if let Some(parent) = parent_id {
            inner["parent"] = serde_json::json!(parent.as_str());
        }
        let body = serde_json::json!({ "data": inner });
        let resp: FolderCreated = self
            .client
            .post_read("/media/folder", &[("action", "save")], Some(&body))
            .await?;
        Ok(resp.id)
    }

    /// Borra una carpeta. `POST /sapi/media/folder?action={delete,softdelete}`.
    ///
    /// `permanent=false` usa la papelera. El comportamiento recursivo sobre
    /// carpetas no vacías no está confirmado y depende del backend.
    pub async fn delete(&self, id: &Id, permanent: bool) -> Result<()> {
        let action = if permanent { "delete" } else { "softdelete" };
        let body = serde_json::json!({ "data": { "folders": [id.as_str()] } });
        let _: serde_json::Value = self
            .client
            .post("/media/folder", &[("action", action)], Some(&body))
            .await?;
        Ok(())
    }

    /// Adjunta un item a una carpeta. `POST /sapi/media/folder?action=add-item`.
    ///
    /// El cuerpo está observado pero **no confirmado** (ver `docs/protocol.md`).
    pub async fn add_item(&self, folder_id: &Id, item_id: &Id) -> Result<()> {
        let body = serde_json::json!({
            "data": { "id": folder_id.as_str(), "items": [item_id.as_str()] }
        });
        let _: serde_json::Value = self
            .client
            .post("/media/folder", &[("action", "add-item")], Some(&body))
            .await?;
        Ok(())
    }
}
