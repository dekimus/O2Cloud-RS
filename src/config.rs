//! Configuración de la aplicación (no secretos).

/// Host principal del portal / SAPI.
pub const DEFAULT_BASE_URL: &str = "https://cloud.o2online.es";
/// Host dedicado de subida (no usado en el slice de lectura).
pub const DEFAULT_UPLOAD_BASE_URL: &str = "https://upload.cloud.o2online.es";
/// Perfil por defecto cuando no se pasa `--profile`.
pub const DEFAULT_PROFILE: &str = "default";
/// User-Agent usado por el cliente.
pub const USER_AGENT: &str = concat!(
    "o2cloud/",
    env!("CARGO_PKG_VERSION"),
    " (Linux; no oficial)"
);

/// Configuración no sensible del cliente.
///
/// Los valores por defecto se pueden sobrescribir con variables de entorno
/// `O2CLOUD_*` (orientadas a testing/automatización explícita).
#[derive(Debug, Clone)]
pub struct AppConfig {
    base_url: String,
    upload_base_url: String,
    user_agent: String,
    device_id: String,
    profile: String,
}

impl AppConfig {
    /// Construye la configuración a partir del perfil y del entorno.
    #[must_use]
    pub fn new(profile: Option<String>) -> Self {
        let profile = profile.unwrap_or_else(|| env("O2CLOUD_PROFILE", DEFAULT_PROFILE));
        Self {
            base_url: env("O2CLOUD_BASE_URL", DEFAULT_BASE_URL),
            upload_base_url: env("O2CLOUD_UPLOAD_BASE_URL", DEFAULT_UPLOAD_BASE_URL),
            user_agent: env("O2CLOUD_USER_AGENT", USER_AGENT),
            device_id: env("O2CLOUD_DEVICE_ID", "o2cloud-cli"),
            profile,
        }
    }

    /// URL base sin barra final.
    #[must_use]
    pub fn base_url(&self) -> &str {
        self.base_url.trim_end_matches('/')
    }

    /// Host de subida sin barra final.
    #[must_use]
    pub fn upload_base_url(&self) -> &str {
        self.upload_base_url.trim_end_matches('/')
    }

    /// User-Agent configurado.
    #[must_use]
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    /// Identificador de dispositivo (para cabeceras de subida).
    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Perfil activo (namespaca el keyring).
    #[must_use]
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// Nombre del *service* del keyring para este perfil.
    #[must_use]
    pub fn keyring_service(&self) -> String {
        format!("o2cloud:{}", self.profile)
    }

    /// Construye configuración apuntando a una URL base concreta.
    ///
    /// Pensado para tests y herramientas; no debe usarse en producción.
    #[must_use]
    #[doc(hidden)]
    pub fn with_base_url(base_url: impl Into<String>, profile: Option<String>) -> Self {
        let mut config = Self::new(profile);
        config.base_url = base_url.into();
        config
    }

    /// Construye configuración apuntando a una URL base y a un host de subida.
    ///
    /// Pensado para tests y herramientas; no debe usarse en producción.
    #[must_use]
    #[doc(hidden)]
    pub fn with_base_urls(
        base_url: impl Into<String>,
        upload_base_url: impl Into<String>,
        profile: Option<String>,
    ) -> Self {
        let mut config = Self::with_base_url(base_url, profile);
        config.upload_base_url = upload_base_url.into();
        config
    }
}

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let cfg = AppConfig::new(None);
        assert_eq!(cfg.base_url(), DEFAULT_BASE_URL);
        assert_eq!(cfg.profile(), "default");
    }

    #[test]
    fn profile_is_honoured() {
        let cfg = AppConfig::new(Some("work".into()));
        assert_eq!(cfg.profile(), "work");
        assert_eq!(cfg.keyring_service(), "o2cloud:work");
    }
}
