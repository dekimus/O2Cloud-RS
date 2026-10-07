//! Operaciones de sesión/perfil.

use serde_json::Value;

use crate::api::client::O2CloudClient;
use crate::error::Result;

/// Operaciones sobre el recurso `profile` (validación de sesión / whoami).
#[derive(Debug)]
pub struct AuthApi<'a> {
    pub(crate) client: &'a O2CloudClient,
}

impl AuthApi<'_> {
    /// Valida la sesión y devuelve el perfil. `GET /sapi/profile?action=get`.
    ///
    /// Un HTTP 401 se traduce en [`crate::error::Error::Unauthorized`].
    pub async fn whoami(&self) -> Result<Value> {
        self.client.get("/profile", &[("action", "get")]).await
    }
}
