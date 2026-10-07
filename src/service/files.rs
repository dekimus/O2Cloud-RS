//! Casos de uso de mutación: mkdir, copy, move, delete/softdelete.

use crate::api::client::O2CloudClient;
use crate::api::models::Id;
use crate::error::{Error, Result};
use crate::service::{resolve_folder, resolve_folder_id, stat};

/// Carpeta creada.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CreatedFolder {
    /// Id de la nueva carpeta.
    pub id: Id,
    /// Nombre.
    pub name: String,
    /// Ruta remota solicitada.
    pub path: String,
}

/// Resultado de una copia server-side.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CopyOutcome {
    /// Origen.
    pub source: String,
    /// Carpeta destino.
    pub dest: String,
}

/// Resultado de un movimiento.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MoveOutcome {
    /// Origen.
    pub source: String,
    /// Carpeta destino.
    pub dest: String,
}

/// Elemento remoto eliminable (archivo o carpeta).
#[derive(Debug, Clone)]
pub enum Removable {
    /// Carpeta.
    Folder {
        /// Id.
        id: Id,
        /// Nombre.
        name: String,
    },
    /// Archivo/foto/vídeo/audio.
    File {
        /// Id.
        id: Id,
        /// Nombre.
        name: String,
        /// Tipo de media (para elegir el recurso de borrado).
        mediatype: String,
    },
}

impl Removable {
    /// Nombre del elemento.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Folder { name, .. } | Self::File { name, .. } => name,
        }
    }

    /// Id del elemento.
    #[must_use]
    pub fn id(&self) -> &Id {
        match self {
            Self::Folder { id, .. } | Self::File { id, .. } => id,
        }
    }

    /// Descripción legible para la confirmación.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Folder { name, .. } => format!("la carpeta '{name}'"),
            Self::File { name, .. } => format!("el archivo '{name}'"),
        }
    }
}

/// Crea una carpeta en la ruta remota dada.
pub async fn mkdir(client: &O2CloudClient, remote: &str) -> Result<CreatedFolder> {
    let trimmed = remote.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err(Error::Protocol("cannot create the root folder".to_string()));
    }
    let (parent_path, name) = match trimmed.rsplit_once('/') {
        Some((parent, name)) => (parent, name),
        None => ("", trimmed),
    };
    let parent_remote = if parent_path.is_empty() {
        "/"
    } else {
        parent_path
    };
    let parent_id = resolve_folder_id(client, parent_remote).await?;
    let id = client.folders().create(name, Some(&parent_id)).await?;
    Ok(CreatedFolder {
        id,
        name: name.to_string(),
        path: remote.to_string(),
    })
}

/// Copia server-side de `source` a la carpeta `dest` (sin subir/descargar).
pub async fn copy(client: &O2CloudClient, source: &str, dest: &str) -> Result<CopyOutcome> {
    let item = stat(client, source).await?;
    let dest_folder = resolve_folder_id(client, dest).await?;
    client.media().copy(&item.id, Some(&dest_folder)).await?;
    Ok(CopyOutcome {
        source: source.to_string(),
        dest: dest.to_string(),
    })
}

/// Mueve un elemento a la carpeta `dest`.
///
/// **PARTIAL**: usa `media/folder?action=add-item`, cuyo cuerpo está observado
/// pero no confirmado al 100% (ver `docs/protocol.md`).
pub async fn move_item(client: &O2CloudClient, source: &str, dest: &str) -> Result<MoveOutcome> {
    let item = stat(client, source).await?;
    let dest_folder = resolve_folder_id(client, dest).await?;
    client.folders().add_item(&dest_folder, &item.id).await?;
    Ok(MoveOutcome {
        source: source.to_string(),
        dest: dest.to_string(),
    })
}

/// Resuelve una referencia remota (id o ruta) a un elemento eliminable.
///
/// Se comprueba primero si es una carpeta (por id o ruta); si no, se trata
/// como un archivo de media.
pub async fn resolve_removable(client: &O2CloudClient, remote: &str) -> Result<Removable> {
    let folders = client.folders().list().await?;

    if remote.parse::<u64>().is_ok() {
        if let Some(folder) = folders.iter().find(|f| f.id.as_str() == remote) {
            return Ok(Removable::Folder {
                id: folder.id.clone(),
                name: folder.name.clone(),
            });
        }
    } else if let Some(folder) = resolve_folder(remote, &folders) {
        return Ok(Removable::Folder {
            id: folder.id.clone(),
            name: folder.name.clone(),
        });
    }

    let item = stat(client, remote).await?;
    Ok(Removable::File {
        id: item.id.clone(),
        name: item.name.clone().unwrap_or_else(|| item.id.to_string()),
        mediatype: item.mediatype.clone().unwrap_or_else(|| "file".to_string()),
    })
}

/// Elimina un elemento (a la papelera por defecto, permanente si `permanent`).
///
/// Para carpetas, el borrado recursivo depende del backend y no está
/// confirmado.
pub async fn remove(client: &O2CloudClient, target: &Removable, permanent: bool) -> Result<()> {
    match target {
        Removable::File { id, mediatype, .. } => {
            client.media().delete(id, permanent, mediatype).await
        }
        Removable::Folder { id, .. } => client.folders().delete(id, permanent).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_folder() -> Removable {
        Removable::Folder {
            id: Id::from("1"),
            name: "fotos".into(),
        }
    }

    fn sample_file() -> Removable {
        Removable::File {
            id: Id::from("2"),
            name: "a.txt".into(),
            mediatype: "file".into(),
        }
    }

    #[test]
    fn removable_labels() {
        assert_eq!(sample_folder().label(), "la carpeta 'fotos'");
        assert_eq!(sample_file().label(), "el archivo 'a.txt'");
        assert_eq!(sample_file().name(), "a.txt");
        assert_eq!(sample_file().id().as_str(), "2");
    }
}
