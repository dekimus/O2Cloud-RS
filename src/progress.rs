//! Indicador de progreso desacoplado (stderr en la CLI, callback en la GUI).

use std::sync::Arc;

use indicatif::{ProgressBar, ProgressStyle};

/// Evento de progreso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressEvent {
    /// Longitud total (bytes).
    Length(u64),
    /// Mensaje descriptivo (p. ej. nombre del archivo).
    Message(String),
    /// Bytes transferidos.
    Inc(u64),
    /// Completado con un mensaje.
    Finish(String),
    /// Fallido con un mensaje.
    Fail(String),
}

/// Receptor de eventos de progreso.
pub trait ProgressSink: Send + Sync {
    /// Procesa un evento.
    fn on(&self, event: ProgressEvent);
}

/// Reportero de progreso (opcional y clonable).
///
/// Por defecto es silencioso. Se puede crear una barra de `indicatif` (stderr)
/// para la CLI, o un *callback* para la GUI.
#[derive(Clone)]
pub struct ProgressReporter {
    sink: Option<Arc<dyn ProgressSink>>,
}

impl std::fmt::Debug for ProgressReporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgressReporter")
            .field("enabled", &self.sink.is_some())
            .finish()
    }
}

impl Default for ProgressReporter {
    fn default() -> Self {
        Self::silent()
    }
}

impl ProgressReporter {
    /// Reportero silencioso (no-op).
    #[must_use]
    pub fn silent() -> Self {
        Self { sink: None }
    }

    /// Barra de progreso en stderr (`indicatif`), si `enabled`.
    #[must_use]
    pub fn indicatif(enabled: bool) -> Self {
        if !enabled {
            return Self::silent();
        }
        let bar = ProgressBar::new(0);
        if let Ok(style) =
            ProgressStyle::with_template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        {
            bar.set_style(style.progress_chars("=>-"));
        }
        Self {
            sink: Some(Arc::new(IndicatifSink { bar })),
        }
    }

    /// Reportero que invoca `f` con cada evento (para la GUI).
    #[must_use]
    pub fn callback<F>(f: F) -> Self
    where
        F: Fn(ProgressEvent) + Send + Sync + 'static,
    {
        Self {
            sink: Some(Arc::new(CallbackSink { f })),
        }
    }

    /// ¿Hay algún receptor activo?
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.sink.is_some()
    }

    /// Fija la longitud total.
    pub fn set_length(&self, total: u64) {
        self.emit(ProgressEvent::Length(total));
    }

    /// Fija el mensaje descriptivo.
    pub fn set_message(&self, message: impl Into<String>) {
        self.emit(ProgressEvent::Message(message.into()));
    }

    /// Suma bytes transferidos.
    pub fn inc(&self, delta: u64) {
        self.emit(ProgressEvent::Inc(delta));
    }

    /// Marca completado.
    pub fn finish_with_message(&self, message: impl Into<String>) {
        self.emit(ProgressEvent::Finish(message.into()));
    }

    /// Marca fallido.
    pub fn abandon_with_message(&self, message: impl Into<String>) {
        self.emit(ProgressEvent::Fail(message.into()));
    }

    fn emit(&self, event: ProgressEvent) {
        if let Some(sink) = &self.sink {
            sink.on(event);
        }
    }
}

struct IndicatifSink {
    bar: ProgressBar,
}

impl ProgressSink for IndicatifSink {
    fn on(&self, event: ProgressEvent) {
        match event {
            ProgressEvent::Length(total) => self.bar.set_length(total),
            ProgressEvent::Message(message) => self.bar.set_message(message),
            ProgressEvent::Inc(delta) => self.bar.inc(delta),
            ProgressEvent::Finish(message) => self.bar.finish_with_message(message),
            ProgressEvent::Fail(message) => self.bar.abandon_with_message(message),
        }
    }
}

struct CallbackSink<F> {
    f: F,
}

impl<F> ProgressSink for CallbackSink<F>
where
    F: Fn(ProgressEvent) + Send + Sync,
{
    fn on(&self, event: ProgressEvent) {
        (self.f)(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn callback_receives_events() {
        let seen: Arc<Mutex<Vec<ProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_clone = Arc::clone(&seen);
        let reporter = ProgressReporter::callback(move |event| {
            seen_clone.lock().unwrap().push(event);
        });

        reporter.set_length(100);
        reporter.set_message("archivo.bin");
        reporter.inc(40);
        reporter.finish_with_message("ok");

        let events = seen.lock().unwrap().clone();
        assert_eq!(events[0], ProgressEvent::Length(100));
        assert_eq!(events[1], ProgressEvent::Message("archivo.bin".to_string()));
        assert_eq!(events[2], ProgressEvent::Inc(40));
        assert_eq!(events[3], ProgressEvent::Finish("ok".to_string()));
    }

    #[test]
    fn silent_reporter_is_noop() {
        let reporter = ProgressReporter::silent();
        assert!(!reporter.is_enabled());
        reporter.set_length(1);
        reporter.inc(1);
    }
}
