//! Caso de uso: subida de un archivo con confirmación de validación.

use std::path::Path;
use std::time::Duration;

use crate::api::client::O2CloudClient;
use crate::api::models::Id;
use crate::error::{Error, Result};
use crate::progress::ProgressReporter;

/// Opciones de subida.
#[derive(Debug, Clone)]
pub struct UploadOptions {
    /// Mostrar barra de progreso (stderr).
    pub show_progress: bool,
    /// Reportero de progreso alternativo (p. ej. la GUI); si se indica,
    /// tiene prioridad sobre `show_progress`.
    pub reporter: Option<ProgressReporter>,
    /// Número máximo de sondeos de validación.
    pub max_polls: u32,
    /// Retardo inicial del backoff de validación.
    pub initial_delay: Duration,
    /// Retardo máximo del backoff de validación.
    pub max_delay: Duration,
}

impl Default for UploadOptions {
    fn default() -> Self {
        Self {
            show_progress: false,
            reporter: None,
            max_polls: 10,
            initial_delay: Duration::from_millis(300),
            max_delay: Duration::from_millis(3000),
        }
    }
}

/// Resultado de una subida.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UploadOutcome {
    /// Id remoto asignado.
    pub id: Id,
    /// Nombre remoto.
    pub name: String,
    /// Tamaño en bytes.
    pub size: u64,
    /// Ruta local de origen.
    pub local_path: String,
    /// `etag` devuelto por el servidor.
    pub etag: Option<String>,
    /// Último estado de validación observado.
    pub validation_status: Option<String>,
    /// `true` si el estado confirmó que el objeto está disponible.
    pub validated: bool,
}

/// Sube un archivo local a la carpeta remota indicada y confirma la validación.
///
/// Tras la subida se sondea `get-validation-status` con backoff: la
/// consistencia es eventual y no se declara "disponible" hasta que el estado lo
/// confirma (`U`). Si el sondeo agota su tiempo, se devuelve `validated=false`
/// sin borrar los bytes ya almacenados.
pub async fn upload(
    client: &O2CloudClient,
    local: &Path,
    folder_id: &Id,
    options: &UploadOptions,
) -> Result<UploadOutcome> {
    let metadata = std::fs::metadata(local)
        .map_err(|e| Error::LocalIo(format!("cannot stat {}: {e}", local.display())))?;
    if !metadata.is_file() {
        return Err(Error::LocalIo(format!(
            "not a regular file: {}",
            local.display()
        )));
    }
    let size = metadata.len();
    let name = local
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::LocalIo(format!("invalid file name: {}", local.display())))?
        .to_string();

    let progress = options
        .reporter
        .clone()
        .unwrap_or_else(|| ProgressReporter::indicatif(options.show_progress));
    progress.set_length(size);
    progress.set_message(format!("Subiendo {name}"));

    let result = match client
        .upload()
        .save(local, &name, folder_id, &progress)
        .await
    {
        Ok(result) => {
            progress.finish_with_message(format!("✓ {name}"));
            result
        }
        Err(err) => {
            progress.abandon_with_message(format!("✗ {name}"));
            return Err(err);
        }
    };

    let id = result
        .id
        .ok_or_else(|| Error::InvalidResponse("upload response has no id".to_string()))?;
    let (validation_status, validated) = poll_validation(client, &id, options).await;

    Ok(UploadOutcome {
        id,
        name,
        size,
        local_path: local.display().to_string(),
        etag: result.etag,
        validation_status,
        validated,
    })
}

/// Sondea el estado de validación con backoff hasta confirmar o agotar intentos.
async fn poll_validation(
    client: &O2CloudClient,
    id: &Id,
    options: &UploadOptions,
) -> (Option<String>, bool) {
    let mut delay = options.initial_delay;
    let mut last_status = None;

    for _ in 0..options.max_polls {
        match client
            .media()
            .validation_status(std::slice::from_ref(id))
            .await
        {
            Ok(entries) => {
                if let Some(entry) = entries.into_iter().find(|entry| &entry.id == id) {
                    if entry.status.as_deref() == Some("U") {
                        return (entry.status, true);
                    }
                    last_status = entry.status;
                }
            }
            Err(err) => tracing::debug!(error = %err, "validation poll failed"),
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(options.max_delay);
    }

    (last_status, false)
}
