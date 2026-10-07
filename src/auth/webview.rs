//! Login con `WebView` embebida (feature `webview`, solo Linux).
//!
//! Abre el portal en una `WebView` `WebKitGTK` dentro de una ventana GTK, deja
//! que el usuario complete el login (teléfono + SMS) y captura la sesión SAPI
//! leyendo las cookies del propio motor (`JSESSIONID` + `validationKey`). Al
//! capturarlas, cierra la ventana y devuelve la cabecera `Cookie` completa.
//!
//! Se usa directamente el bucle de GTK (sin `winit`/`tao`) porque es la ruta
//! nativa de `wry` en Linux y evita dependencias adicionales. La `WebView` es
//! `!Send`: todo ocurre en el hilo que llama a [`capture_cookie_header`], que
//! debe ser el hilo principal (GTK lo exige).
//!
//! Alternativa sin esta feature: login asistido por navegador del sistema
//! (ver [`crate::auth::browser`]).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use wry::WebViewBuilderExtUnix;

use crate::auth::browser::LOGIN_URL;
use crate::error::{Error, Result};

/// Origen donde viven las cookies de sesión SAPI.
const ORIGIN: &str = "https://cloud.o2online.es/";
/// Intervalo entre comprobaciones de cookies.
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Tiempo máximo de espera antes de cancelar.
const OVERALL_TIMEOUT: Duration = Duration::from_secs(300);

/// Tiempo máximo de espera, configurable vía `O2CLOUD_WEBVIEW_TIMEOUT_SECS`.
fn overall_timeout() -> Duration {
    std::env::var("O2CLOUD_WEBVIEW_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(OVERALL_TIMEOUT, Duration::from_secs)
}

/// URL que carga la `WebView` (override de test: `O2CLOUD_WEBVIEW_URL`).
fn login_url() -> String {
    env_or("O2CLOUD_WEBVIEW_URL", LOGIN_URL)
}

/// Origen del que se leen las cookies (override: `O2CLOUD_WEBVIEW_ORIGIN`).
fn origin() -> String {
    env_or("O2CLOUD_WEBVIEW_ORIGIN", ORIGIN)
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Abre la `WebView`, espera el login y devuelve la cabecera `Cookie`.
///
/// Bloquea hasta que se capturan las cookies, se cierra la ventana o se agota
/// el tiempo. Requiere una sesión gráfica (X11/Wayland); si no hay display,
/// GTK falla y se devuelve un error claro.
pub fn capture_cookie_header() -> Result<String> {
    gtk::init().map_err(|e| Error::LocalIo(format!("cannot initialize GTK (no display?): {e}")))?;

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("O2 Cloud — Iniciar sesión");
    window.set_default_size(480, 720);
    let fixed = gtk::Fixed::new();
    window.add(&fixed);
    window.show_all();

    let webview = Rc::new(
        wry::WebViewBuilder::new()
            .with_url(login_url())
            .build_gtk(&fixed)
            .map_err(|e| Error::LocalIo(format!("cannot create webview: {e}")))?,
    );

    let result: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let finished = Rc::new(RefCell::new(false));
    let origin = origin();

    // Sondea las cookies hasta capturar la sesión completa.
    {
        let webview = Rc::clone(&webview);
        let result = Rc::clone(&result);
        let finished = Rc::clone(&finished);
        gtk::glib::timeout_add_local(POLL_INTERVAL, move || {
            if let Ok(cookies) = webview.cookies_for_url(&origin) {
                let pairs = cookies
                    .into_iter()
                    .map(|cookie| (cookie.name().to_string(), cookie.value().to_string()));
                if let Some(header) = cookie_header(pairs) {
                    *result.borrow_mut() = Some(header);
                    *finished.borrow_mut() = true;
                    gtk::main_quit();
                    return gtk::glib::ControlFlow::Break;
                }
            }
            gtk::glib::ControlFlow::Continue
        });
    }

    // Timeout global.
    {
        let finished = Rc::clone(&finished);
        gtk::glib::timeout_add_local(overall_timeout(), move || {
            if !*finished.borrow() {
                gtk::main_quit();
            }
            gtk::glib::ControlFlow::Break
        });
    }

    // Cerrar la ventana cancela el login.
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        gtk::glib::Propagation::Proceed
    });

    gtk::main();

    let captured = result.borrow().clone();
    captured.ok_or(Error::Cancelled)
}

/// Construye la cabecera `Cookie` a partir de pares `(nombre, valor)`.
///
/// Devuelve `None` mientras no estén presentes **ambas** credenciales
/// (`JSESSIONID` y `validationKey`), para no capturar una sesión incompleta.
fn cookie_header(pairs: impl IntoIterator<Item = (String, String)>) -> Option<String> {
    let mut has_jsession = false;
    let mut has_key = false;
    let mut parts = Vec::new();

    for (name, value) in pairs {
        if value.is_empty() {
            continue;
        }
        if name.eq_ignore_ascii_case("JSESSIONID") {
            has_jsession = true;
        }
        if name.eq_ignore_ascii_case("validationKey") {
            has_key = true;
        }
        parts.push(format!("{name}={value}"));
    }

    if has_jsession && has_key {
        Some(parts.join("; "))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn header_requires_both_credentials() {
        assert!(cookie_header(pairs(&[("JSESSIONID", "a")])).is_none());
        assert!(cookie_header(pairs(&[("validationKey", "b")])).is_none());
        assert_eq!(
            cookie_header(pairs(&[("JSESSIONID", "a"), ("validationKey", "b")])),
            Some("JSESSIONID=a; validationKey=b".to_string())
        );
    }

    #[test]
    fn header_keeps_extra_cookies_and_skips_empty() {
        let header = cookie_header(pairs(&[
            ("JSESSIONID", "a"),
            ("other", "x"),
            ("validationKey", "b"),
            ("empty", ""),
        ]))
        .unwrap();
        assert!(header.contains("other=x"));
        assert!(!header.contains("empty="));
    }
}
