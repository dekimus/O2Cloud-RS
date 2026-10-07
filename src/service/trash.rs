//! Caso de uso: papelera (listar y vaciar).

use crate::api::client::O2CloudClient;
use crate::api::models::TrashEntry;
use crate::error::Result;

/// Lista los elementos de la papelera.
///
/// `GET /sapi/media/trash?action=get` → `data.entries[]{date,id,name,type}`.
pub async fn trash_list(client: &O2CloudClient) -> Result<Vec<TrashEntry>> {
    client.media().trash().await
}

/// Vacía la papelera borrando **permanentemente** cada entrada.
///
/// Se apoya en los endpoints de borrado permanente ya confirmados
/// (`media/file?action=delete`, `media/folder?action=delete`), sin inventar un
/// endpoint de vaciado masivo. Devuelve cuántos elementos se borraron.
pub async fn trash_empty(client: &O2CloudClient) -> Result<usize> {
    let entries = client.media().trash().await?;
    let mut removed = 0usize;
    for entry in &entries {
        if entry.kind.as_deref() == Some("folder") {
            client.folders().delete(&entry.id, true).await?;
        } else {
            client.media().delete(&entry.id, true, "file").await?;
        }
        removed += 1;
    }
    Ok(removed)
}
