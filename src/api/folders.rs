//! Operaciones de carpetas: listado.

use serde::Deserialize;

use crate::api::client::O2CloudClient;
use crate::api::models::Folder;
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
}
