//! Caso de uso: listado de carpetas y archivos.

use crate::api::client::O2CloudClient;
use crate::api::models::{Folder, Id, MediaItem};
use crate::error::{Error, Result};
use crate::service::resolve_folder;

/// Listado resultante de `ls`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Listing {
    /// Carpetas (subcarpetas del destino).
    pub folders: Vec<Folder>,
    /// Archivos del destino (por carpeta cuando el detalle la expone).
    pub files: Vec<MediaItem>,
}

/// Lista el contenido de una ruta remota (por nombre de ruta).
pub async fn list(client: &O2CloudClient, remote: &str) -> Result<Listing> {
    let folders = client.folders().list().await?;
    let target = resolve_folder(remote, &folders)
        .ok_or_else(|| Error::NotFound(format!("remote folder '{remote}' not found")))?;
    list_in(client, &folders, target).await
}

/// Lista el contenido de una carpeta por su id (evita ambigüedad de nombres).
pub async fn list_folder(client: &O2CloudClient, folder_id: &Id) -> Result<Listing> {
    let folders = client.folders().list().await?;
    let target = folders
        .iter()
        .find(|folder| &folder.id == folder_id)
        .ok_or_else(|| Error::NotFound(format!("folder '{folder_id}' not found")))?;
    list_in(client, &folders, target).await
}

async fn list_in(client: &O2CloudClient, folders: &[Folder], target: &Folder) -> Result<Listing> {
    let sparse = client.media().list(1000, None).await?;
    let details = client.media().details(&sparse.media).await?;
    let target_is_root = target.name == "/";
    // El detalle expone `folder` (None = raíz). Para la raíz incluimos los
    // archivos sin carpeta; para el resto, solo los de esa carpeta.
    let files: Vec<MediaItem> = details
        .into_iter()
        .filter(|item| match item.folder.as_ref() {
            Some(folder) => folder == &target.id,
            None => target_is_root,
        })
        .collect();

    let subfolders = filter_subfolders(folders, target);
    Ok(Listing {
        folders: subfolders,
        files,
    })
}

/// Devuelve las carpetas hijas de `parent`; si el backend no reporta
/// `parentid`, devuelve todas (best-effort). Las carpetas de primer nivel sin
/// `parentid` se consideran hijas de la raíz.
fn filter_subfolders(folders: &[Folder], parent: &Folder) -> Vec<Folder> {
    let has_parents = folders.iter().any(|f| f.parentid.is_some());
    if !has_parents {
        return folders.to_vec();
    }
    let root_id = folders.iter().find(|f| f.name == "/").map(|f| &f.id);
    let is_root = root_id == Some(&parent.id);
    folders
        .iter()
        .filter(|f| {
            f.id != parent.id
                && (f.parentid.as_ref() == Some(&parent.id) || (is_root && f.parentid.is_none()))
        })
        .cloned()
        .collect()
}
