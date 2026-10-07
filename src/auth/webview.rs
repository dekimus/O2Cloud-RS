//! Login con `WebView` embebida (feature `webview`, solo Linux).
//!
//! Abre el portal en una `WebView` `WebKitGTK` dentro de una ventana GTK, deja
//! que el usuario complete el login (teléfono + SMS) y captura la sesión SAPI
//! leyendo las cookies del propio motor (`JSESSIONID` + `validationKey`).
//!
//! Se usa directamente el bucle de GTK (sin `winit`/`tao`) porque es la ruta
//! nativa de `wry` en Linux. La `WebView` es `!Send`: todo ocurre en el hilo
//! que llama a [`capture_cookie_header`], que debe ser el hilo principal.
//!
//! Si la `WebView` no puede usarse (sin display, backend gráfico roto o la
//! página no carga), se devuelve [`CaptureOutcome::Unavailable`] para que la
//! CLI caiga al login con el navegador del sistema.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use wry::WebViewBuilderExtUnix;

use crate::auth::browser::LOGIN_URL;

/// Origen donde viven las cookies de sesión SAPI.
const ORIGIN: &str = "https://cloud.o2online.es/";
/// Intervalo entre comprobaciones de cookies.
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Tiempo máximo para que cargue la primera página antes de considerarlo fallo.
const LOAD_TIMEOUT: Duration = Duration::from_secs(30);
/// Tiempo máximo de espera del login antes de cancelar.
const OVERALL_TIMEOUT: Duration = Duration::from_secs(300);
/// Anchura inicial de la ventana.
const WINDOW_WIDTH: i32 = 640;
/// Altura inicial de la ventana.
const WINDOW_HEIGHT: i32 = 800;

/// Mensaje de ayuda cuando la `WebView` no funciona.
pub const TROUBLESHOOTING: &str = "\
Sugerencias (el renderizador DMABUF/compositing ya está desactivado por defecto):
  - Forzar X11:            export GDK_BACKEND=x11
  - Renderizado software:  export LIBGL_ALWAYS_SOFTWARE=1
  - Desactivar el sandbox de WebKitGTK (menos seguro):
        export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
  - Importación manual:
        o2cloud login --import-cookie '<Cookie: ...>'";

/// Resultado de la captura con la `WebView`.
#[derive(Debug)]
pub enum CaptureOutcome {
    /// Sesión capturada (cabecera `Cookie` completa).
    Captured(String),
    /// El usuario cerró la ventana o no completó el login a tiempo.
    Cancelled,
    /// La `WebView` no pudo usarse; la CLI debería caer al navegador del sistema.
    Unavailable(String),
}

/// Abre la `WebView`, espera el login y devuelve la cabecera `Cookie`.
///
/// Bloquea hasta capturar las cookies, cerrar la ventana o agotar el tiempo.
#[must_use]
pub fn capture_cookie_header() -> CaptureOutcome {
    configure_graphics();

    if let Err(e) = gtk::init() {
        return CaptureOutcome::Unavailable(format!("cannot initialize GTK (no display?): {e}"));
    }

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("O2 Cloud — Iniciar sesión");
    window.set_default_size(WINDOW_WIDTH, WINDOW_HEIGHT);
    let fixed = gtk::Fixed::new();
    window.add(&fixed);

    let result: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let finished = Rc::new(RefCell::new(false));
    let loaded = Rc::new(RefCell::new(false));

    let loaded_for_handler = Rc::clone(&loaded);
    let builder = wry::WebViewBuilder::new()
        .with_url(login_url())
        .with_bounds(rect(f64::from(WINDOW_WIDTH), f64::from(WINDOW_HEIGHT)))
        .with_on_page_load_handler(move |event, _url| {
            if matches!(event, wry::PageLoadEvent::Finished) {
                *loaded_for_handler.borrow_mut() = true;
            }
        });

    let webview = match builder.build_gtk(&fixed) {
        Ok(webview) => Rc::new(webview),
        Err(e) => return CaptureOutcome::Unavailable(format!("cannot create webview: {e}")),
    };

    // En GTK la WebView no se redimensiona sola: hay que fijar sus bounds al
    // tamaño de la ventana (si no, se queda en su tamaño por defecto).
    {
        let webview = Rc::clone(&webview);
        window.connect_size_allocate(move |_, allocation| {
            tracing::debug!(
                width = allocation.width(),
                height = allocation.height(),
                "resizing webview to window"
            );
            let _ = webview.set_bounds(rect(
                f64::from(allocation.width()),
                f64::from(allocation.height()),
            ));
        });
    }

    window.show_all();

    let origin = origin();

    // Sondea las cookies hasta capturar la sesión completa.
    {
        let webview = Rc::clone(&webview);
        let result = Rc::clone(&result);
        let finished = Rc::clone(&finished);
        let window = window.clone();
        gtk::glib::timeout_add_local(POLL_INTERVAL, move || {
            if let Ok(cookies) = webview.cookies_for_url(&origin) {
                let pairs = cookies
                    .into_iter()
                    .map(|cookie| (cookie.name().to_string(), cookie.value().to_string()));
                if let Some(header) = cookie_header(pairs) {
                    *result.borrow_mut() = Some(header);
                    *finished.borrow_mut() = true;
                    tracing::debug!("cookies capturadas; cerrando la WebView");
                    // Ocultar la ventana desde dentro del bucle (es seguro aquí).
                    window.set_visible(false);
                    gtk::main_quit();
                    return gtk::glib::ControlFlow::Break;
                }
            }
            gtk::glib::ControlFlow::Continue
        });
    }

    // Watchdog de carga: si la primera página no termina de cargar, aborta.
    {
        let loaded = Rc::clone(&loaded);
        let finished = Rc::clone(&finished);
        gtk::glib::timeout_add_local(load_timeout(), move || {
            if !*loaded.borrow() && !*finished.borrow() {
                gtk::main_quit();
            }
            gtk::glib::ControlFlow::Break
        });
    }

    // Timeout global.
    gtk::glib::timeout_add_local(overall_timeout(), || {
        gtk::main_quit();
        gtk::glib::ControlFlow::Break
    });

    // Cerrar la ventana cancela el login.
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        gtk::glib::Propagation::Proceed
    });

    gtk::main();

    // El bucle principal del hilo ya no corre: asegurarse de que no quede
    // ninguna ventana visible (p. ej. en el caso de timeout), para no dejar una
    // ventana "congelada".
    for widget in gtk::Window::list_toplevels() {
        widget.set_visible(false);
    }

    let captured = result.borrow().clone();
    if let Some(header) = captured {
        CaptureOutcome::Captured(header)
    } else if *loaded.borrow() {
        // La página cargó pero no se completó el login (cierre o timeout).
        CaptureOutcome::Cancelled
    } else {
        CaptureOutcome::Unavailable(
            "the login page did not load (WebKit network process or GPU/backend issue)".to_string(),
        )
    }
}

/// Configura el entorno gráfico para maximizar la compatibilidad en Linux.
///
/// - Prefiere X11/XWayland (`GDK_BACKEND`), más fiable con `WebKitGTK`.
/// - Desactiva el renderizador DMABUF y el *compositing* de `WebKitGTK`.
///   Sin esto, en muchos entornos (renderizado software, contenedores,
///   drivers, `XWayland`) aparece `Failed to create GBM buffer` y la página
///   queda en blanco.
///
/// Todos los valores respetan una variable de entorno ya definida, de modo
/// que se pueden sobrescribir.
fn configure_graphics() {
    prefer_x11_backend();
    set_env_default("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    set_env_default("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
}

/// Define `key=value` solo si la variable no está ya definida.
fn set_env_default(key: &str, value: &str) {
    if std::env::var_os(key).is_none() {
        std::env::set_var(key, value);
    }
}

/// Fuerza X11 por defecto para la `WebView` si no se ha elegido backend.
///
/// `WebKitGTK` es más fiable bajo X11/XWayland; se puede cambiar con
/// `O2CLOUD_WEBVIEW_BACKEND` o `GDK_BACKEND`.
fn prefer_x11_backend() {
    if std::env::var_os("GDK_BACKEND").is_some() {
        return;
    }
    let backend = std::env::var("O2CLOUD_WEBVIEW_BACKEND").unwrap_or_else(|_| "x11".to_string());
    if !backend.is_empty() {
        std::env::set_var("GDK_BACKEND", backend);
    }
}

/// Tiempo máximo de espera, configurable vía `O2CLOUD_WEBVIEW_TIMEOUT_SECS`.
fn overall_timeout() -> Duration {
    std::env::var("O2CLOUD_WEBVIEW_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(OVERALL_TIMEOUT, Duration::from_secs)
}

/// Tiempo máximo de carga inicial, vía `O2CLOUD_WEBVIEW_LOAD_TIMEOUT_SECS`.
fn load_timeout() -> Duration {
    std::env::var("O2CLOUD_WEBVIEW_LOAD_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(LOAD_TIMEOUT, Duration::from_secs)
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

/// Construye un `Rect` lógico anclado a la esquina superior izquierda.
fn rect(width: f64, height: f64) -> wry::Rect {
    wry::Rect {
        position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(0.0, 0.0)),
        size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(width, height)),
    }
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
