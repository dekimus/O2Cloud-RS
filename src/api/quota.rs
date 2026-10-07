//! Operaciones de cuota.

use crate::api::client::O2CloudClient;
use crate::api::models::Quota;
use crate::error::Result;

/// Operaciones de cuota / almacenamiento.
#[derive(Debug)]
pub struct QuotaApi<'a> {
    pub(crate) client: &'a O2CloudClient,
}

impl QuotaApi<'_> {
    /// Consulta la cuota. `GET /sapi/media?action=get-storage-space&softdeleted=true`.
    pub async fn get(&self) -> Result<Quota> {
        self.client
            .get(
                "/media",
                &[("action", "get-storage-space"), ("softdeleted", "true")],
            )
            .await
    }
}
