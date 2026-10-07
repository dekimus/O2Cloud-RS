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

/// Respuesta de creación: `{"folder":{"id":…}, "success":"…"}`.
#[derive(Debug, Deserialize)]
struct FolderSaveResponse {
    #[serde(default)]
    folder: Option<FolderSaved>,
    #[serde(default)]
    id: Option<Id>,
}

#[derive(Debug, Deserialize)]
struct FolderSaved {
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
    /// El cuerpo incluye `parentid` cuando se indica (campo confirmado contra
    /// el servicio real; `parent` era ignorado). La respuesta observada es
    /// `{"folder":{"id":…},"success":"…"}`.
    /// Devuelve el id de la nueva carpeta.
    pub async fn create(&self, name: &str, parent_id: Option<&Id>) -> Result<Id> {
        let mut inner = serde_json::json!({ "name": name });
        if let Some(parent) = parent_id {
            inner["parentid"] = serde_json::json!(parent.as_str());
        }
        let body = serde_json::json!({ "data": inner });
        let resp: FolderSaveResponse = self
            .client
            .post_read("/media/folder", &[("action", "save")], Some(&body))
            .await?;
        resp.folder
            .map(|folder| folder.id)
            .or(resp.id)
            .ok_or_else(|| {
                crate::error::Error::InvalidResponse("folder save response has no id".to_string())
            })
    }

    /// Borra una carpeta.
    ///
    /// Confirmado contra el servicio real:
    /// - permanente: `POST /media/folder?action=delete` con
    ///   `{"data":{"folders":[id]}}` (devuelve `data: null`);
    /// - papelera: `POST /media/folder?action=softdelete&id=<id>` (query, sin
    ///   body) → "Folders have been softdeleted".
    pub async fn delete(&self, id: &Id, permanent: bool) -> Result<()> {
        if permanent {
            let body = serde_json::json!({ "data": { "folders": [id.as_str()] } });
            let _: serde_json::Value = self
                .client
                .post("/media/folder", &[("action", "delete")], Some(&body))
                .await?;
        } else {
            let _: serde_json::Value = self
                .client
                .post(
                    "/media/folder",
                    &[("action", "softdelete"), ("id", id.as_str())],
                    None,
                )
                .await?;
        }
        Ok(())
    }

    /// Mueve/adjunta un item a una carpeta.
    /// `POST /sapi/media/folder?action=add-item`.
    ///
    /// Cuerpo **confirmado** contra el servicio real:
    /// `{"data":{"folderid":<carpeta>,"items":[<item>]}}` (con `id` devuelve
    /// `COM-1011`). Tras la operación, el detalle del item expone `folder`.
    pub async fn add_item(&self, folder_id: &Id, item_id: &Id) -> Result<()> {
        let body = serde_json::json!({
            "data": { "folderid": folder_id.as_str(), "items": [item_id.as_str()] }
        });
        let _: serde_json::Value = self
            .client
            .post("/media/folder", &[("action", "add-item")], Some(&body))
            .await?;
        Ok(())
    }
}
