//! Modelos tipados de las entidades SAPI.

use serde::{Deserialize, Deserializer, Serialize};

/// Identificador de un recurso SAPI.
///
/// Los ids de carpeta llegan como números JSON y los de media como strings;
/// este tipo los normaliza a `String` para un manejo uniforme.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Id(String);

impl Id {
    /// El id como `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Id {
    fn from(v: &str) -> Self {
        Self(v.to_string())
    }
}

impl From<String> for Id {
    fn from(v: String) -> Self {
        Self(v)
    }
}

impl From<u64> for Id {
    fn from(v: u64) -> Self {
        Self(v.to_string())
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct IdVisitor;

        impl serde::de::Visitor<'_> for IdVisitor {
            type Value = Id;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an id as a string or number")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Id, E> {
                Ok(Id(v.to_string()))
            }

            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Id, E> {
                Ok(Id(v))
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Id, E> {
                Ok(Id(v.to_string()))
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Id, E> {
                Ok(Id(v.to_string()))
            }

            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Id, E> {
                Ok(Id(format!("{v:.0}")))
            }
        }

        deserializer.deserialize_any(IdVisitor)
    }
}

/// Información de cuota / almacenamiento.
///
/// `GET /sapi/media?action=get-storage-space&softdeleted=true` → `data`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Quota {
    /// Capacidad total en bytes.
    #[serde(default)]
    pub quota: i64,
    /// Bytes libres.
    #[serde(default)]
    pub free: i64,
    /// Bytes usados.
    #[serde(default)]
    pub used: i64,
    /// Bytes en papelera (soft-deleted).
    #[serde(default)]
    pub softdeleted: i64,
    /// `true` si no hay límite de almacenamiento.
    #[serde(default)]
    pub nolimit: bool,
    /// Desglose por usuario (planes multi-usuario).
    #[serde(default)]
    pub individual: Option<Individual>,
}

/// Desglose individual de uso dentro de `Quota`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Individual {
    #[serde(default)]
    pub used: i64,
    #[serde(default)]
    pub softdeleted: i64,
}

/// Carpeta / auto-carpeta Funambol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    /// Identificador de la carpeta.
    pub id: Id,
    /// Nombre de la carpeta (`/` para la raíz).
    pub name: String,
    #[serde(default)]
    pub status: Option<String>,
    /// Auto-carpeta del sistema (Fotos/Vídeos).
    #[serde(default)]
    pub magic: bool,
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub creationdate: Option<serde_json::Value>,
    /// Fecha (epoch-ms) según SAPI.
    #[serde(default)]
    pub date: Option<i64>,
    /// Carpeta padre (observada por la implementación Rust).
    #[serde(default, alias = "parent_id", alias = "parent")]
    pub parentid: Option<Id>,
}

/// Elemento de media (archivo/foto/vídeo/audio).
///
/// El listado sparse solo trae `id`, `date`, `mediatype`, `status`, `userid`;
/// los campos de detalle (`name`, `size`, `url`, …) llegan con
/// `POST /sapi/media/file?action=get`.
#[derive(Clone, Serialize, Deserialize)]
pub struct MediaItem {
    /// Identificador del elemento (string en el flujo web).
    pub id: Id,
    /// Fecha (epoch-ms).
    #[serde(default)]
    pub date: Option<i64>,
    /// `file` | `picture` | `video` | `audio`.
    #[serde(default)]
    pub mediatype: Option<String>,
    /// Estado (p. ej. `U` = subido/válido).
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub userid: Option<String>,
    // -- campos de detalle --
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default)]
    pub contenttype: Option<String>,
    /// Puede ser epoch-ms (entero) o cadena compacta, según el endpoint.
    #[serde(default)]
    pub creationdate: Option<serde_json::Value>,
    /// Puede ser epoch-ms (entero) o cadena compacta, según el endpoint.
    #[serde(default)]
    pub modificationdate: Option<serde_json::Value>,
    /// Epoch-ms de subida (entero), aunque podría venir como otro tipo.
    #[serde(default)]
    pub uploaded: Option<serde_json::Value>,
    #[serde(default)]
    pub softdeleted: Option<bool>,
    #[serde(default)]
    pub etag: Option<String>,
    /// Carpeta contenedora (entero en el detalle real; a veces `folderid`).
    #[serde(default, alias = "folderid")]
    pub folder: Option<Id>,
    /// URL firmada de descarga. **Nunca se serializa** (secreto).
    #[serde(default, skip_serializing)]
    pub url: Option<String>,
}

impl std::fmt::Debug for MediaItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `url` es una URL firmada: no exponerla en debug/logs.
        f.debug_struct("MediaItem")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("size", &self.size)
            .field("mediatype", &self.mediatype)
            .field("status", &self.status)
            .field("folder", &self.folder)
            .field("has_url", &self.url.is_some())
            .finish_non_exhaustive()
    }
}

/// Listado sparse de media: `data{media[], more}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaListing {
    #[serde(default)]
    pub media: Vec<MediaItem>,
    /// `true` si hay más páginas.
    #[serde(default)]
    pub more: bool,
}

/// Respuesta del endpoint de subida.
///
/// Observado: `success` llega como cadena ("Media uploaded successfully"),
/// aunque podría ser booleano; por eso es tolerante.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UploadResult {
    #[serde(default)]
    pub success: serde_json::Value,
    #[serde(default)]
    pub id: Option<Id>,
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
}

/// Entrada de estado de validación tras una subida.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationEntry {
    pub id: Id,
    #[serde(default)]
    pub status: Option<String>,
}

/// Entrada de la papelera (`GET /sapi/media/trash?action=get`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashEntry {
    /// Id del elemento.
    pub id: Id,
    /// Nombre.
    #[serde(default)]
    pub name: String,
    /// Fecha (epoch-ms).
    #[serde(default)]
    pub date: Option<i64>,
    /// Tipo: `media` o `folder`.
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
}

/// Perfil de cuenta (parcial; tolerante a campos desconocidos).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default, alias = "userid")]
    pub user_id: Option<serde_json::Value>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub firstname: Option<String>,
    #[serde(default)]
    pub lastname: Option<String>,
    #[serde(default)]
    pub msisdn: Option<String>,
    /// Campos no modelados, preservados.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_coerces_number_and_string() {
        let a: Id = serde_json::from_value(serde_json::json!(42)).unwrap();
        let b: Id = serde_json::from_value(serde_json::json!("42")).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.as_str(), "42");
    }

    #[test]
    fn media_item_deserializes_sparse_and_detail() {
        let sparse: MediaItem =
            serde_json::from_str(r#"{"id":"1","date":1751441234567,"mediatype":"file"}"#).unwrap();
        assert_eq!(sparse.id.as_str(), "1");
        assert!(sparse.name.is_none());

        let detail: MediaItem = serde_json::from_str(
            r#"{"id":"1","name":"a.txt","size":10,"url":"/sapi/download/file?action=get&k=x"}"#,
        )
        .unwrap();
        assert_eq!(detail.name.as_deref(), Some("a.txt"));
        assert_eq!(detail.size, Some(10));
        assert_eq!(
            detail.url.as_deref(),
            Some("/sapi/download/file?action=get&k=x")
        );
    }

    #[test]
    fn media_item_url_is_not_serialized() {
        let item: MediaItem =
            serde_json::from_str(r#"{"id":"1","url":"/sapi/download/file?action=get&k=secret"}"#)
                .unwrap();
        let json = serde_json::to_string(&item).unwrap();
        assert!(!json.contains("secret"));
        assert!(!json.contains("url"));
    }
}
