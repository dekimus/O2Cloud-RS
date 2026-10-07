//! Operaciones de media: listado y detalle.

use std::collections::{BTreeMap, HashSet};

use serde::Deserialize;

use crate::api::client::O2CloudClient;
use crate::api::models::{Id, MediaItem, MediaListing, ValidationEntry};
use crate::error::{Error, Result};

/// Operaciones sobre el recurso `media`.
#[derive(Debug)]
pub struct MediaApi<'a> {
    pub(crate) client: &'a O2CloudClient,
}

/// Respuesta del endpoint de detalle: `data.files[]`.
#[derive(Debug, Deserialize)]
struct DetailResponse {
    #[serde(default)]
    files: Vec<MediaItem>,
}

/// Respuesta de `get-validation-status`: `data.ids[]`.
#[derive(Debug, Deserialize)]
struct ValidationResponse {
    #[serde(default)]
    ids: Vec<ValidationEntry>,
}

impl MediaApi<'_> {
    /// Lista media (sparse). `GET /sapi/media?action=get&limit=N`.
    ///
    /// `folder_id` se reenvía como `?folder=` cuando se proporciona; el
    /// comportamiento de filtrado por carpeta del backend no está confirmado.
    pub async fn list(&self, limit: u32, folder_id: Option<&Id>) -> Result<MediaListing> {
        let limit_s = limit.to_string();
        let folder_s;
        let mut params: Vec<(&str, &str)> = vec![("action", "get"), ("limit", &limit_s)];
        if let Some(folder) = folder_id {
            folder_s = folder.as_str().to_string();
            params.push(("folder", &folder_s));
        }
        self.client.get("/media", &params).await
    }

    /// Obtiene el detalle completo de un único elemento.
    ///
    /// `POST /sapi/media/<type>?action=get`. **Nota**: el endpoint real
    /// devuelve el detalle de **todos** los elementos del tipo (ignora el
    /// filtro de `id`), así que aquí se filtra por id en cliente.
    pub async fn detail(&self, id: &Id, mediatype: &str) -> Result<MediaItem> {
        let items = self
            .detail_many(std::slice::from_ref(id), mediatype)
            .await?;
        items
            .into_iter()
            .find(|item| &item.id == id)
            .ok_or_else(|| Error::NotFound(format!("media '{id}' not found")))
    }

    /// Obtiene el detalle de varios elementos del **mismo** `mediatype`.
    ///
    /// Como el endpoint devuelve todos los del tipo, se filtran los ids
    /// pedidos en cliente.
    pub async fn detail_many(&self, ids: &[Id], mediatype: &str) -> Result<Vec<MediaItem>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let resource = type_resource(mediatype);
        let path = format!("/media/{resource}");
        let files: Vec<serde_json::Value> = ids
            .iter()
            .map(|id| serde_json::json!({ "id": id.as_str() }))
            .collect();
        let body = serde_json::json!({ "data": { "files": files } });
        let resp: DetailResponse = self
            .client
            .post_read(&path, &[("action", "get")], Some(&body))
            .await?;
        let wanted: HashSet<Id> = ids.iter().cloned().collect();
        Ok(resp
            .files
            .into_iter()
            .filter(|item| wanted.contains(&item.id))
            .collect())
    }

    /// Resuelve el detalle de una lista de elementos sparse, agrupando por
    /// `mediatype` para usar el recurso SAPI correcto.
    pub async fn details(&self, items: &[MediaItem]) -> Result<Vec<MediaItem>> {
        let mut grouped: BTreeMap<String, Vec<Id>> = BTreeMap::new();
        for item in items {
            let mediatype = item.mediatype.clone().unwrap_or_else(|| "file".to_string());
            grouped.entry(mediatype).or_default().push(item.id.clone());
        }
        let mut out = Vec::new();
        for (mediatype, ids) in grouped {
            out.extend(self.detail_many(&ids, &mediatype).await?);
        }
        Ok(out)
    }

    /// Consulta el estado de validación de ids recién subidos.
    ///
    /// `POST /sapi/media?action=get-validation-status {data:{ids:[{id}]}}`.
    pub async fn validation_status(&self, ids: &[Id]) -> Result<Vec<ValidationEntry>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let body = serde_json::json!({
            "data": { "ids": ids.iter().map(|id| serde_json::json!({ "id": id.as_str() })).collect::<Vec<_>>() }
        });
        let resp: ValidationResponse = self
            .client
            .post_read(
                "/media",
                &[("action", "get-validation-status")],
                Some(&body),
            )
            .await?;
        Ok(resp.ids)
    }

    /// Borra un elemento de media.
    ///
    /// Confirmado contra el servicio real:
    /// - permanente: `POST /media/<type>?action=delete` con
    ///   `{"data":{"files":[id]}}` → "Files removed successfully";
    /// - papelera: `POST /media?action=softdelete&id=<id>` (query, sin body).
    pub async fn delete(&self, id: &Id, permanent: bool, mediatype: &str) -> Result<()> {
        if permanent {
            let resource = type_resource(mediatype);
            let path = format!("/media/{resource}");
            let body = serde_json::json!({ "data": { "files": [id.as_str()] } });
            let _: serde_json::Value = self
                .client
                .post(&path, &[("action", "delete")], Some(&body))
                .await?;
        } else {
            let _: serde_json::Value = self
                .client
                .post(
                    "/media",
                    &[("action", "softdelete"), ("id", id.as_str())],
                    None,
                )
                .await?;
        }
        Ok(())
    }
}

/// Recurso SAPI por tipo de media.
fn type_resource(mediatype: &str) -> &str {
    match mediatype {
        "picture" | "video" | "audio" => mediatype,
        _ => "file",
    }
}
