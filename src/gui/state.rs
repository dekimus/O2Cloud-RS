//! Estado de la GUI (view-model, sin dependencias de UI) — testeable.

use crate::api::models::{Folder, Id, MediaItem, Quota};
use crate::progress::ProgressEvent;
use crate::service::StatusInfo;

/// Pestaña activa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    /// Explorador de archivos.
    #[default]
    Browser,
    /// Cuenta (estado, cuota, suscripción).
    Account,
    /// Sesión (login/logout).
    Session,
}

/// Campos de entrada de las acciones.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    /// Nombre de la carpeta a crear.
    pub mkdir_name: String,
    /// Ruta local a subir.
    pub upload_local: String,
    /// Ruta local de descarga.
    pub download_dest: String,
    /// Carpeta remota destino para `mv`.
    pub move_dest: String,
    /// Marca de borrado permanente.
    pub permanent: bool,
    /// Cookie a importar en la pestaña de sesión.
    pub cookie: String,
}

/// Petición de borrado pendiente de confirmación.
#[derive(Debug, Clone)]
pub struct DeleteRequest {
    /// Referencia remota (id o ruta).
    pub remote: String,
    /// Descripción legible.
    pub label: String,
    /// Borrado permanente.
    pub permanent: bool,
}

/// Progreso de una transferencia.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    /// Mensaje (nombre del archivo).
    pub message: String,
    /// Bytes transferidos.
    pub done: u64,
    /// Total en bytes (0 si se desconoce).
    pub total: u64,
}

impl Progress {
    /// Fracción completada en `0.0..=1.0`.
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.done as f64 / self.total as f64).clamp(0.0, 1.0) as f32
        }
    }
}

/// Estado completo de la aplicación.
#[derive(Debug, Default)]
pub struct AppState {
    /// Pestaña activa.
    pub tab: Tab,
    /// Migas de pan (raíz incluida); la última es la carpeta actual.
    pub breadcrumb: Vec<Folder>,
    /// Subcarpetas de la carpeta actual.
    pub folders: Vec<Folder>,
    /// Archivos de la carpeta actual.
    pub files: Vec<MediaItem>,
    /// Cuota de almacenamiento.
    pub quota: Option<Quota>,
    /// Estado de la sesión.
    pub status: Option<StatusInfo>,
    /// Operación en curso.
    pub busy: bool,
    /// Último mensaje de éxito.
    pub message: Option<String>,
    /// Último error.
    pub error: Option<String>,
    /// Archivo seleccionado (id) para acciones.
    pub selected_file: Option<String>,
    /// Entradas de texto de las acciones.
    pub inputs: Inputs,
    /// Borrado pendiente de confirmación.
    pub pending_delete: Option<DeleteRequest>,
    /// Progreso de la transferencia en curso.
    pub progress: Option<Progress>,
}

impl AppState {
    /// Carpeta actual (última miga de pan).
    #[must_use]
    pub fn current_folder(&self) -> Option<&Folder> {
        self.breadcrumb.last()
    }

    /// Id de la carpeta actual.
    #[must_use]
    pub fn current_id(&self) -> Option<&Id> {
        self.current_folder().map(|folder| &folder.id)
    }

    /// ¿Es la carpeta actual la raíz?
    #[must_use]
    pub fn at_root(&self) -> bool {
        self.breadcrumb
            .last()
            .is_some_and(|folder| folder.name == "/")
    }

    /// Ruta legible de la carpeta actual (p. ej. `/fotos/2024`).
    #[must_use]
    pub fn current_path(&self) -> String {
        let parts: Vec<&str> = self
            .breadcrumb
            .iter()
            .map(|folder| folder.name.as_str())
            .filter(|name| *name != "/")
            .collect();
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    /// Fija la raíz (reinicia las migas).
    pub fn set_root(&mut self, root: Folder) {
        self.breadcrumb = vec![root];
        self.selected_file = None;
    }

    /// Entra en una subcarpeta.
    pub fn enter(&mut self, folder: Folder) {
        self.breadcrumb.push(folder);
        self.selected_file = None;
    }

    /// Sube a la carpeta padre; devuelve `true` si cambió.
    pub fn go_up(&mut self) -> bool {
        if self.breadcrumb.len() > 1 {
            self.breadcrumb.pop();
            self.selected_file = None;
            true
        } else {
            false
        }
    }

    /// Nombre a mostrar del archivo seleccionado.
    #[must_use]
    pub fn selected_name(&self) -> Option<&str> {
        let id = self.selected_file.as_deref()?;
        self.files
            .iter()
            .find(|file| file.id.as_str() == id)
            .and_then(|file| file.name.as_deref())
    }

    /// Aplica un evento de progreso al estado.
    pub fn apply_progress(&mut self, event: ProgressEvent) {
        match event {
            ProgressEvent::Finish(_) | ProgressEvent::Fail(_) => self.progress = None,
            other => {
                let progress = self.progress.get_or_insert_with(Progress::default);
                match other {
                    ProgressEvent::Length(total) => progress.total = total,
                    ProgressEvent::Message(message) => progress.message = message,
                    ProgressEvent::Inc(delta) => progress.done += delta,
                    ProgressEvent::Finish(_) | ProgressEvent::Fail(_) => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: u64, name: &str) -> Folder {
        Folder {
            id: Id::from(id),
            name: name.to_string(),
            status: None,
            magic: false,
            offline: false,
            creationdate: None,
            date: None,
            parentid: None,
        }
    }

    #[test]
    fn breadcrumb_navigation() {
        let mut state = AppState::default();
        assert_eq!(state.current_path(), "/");
        assert!(!state.at_root()); // aún no se ha cargado la raíz

        state.set_root(folder(1, "/"));
        assert!(state.at_root());
        state.enter(folder(2, "fotos"));
        state.enter(folder(3, "2024"));
        assert_eq!(state.current_path(), "/fotos/2024");
        assert_eq!(state.current_id().map(Id::as_str), Some("3"));
        assert!(!state.at_root());

        assert!(state.go_up());
        assert_eq!(state.current_path(), "/fotos");
        assert!(state.go_up());
        assert!(state.at_root());
        assert!(!state.go_up());
    }
}
