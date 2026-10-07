//! Casos de uso: orquestación sin detalles HTTP.

mod download;
mod files;
mod ls;
mod quota;
mod stat;
mod status;
mod upload;

pub use download::{download, DownloadOptions, DownloadOutcome};
pub use files::{
    copy, mkdir, move_item, remove, resolve_removable, CopyOutcome, CreatedFolder, MoveOutcome,
    Removable,
};
pub use ls::{list, Listing};
pub use quota::quota;
pub use stat::stat;
pub use status::{status, StatusInfo};
pub use upload::{upload, UploadOptions, UploadOutcome};

use crate::api::client::O2CloudClient;
use crate::api::models::{Folder, Id};
use crate::error::{Error, Result};
use serde_json::Value;

/// Resuelve una ruta remota (p. ej. `/fotos/2024`) a una carpeta, usando
/// `parentid` cuando el backend lo proporciona y con fallback por nombre.
pub(crate) fn resolve_folder<'a>(path: &str, folders: &'a [Folder]) -> Option<&'a Folder> {
    let trimmed = path.trim().trim_matches('/');
    if trimmed.is_empty() {
        return folders
            .iter()
            .find(|f| f.name == "/")
            .or_else(|| folders.first());
    }

    let parts: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();

    // Caminar desde la raíz siguiendo `parentid` cuando esté disponible.
    let mut result = folders.iter().find(|f| f.name == "/");
    let mut current_id: Option<&Id> = result.map(|f| &f.id);
    for part in &parts {
        let next = folders.iter().find(|f| {
            f.name == *part && (current_id.is_none() || f.parentid.as_ref() == current_id)
        });
        if let Some(folder) = next {
            result = Some(folder);
            current_id = Some(&folder.id);
        } else {
            result = None;
            break;
        }
    }

    // Fallback: coincidencia por nombre del último segmento.
    result.or_else(|| {
        let last = parts.last()?;
        folders.iter().find(|f| f.name == *last)
    })
}

/// Resuelve una ruta remota a un id de carpeta, consultando el listado.
pub async fn resolve_folder_id(client: &O2CloudClient, remote: &str) -> Result<Id> {
    let folders = client.folders().list().await?;
    resolve_folder(remote, &folders)
        .map(|folder| folder.id.clone())
        .ok_or_else(|| Error::NotFound(format!("remote folder '{remote}' not found")))
}

/// Extrae una etiqueta de identidad legible del perfil (tolerante a la forma).
pub(crate) fn extract_identity(data: &Value) -> Option<String> {
    for key in ["email", "username", "msisdn", "userid", "firstname"] {
        if let Some(value) = data.get(key) {
            if let Some(s) = value.as_str() {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            } else if let Some(n) = value.as_u64() {
                return Some(n.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: u64, name: &str, parent: Option<u64>) -> Folder {
        Folder {
            id: Id::from(id),
            name: name.to_string(),
            status: None,
            magic: false,
            offline: false,
            creationdate: None,
            date: None,
            parentid: parent.map(Id::from),
        }
    }

    #[test]
    fn resolves_root() {
        let folders = vec![
            folder(1, "/", None),
            folder(2, "fotos", Some(1)),
            folder(3, "2024", Some(2)),
        ];
        let root = resolve_folder("/", &folders).unwrap();
        assert_eq!(root.id.as_str(), "1");
        let deep = resolve_folder("/fotos/2024", &folders).unwrap();
        assert_eq!(deep.id.as_str(), "3");
    }

    #[test]
    fn resolves_by_name_fallback() {
        let folders = vec![folder(1, "/", None), folder(2, "fotos", None)];
        let found = resolve_folder("fotos", &folders).unwrap();
        assert_eq!(found.id.as_str(), "2");
    }
}
