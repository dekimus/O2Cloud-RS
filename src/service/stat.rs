//! Caso de uso: detalle de un archivo remoto (`stat`).

use crate::api::client::O2CloudClient;
use crate::api::models::{Id, MediaItem};
use crate::error::{Error, Result};

/// Obtiene el detalle de un archivo remoto, dado un id numérico o una
/// ruta/nombre.
pub async fn stat(client: &O2CloudClient, remote: &str) -> Result<MediaItem> {
    // Id numérico directo.
    if let Ok(id) = remote.parse::<u64>() {
        let listing = client.media().list(2000, None).await?;
        let mediatype = listing
            .media
            .iter()
            .find(|m| m.id.as_str() == id.to_string())
            .and_then(|m| m.mediatype.clone())
            .unwrap_or_else(|| "file".to_string());
        return client.media().detail(&Id::from(id), &mediatype).await;
    }

    // Por nombre (basename de la ruta).
    let name = remote.rsplit('/').next().unwrap_or(remote);
    let sparse = client.media().list(1000, None).await?;
    let details = client.media().details(&sparse.media).await?;
    details
        .into_iter()
        .find(|m| m.name.as_deref() == Some(name))
        .ok_or_else(|| Error::NotFound(format!("remote file '{remote}' not found")))
}
