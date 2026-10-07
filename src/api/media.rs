//! Operaciones de media: listado y detalle.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::api::client::O2CloudClient;
use crate::api::models::{Id, MediaItem, MediaListing};
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
    /// `POST /sapi/media/<type>?action=get` con `{"data":{"files":[{"id":…}]}}`.
    pub async fn detail(&self, id: &Id, mediatype: &str) -> Result<MediaItem> {
        let resource = type_resource(mediatype);
        let path = format!("/media/{resource}");
        let body = serde_json::json!({ "data": { "files": [{ "id": id.as_str() }] } });
        let resp: DetailResponse = self
            .client
            .post_read(&path, &[("action", "get")], Some(&body))
            .await?;
        resp.files
            .into_iter()
            .next()
            .ok_or_else(|| Error::InvalidResponse(format!("detail for {id} returned no file")))
    }

    /// Obtiene el detalle de varios elementos del **mismo** `mediatype` en una
    /// sola petición (extiende el detalle de un id, confirmado, a N ids).
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
        Ok(resp.files)
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
}

/// Recurso SAPI por tipo de media.
fn type_resource(mediatype: &str) -> &str {
    match mediatype {
        "picture" | "video" | "audio" => mediatype,
        _ => "file",
    }
}
