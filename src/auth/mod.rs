//! Autenticación: sesión SAPI, almacenamiento seguro y login por navegador.

pub mod browser;
pub mod session;
pub mod store;

#[cfg(all(feature = "webview", target_os = "linux"))]
pub mod webview;

pub use browser::{
    open_login_page, BrowserOpener, RecordingOpener, SystemBrowserOpener, INSTRUCTIONS, LOGIN_URL,
};
pub use session::Session;
pub use store::{EnvStore, KeyringStore, MemoryStore, SecretStore};

use crate::error::Result;

/// Valida y guarda en el almacén una sesión a partir de una cookie pegada.
///
/// Acepta la cabecera `Cookie` cruda, el prefijo `Cookie:`, o un fragmento de
/// "Copy as cURL" (`-b '…'`). Almacena la cabecera **normalizada** (no el texto
/// pegado tal cual) y devuelve la [`Session`] resultante.
pub fn save_session(store: &dyn SecretStore, cookie: &str) -> Result<Session> {
    let session = Session::from_cookie_header(cookie)?;
    store.set_cookie(session.cookie_header())?;
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_session_normalizes_and_stores() {
        let store = MemoryStore::new();
        let session = save_session(
            &store,
            "-b 'JSESSIONID=x; validationKey=0123456789abcdef0123456789abcdef'",
        )
        .unwrap();
        assert_eq!(session.validation_key(), "0123456789abcdef0123456789abcdef");
        assert!(session.has_jsession_id());
        assert_eq!(
            store.get_cookie().unwrap().as_deref(),
            Some("JSESSIONID=x; validationKey=0123456789abcdef0123456789abcdef")
        );
    }
}
