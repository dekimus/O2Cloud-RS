//! Caso de uso: estado de la sesión.

use serde_json::Value;

use crate::api::client::O2CloudClient;
use crate::error::Result;
use crate::service::extract_identity;

/// Resultado del comando `status`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StatusInfo {
    /// `true` si hay una sesión válida.
    pub authenticated: bool,
    /// Identidad legible (email/usuario/msisdn) si se pudo extraer.
    pub identity: Option<String>,
    /// Perfil crudo (para salida JSON).
    pub profile: Option<Value>,
}

/// Determina el estado de autenticación.
///
/// - Sin sesión local → `authenticated = false`.
/// - Con sesión, hace `whoami` (probe): éxito → `authenticated = true`; un 401
///   se propaga como [`crate::error::Error::Unauthorized`] con mensaje claro.
pub async fn status(client: &O2CloudClient) -> Result<StatusInfo> {
    let Some(_session) = client.session() else {
        return Ok(StatusInfo {
            authenticated: false,
            identity: None,
            profile: None,
        });
    };

    let profile = client.auth().whoami().await?;
    let identity = extract_identity(&profile);
    Ok(StatusInfo {
        authenticated: true,
        identity,
        profile: Some(profile),
    })
}
