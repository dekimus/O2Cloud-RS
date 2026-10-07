//! Caso de uso: consulta de cuota.

use crate::api::client::O2CloudClient;
use crate::api::models::Quota;
use crate::error::Result;

/// Consulta la cuota de almacenamiento.
pub async fn quota(client: &O2CloudClient) -> Result<Quota> {
    client.quota().get().await
}
