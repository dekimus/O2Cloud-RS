//! Operaciones de subida al host dedicado (`upload.cloud.o2online.es`).

use std::path::Path;

use futures_util::StreamExt;
use reqwest::multipart::{Form, Part};
use tokio_util::io::ReaderStream;

use crate::api::client::O2CloudClient;
use crate::api::models::{Id, UploadResult};
use crate::error::{Error, Result};
use crate::progress::ProgressReporter;
use crate::util::compact_utc_now;

/// Operaciones de subida.
///
/// Endpoint confirmado: `POST {upload_host}/sapi/upload?action=save` con un
/// formulario multipart de dos partes (`data` con JSON y `file` con bytes),
/// cabecera `X-deviceid` y fechas en `YYYYMMDDThhmmssZ`.
#[derive(Debug)]
pub struct UploadApi<'a> {
    pub(crate) client: &'a O2CloudClient,
}

impl UploadApi<'_> {
    /// Sube un archivo local de forma streaming y devuelve la respuesta del
    /// servidor (`{success, id, etag, status, type}`).
    ///
    /// El archivo **no** se carga en memoria: se transmite desde disco.
    pub async fn save(
        &self,
        local: &Path,
        name: &str,
        folder_id: &Id,
        progress: &ProgressReporter,
    ) -> Result<UploadResult> {
        let metadata = std::fs::metadata(local)
            .map_err(|e| Error::LocalIo(format!("cannot stat {}: {e}", local.display())))?;
        if !metadata.is_file() {
            return Err(Error::LocalIo(format!(
                "not a regular file: {}",
                local.display()
            )));
        }
        let size = metadata.len();
        let timestamp = compact_utc_now();

        // El campo `data` contiene JSON con un envoltorio `data` y la carpeta
        // destino en la clave `folderid` (el campo `folder` del cliente web se
        // ignora y el archivo cae en la raíz; confirmado live 2026-10-07).
        let metadata_json = serde_json::json!({
            "data": {
                "name": name,
                "size": size,
                "modificationdate": timestamp,
                "creationdate": timestamp,
                "folderid": folder_id.as_str(),
            }
        });

        let file = tokio::fs::File::open(local)
            .await
            .map_err(|e| Error::LocalIo(format!("cannot open {}: {e}", local.display())))?;

        let progress_stream = progress.clone();
        let stream = ReaderStream::new(file).inspect(move |chunk| {
            if let Ok(bytes) = chunk {
                progress_stream.inc(bytes.len() as u64);
            }
        });
        let file_part = Part::stream_with_length(reqwest::Body::wrap_stream(stream), size)
            .file_name(name.to_string())
            .mime_str("application/octet-stream")
            .map_err(|e| Error::Protocol(format!("invalid mime type: {e}")))?;

        let form = Form::new()
            .part("data", Part::text(metadata_json.to_string()))
            .part("file", file_part);

        let headers = [
            ("X-deviceid", self.client.device_id().to_string()),
            ("x-funambol-file-size", size.to_string()),
        ];

        self.client
            .post_multipart("/upload", &[("action", "save")], form, &headers)
            .await
    }
}
