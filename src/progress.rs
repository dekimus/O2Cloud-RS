//! Indicador de progreso (stderr), desacoplado del CLI.

use indicatif::{ProgressBar, ProgressStyle};

/// Reportero de progreso.
///
/// Es un envoltorio ligero sobre `indicatif` que se puede clonar y mover a un
/// stream de subida. Si está deshabilitado, todas las operaciones son no-ops
/// (modo `--quiet`/`--json` o sin TTY).
#[derive(Debug, Clone)]
pub struct ProgressReporter {
    bar: Option<ProgressBar>,
}

impl ProgressReporter {
    /// Crea un reportero para `total` bytes. Si `enabled` es `false`, es no-op.
    #[must_use]
    pub fn new(total: u64, enabled: bool) -> Self {
        if !enabled {
            return Self { bar: None };
        }
        let bar = ProgressBar::new(total);
        if let Ok(style) =
            ProgressStyle::with_template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        {
            bar.set_style(style.progress_chars("=>-"));
        }
        Self { bar: Some(bar) }
    }

    /// Fija el mensaje (nombre del archivo).
    pub fn set_message(&self, message: impl Into<std::borrow::Cow<'static, str>>) {
        if let Some(bar) = &self.bar {
            bar.set_message(message);
        }
    }

    /// Incrementa el contador de bytes transferidos.
    pub fn inc(&self, delta: u64) {
        if let Some(bar) = &self.bar {
            bar.inc(delta);
        }
    }

    /// Marca el progreso como completado con un mensaje.
    pub fn finish_with_message(&self, message: impl Into<std::borrow::Cow<'static, str>>) {
        if let Some(bar) = &self.bar {
            bar.finish_with_message(message);
        }
    }

    /// Marca el progreso como fallido con un mensaje.
    pub fn abandon_with_message(&self, message: impl Into<std::borrow::Cow<'static, str>>) {
        if let Some(bar) = &self.bar {
            bar.abandon_with_message(message);
        }
    }
}
