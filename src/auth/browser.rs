//! Login asistido por navegador.
//!
//! El portal usa OIDC con autenticación fuerte (SMS), por lo que no hay login
//! silencioso con usuario/contraseña. Estrategia (ver `AGENTS.md`):
//!
//! 1. Abrir el portal de acceso en el **navegador del sistema**; el usuario
//!    completa teléfono + SMS.
//! 2. Capturar la sesión SAPI resultante (`JSESSIONID` + `validationKey`),
//!    que el portapapeles entrega como la cabecera `Cookie` de una petición
//!    `/sapi/*` ("Copy as cURL").
//! 3. Guardarla en el keyring.
//!
//! El lanzamiento del navegador está desacoplado tras [`BrowserOpener`] para
//! poder testearlo y sustituirlo (p. ej. por una `WebView` embebida) sin tocar
//! el resto. Nunca se usa un shell: el programa se ejecuta directamente con la
//! URL como argumento.
//!
//! Nota de implementación: se descarta una `WebView` embebida en esta iteración
//! porque depende de librerías de sistema pesadas (WebKitGTK/GTK) y no puede
//! ejecutarse en tests; la interfaz `BrowserOpener` permite añadirla después.

use crate::error::{Error, Result};

/// URL de entrada del portal (el navegador completa OIDC + SMS).
pub const LOGIN_URL: &str = "https://cloud.o2online.es/login";

/// Instrucciones mostradas al usuario durante el login asistido.
pub const INSTRUCTIONS: &str = "\
Inicia sesión en la ventana del navegador (teléfono + código SMS).
Cuando el portal te haya dejado dentro:
  1. Abre las DevTools (F12) y ve a la pestaña 'Network'.
  2. Recarga y selecciona cualquier petición a '/sapi/*'.
  3. Clic derecho → Copy → 'Copy as cURL'.
  4. Pega aquí la cabecera Cookie (o el 'Copy as cURL' completo).";

/// Abre una URL en un navegador.
pub trait BrowserOpener: Send + Sync {
    /// Abre `url`; devuelve error si no se pudo lanzar el navegador.
    fn open(&self, url: &str) -> Result<()>;
}

/// Abre la URL con el navegador por defecto del sistema.
///
/// Se ejecuta el lanzador directamente (sin shell) pasando la URL como
/// argumento. Es un traspaso de UI, no procesamiento de datos.
#[derive(Debug, Default)]
pub struct SystemBrowserOpener;

impl BrowserOpener for SystemBrowserOpener {
    fn open(&self, url: &str) -> Result<()> {
        // Evita inyección de argumentos al lanzador: solo http(s).
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(Error::Protocol(
                "refusing to open a non-http(s) URL".to_string(),
            ));
        }

        let program = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(target_os = "windows") {
            "explorer"
        } else {
            "xdg-open"
        };

        let status = std::process::Command::new(program)
            .arg(url)
            .status()
            .map_err(|e| Error::LocalIo(format!("cannot launch '{program}': {e}")))?;

        if status.success() {
            Ok(())
        } else {
            Err(Error::LocalIo(format!("'{program}' exited with {status}")))
        }
    }
}

/// `BrowserOpener` de prueba que registra las URLs abiertas.
#[derive(Debug, Default)]
pub struct RecordingOpener {
    opened: std::sync::Mutex<Vec<String>>,
}

impl RecordingOpener {
    /// Crea un opener vacío.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// URLs registradas.
    #[must_use]
    pub fn opened(&self) -> Vec<String> {
        self.opened
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }
}

impl BrowserOpener for RecordingOpener {
    fn open(&self, url: &str) -> Result<()> {
        self.opened
            .lock()
            .map_err(|_| Error::LocalIo("recording opener poisoned".to_string()))?
            .push(url.to_string());
        Ok(())
    }
}

/// Abre la página de acceso del portal con el opener dado.
pub fn open_login_page(opener: &dyn BrowserOpener) -> Result<()> {
    opener.open(LOGIN_URL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_login_url() {
        let opener = RecordingOpener::new();
        open_login_page(&opener).unwrap();
        assert_eq!(opener.opened(), vec![LOGIN_URL.to_string()]);
    }
}
