//! Almacenamiento seguro de secretos (cookie de sesión).

use crate::error::{Error, Result};

/// Almacén de secretos de sesión.
///
/// El único secreto persistido es la cabecera `Cookie` completa (que contiene
/// `JSESSIONID` + `validationKey`). Nunca se guardan contraseñas ni se escriben
/// secretos en archivos de configuración.
pub trait SecretStore: Send + Sync {
    /// Devuelve la cookie almacenada, si existe.
    fn get_cookie(&self) -> Result<Option<String>>;
    /// Almacena la cookie.
    fn set_cookie(&self, value: &str) -> Result<()>;
    /// Borra la cookie almacenada.
    fn clear(&self) -> Result<()>;
}

/// Almacén basado en el keyring del sistema.
pub struct KeyringStore {
    entry: keyring::Entry,
}

impl std::fmt::Debug for KeyringStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No exponer el `Entry` (podría filtrar el nombre del servicio).
        f.debug_struct("KeyringStore").finish_non_exhaustive()
    }
}

impl KeyringStore {
    /// Crea un almacén de keyring para un perfil dado.
    ///
    /// `service` es el nombre del servicio (p. ej. `o2cloud:default`).
    pub fn new(service: &str) -> Result<Self> {
        let entry = keyring::Entry::new(service, "session")
            .map_err(|e| Error::LocalIo(format!("keyring unavailable: {e}")))?;
        Ok(Self { entry })
    }
}

impl SecretStore for KeyringStore {
    fn get_cookie(&self) -> Result<Option<String>> {
        match self.entry.get_password() {
            Ok(pwd) => Ok(Some(pwd)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Error::LocalIo(format!("keyring read failed: {e}"))),
        }
    }

    fn set_cookie(&self, value: &str) -> Result<()> {
        self.entry
            .set_password(value)
            .map_err(|e| Error::LocalIo(format!("keyring write failed: {e}")))
    }

    fn clear(&self) -> Result<()> {
        match self.entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::LocalIo(format!("keyring delete failed: {e}"))),
        }
    }
}

/// Almacén que lee la cookie de la variable de entorno `O2CLOUD_COOKIE`.
///
/// Orientado a testing/automatización explícita. `set_cookie`/`clear` son
/// no-ops (no se escribe en el entorno).
#[derive(Debug, Default)]
pub struct EnvStore;

impl SecretStore for EnvStore {
    fn get_cookie(&self) -> Result<Option<String>> {
        Ok(std::env::var("O2CLOUD_COOKIE")
            .ok()
            .filter(|v| !v.trim().is_empty()))
    }

    fn set_cookie(&self, _value: &str) -> Result<()> {
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        Ok(())
    }
}

/// Almacén en memoria (tests y casos de uso sin keyring).
#[derive(Default)]
pub struct MemoryStore {
    cookie: std::sync::Mutex<Option<String>>,
}

impl std::fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Nunca exponer la cookie almacenada.
        let has_cookie = self.cookie.lock().is_ok_and(|c| c.is_some());
        f.debug_struct("MemoryStore")
            .field("has_cookie", &has_cookie)
            .finish()
    }
}

impl MemoryStore {
    /// Crea un almacén en memoria vacío.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for MemoryStore {
    fn get_cookie(&self) -> Result<Option<String>> {
        Ok(self
            .cookie
            .lock()
            .map_err(|_| Error::LocalIo("memory store poisoned".to_string()))?
            .clone())
    }

    fn set_cookie(&self, value: &str) -> Result<()> {
        *self
            .cookie
            .lock()
            .map_err(|_| Error::LocalIo("memory store poisoned".to_string()))? =
            Some(value.to_string());
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        *self
            .cookie
            .lock()
            .map_err(|_| Error::LocalIo("memory store poisoned".to_string()))? = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trips() {
        let store = MemoryStore::new();
        assert!(store.get_cookie().unwrap().is_none());
        store
            .set_cookie("JSESSIONID=x; validationKey=0123456789abcdef0123456789abcdef")
            .unwrap();
        assert_eq!(
            store.get_cookie().unwrap().as_deref(),
            Some("JSESSIONID=x; validationKey=0123456789abcdef0123456789abcdef")
        );
        store.clear().unwrap();
        assert!(store.get_cookie().unwrap().is_none());
    }
}
