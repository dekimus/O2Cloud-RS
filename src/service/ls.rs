//! Caso de uso: listado de carpetas y archivos.

use crate::api::client::O2CloudClient;
use crate::api::models::{Folder, MediaItem};
use crate::error::{Error, Result};
use crate::service::resolve_folder;

/// Listado resultante de `ls`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Listing {
    /// Carpetas (subcarpetas del destino cuando se puede resolver).
    pub folders: Vec<Folder>,
    /// Archivos (aplanados; el flujo web no expone carpeta por archivo).
    pub files: Vec<MediaItem>,
}

/// Lista el contenido de una ruta remota.
///
/// Las carpetas se filtran a las hijas del destino (usando `parentid` cuando
/// el backend lo devuelve); los archivos se listan a nivel de cuenta porque el
/// listado web no asocia archivos a carpetas de forma fiable.
pub async fn list(client: &O2CloudClient, remote: &str) -> Result<Listing> {
    let folders = client.folders().list().await?;
    let sparse = client.media().list(1000, None).await?;
    let files = client.media().details(&sparse.media).await?;

    let folders = match resolve_folder(remote, &folders) {
        Some(parent) => filter_subfolders(&folders, parent),
        None => {
            return Err(Error::NotFound(format!(
                "remote folder '{remote}' not found"
            )));
        }
    };

    Ok(Listing { folders, files })
}

/// Devuelve las carpetas hijas de `parent`; si el backend no reporta
/// `parentid`, devuelve todas (best-effort).
fn filter_subfolders(folders: &[Folder], parent: &Folder) -> Vec<Folder> {
    let has_parents = folders.iter().any(|f| f.parentid.is_some());
    if !has_parents {
        return folders.to_vec();
    }
    folders
        .iter()
        .filter(|f| f.parentid.as_ref() == Some(&parent.id))
        .cloned()
        .collect()
}
