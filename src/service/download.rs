//! Caso de uso: descarga de un archivo remoto con streaming y rename atómico.

use std::path::{Path, PathBuf};

use crate::api::client::O2CloudClient;
use crate::api::models::Id;
use crate::error::{Error, Result};
use crate::progress::ProgressReporter;
use crate::service::stat;

/// Opciones de descarga.
#[derive(Debug, Clone, Default)]
pub struct DownloadOptions {
    /// Mostrar barra de progreso (stderr).
    pub show_progress: bool,
    /// Sobrescribir el fichero local si ya existe.
    pub force: bool,
}

/// Resultado de una descarga.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadOutcome {
    /// Id remoto.
    pub id: Id,
    /// Nombre del archivo.
    pub name: String,
    /// Ruta local final.
    pub path: String,
    /// Bytes escritos.
    pub size: u64,
}

/// Descarga un archivo remoto a `to`.
///
/// Flujo: metadata → URL firmada → streaming a `<destino>.part` → `rename`
/// atómico. Si la descarga falla, no queda un archivo aparentemente completo.
pub async fn download(
    client: &O2CloudClient,
    remote: &str,
    to: &Path,
    multiple: bool,
    options: &DownloadOptions,
) -> Result<DownloadOutcome> {
    let item = stat(client, remote).await?;
    let signed_url = item
        .url
        .clone()
        .ok_or_else(|| Error::NotFound(format!("remote '{remote}' has no download URL")))?;
    let name = sanitize_name(item.name.as_deref().unwrap_or_default(), &item.id);
    let dest = resolve_dest(to, &name, multiple)?;

    if dest.exists() && !options.force {
        return Err(Error::Conflict(format!(
            "local file already exists: {} (use --force to overwrite)",
            dest.display()
        )));
    }

    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                Error::LocalIo(format!("cannot create directory {}: {e}", parent.display()))
            })?;
        }
    }

    let part = part_path(&dest);
    let _ = tokio::fs::remove_file(&part).await;

    let expected = item.size.and_then(|s| u64::try_from(s).ok());
    let progress = ProgressReporter::new(expected.unwrap_or(0), options.show_progress);
    progress.set_message(format!("Descargando {name}"));

    match client
        .download_signed_url(&signed_url, &part, &progress)
        .await
    {
        Ok(size) => {
            tokio::fs::rename(&part, &dest).await.map_err(|e| {
                Error::LocalIo(format!(
                    "cannot rename {} to {}: {e}",
                    part.display(),
                    dest.display()
                ))
            })?;
            progress.finish_with_message(format!("✓ {name}"));
            Ok(DownloadOutcome {
                id: item.id,
                name,
                path: dest.display().to_string(),
                size,
            })
        }
        Err(err) => {
            let _ = tokio::fs::remove_file(&part).await;
            progress.abandon_with_message(format!("✗ {name}"));
            Err(err)
        }
    }
}

/// Ruta del fichero parcial (`<destino>.part`).
fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().map(ToOwned::to_owned).unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

/// Determina el destino local: dentro de `to` si es un directorio, o `to`
/// directamente para una única descarga.
fn resolve_dest(to: &Path, name: &str, multiple: bool) -> Result<PathBuf> {
    if to.is_dir() {
        Ok(to.join(name))
    } else if multiple {
        Err(Error::Protocol(format!(
            "'--to {}' must be an existing directory when downloading multiple files",
            to.display()
        )))
    } else {
        Ok(to.to_path_buf())
    }
}

/// Sanea un nombre recibido del servidor para evitar path traversal.
fn sanitize_name(raw: &str, fallback_id: &Id) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let cleaned: String = base.chars().filter(|c| *c != '\0').collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        format!("{fallback_id}.bin")
    } else {
        cleaned.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_traversal_and_empty() {
        assert_eq!(sanitize_name("../../etc/passwd", &Id::from("1")), "passwd");
        assert_eq!(sanitize_name("a/b/c.txt", &Id::from("1")), "c.txt");
        assert_eq!(sanitize_name("..", &Id::from("1")), "1.bin");
        assert_eq!(sanitize_name("", &Id::from("7")), "7.bin");
    }

    #[test]
    fn part_path_appends_suffix() {
        assert_eq!(
            part_path(Path::new("/tmp/a.bin")),
            PathBuf::from("/tmp/a.bin.part")
        );
    }
}
