//! Interfaz gráfica nativa (feature `gui`, eframe/egui).
//!
//! Se apoya en la misma librería (`service`/`api`/`auth`) que la CLI; la UI solo
//! compone vistas y manda comandos al [`worker`] async.

mod app;
pub mod state;
mod worker;

pub use app::O2CloudApp;
pub use state::{AppState, DeleteRequest, Inputs, Tab};
pub use worker::{Command, Event, Worker};

use eframe::egui;

/// Lanza la ventana de la aplicación. Requiere un entorno gráfico.
///
/// # Errors
/// Devuelve error si no se puede crear la ventana o el backend gráfico falla.
pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 720.0])
            .with_title("O2 Cloud"),
        ..Default::default()
    };
    eframe::run_native(
        "O2 Cloud",
        options,
        Box::new(|cc| Ok(Box::new(O2CloudApp::new(cc)))),
    )
}
