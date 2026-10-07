//! Aplicación de escritorio (eframe/egui).

use eframe::egui;

use crate::gui::state::{AppState, DeleteRequest, Selection, Tab};
use crate::gui::worker::{Command, Event, Worker};

/// Aplicación GUI.
#[derive(Debug)]
pub struct O2CloudApp {
    state: AppState,
    worker: Worker,
    /// Hay que recargar la vista tras una mutación.
    refresh_after_action: bool,
}

impl O2CloudApp {
    /// Crea la app y lanza la inicialización.
    #[must_use]
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let worker = Worker::spawn(cc.egui_ctx.clone());
        worker.send(Command::Init);
        Self {
            state: AppState::default(),
            worker,
            refresh_after_action: false,
        }
    }

    fn apply_events(&mut self) {
        for event in self.worker.drain() {
            match event {
                Event::Busy(busy) => self.state.busy = busy,
                Event::Root(folder) => self.state.set_root(folder),
                Event::Status(status) => {
                    if !status.authenticated {
                        // Sesión cerrada/expirada: limpiar la vista.
                        self.state.breadcrumb.clear();
                        self.state.folders.clear();
                        self.state.files.clear();
                        self.state.trash.clear();
                        self.state.quota = None;
                        self.state.selected = None;
                    }
                    self.state.status = Some(status);
                }
                Event::Quota(quota) => self.state.quota = Some(quota),
                Event::Listing(listing) => {
                    self.state.folders = listing.folders;
                    self.state.files = listing.files;
                    self.drop_missing_selection();
                }
                Event::Trash(entries) => self.state.trash = entries,
                Event::Progress(event) => self.state.apply_progress(event),
                Event::Changed => self.refresh_after_action = true,
                Event::Message(message) => {
                    self.state.message = Some(message);
                    self.state.error = None;
                }
                Event::Error(error) => {
                    self.state.error = Some(error);
                    self.state.message = None;
                }
            }
        }
    }

    /// Deselecciona si el elemento ya no está en la carpeta actual.
    fn drop_missing_selection(&mut self) {
        let Some(id) = self.state.selected_id().cloned() else {
            return;
        };
        let present = self.state.folders.iter().any(|folder| folder.id == id)
            || self.state.files.iter().any(|file| file.id == id);
        if !present {
            self.state.selected = None;
        }
    }

    /// Recarga la vista actual (carpeta, cuota y papelera si procede).
    fn refresh_view(&self) {
        let authenticated = self
            .state
            .status
            .as_ref()
            .is_some_and(|status| status.authenticated);
        if !authenticated {
            return;
        }
        self.worker.send(Command::Refresh);
        if self.state.current_id().is_some() {
            self.reload_current();
        }
        if self.state.tab == Tab::Trash {
            self.worker.send(Command::TrashList);
        }
    }

    fn reload_current(&self) {
        if let Some(id) = self.state.current_id().cloned() {
            self.worker.send(Command::List(id));
        }
    }

    fn ui_top(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("O2 Cloud");
                ui.separator();
                ui.selectable_value(&mut self.state.tab, Tab::Browser, "Archivos");
                if ui
                    .selectable_value(&mut self.state.tab, Tab::Trash, "Papelera")
                    .clicked()
                {
                    self.worker.send(Command::TrashList);
                }
                ui.selectable_value(&mut self.state.tab, Tab::Account, "Cuenta");
                ui.selectable_value(&mut self.state.tab, Tab::Session, "Sesión");
                ui.separator();
                let connected = self
                    .state
                    .status
                    .as_ref()
                    .is_some_and(|status| status.authenticated);
                if connected {
                    ui.label("🟢 Conectado");
                } else {
                    ui.label("🔴 Sin sesión");
                }
                if self.state.busy {
                    ui.spinner();
                }
                if ui.button("Refrescar").clicked() {
                    self.worker.send(Command::Refresh);
                    self.reload_current();
                }
            });
        });
    }

    fn ui_bottom(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(error) = &self.state.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("⚠ {error}"));
                } else if let Some(message) = &self.state.message {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, format!("✓ {message}"));
                } else if let Some(progress) = &self.state.progress {
                    ui.add(egui::ProgressBar::new(progress.fraction()).text(format!(
                        "{}  {}/{}",
                        progress.message, progress.done, progress.total
                    )));
                } else {
                    ui.label("Listo.");
                }
            });
        });
    }

    fn ui_browser(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("actions")
            .resizable(false)
            .default_size(260.0)
            .show(ui, |ui| self.ui_actions(ui));

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                let at_root = self.state.at_root();
                if ui
                    .add_enabled(!at_root, egui::Button::new("⬆ Subir"))
                    .clicked()
                    && self.state.go_up()
                {
                    self.reload_current();
                }
                ui.label(format!("📁 {}", self.state.current_path()));
                ui.separator();
                ui.label("(clic = seleccionar · doble clic = abrir)");
            });
            ui.separator();

            let folders = self.state.folders.clone();
            let files = self.state.files.clone();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for folder in &folders {
                    let selected = matches!(
                        &self.state.selected,
                        Some(Selection::Folder(selected)) if selected.id == folder.id
                    );
                    let response = ui.selectable_label(selected, format!("📁 {}/", folder.name));
                    if response.double_clicked() {
                        self.state.enter(folder.clone());
                        self.worker.send(Command::List(folder.id.clone()));
                    } else if response.clicked() {
                        self.state.select_folder(folder.clone());
                    }
                }
                for file in &files {
                    let selected = matches!(
                        &self.state.selected,
                        Some(Selection::File(selected)) if selected.id == file.id
                    );
                    let name = file.name.as_deref().unwrap_or("?");
                    let size = file
                        .size
                        .map_or_else(String::new, crate::util::format_bytes);
                    let label = format!("📄 {name}    {size}    [{}]", file.id);
                    if ui.selectable_label(selected, label).clicked() {
                        self.state.select_file(file.clone());
                    }
                }
                if folders.is_empty() && files.is_empty() {
                    ui.label("(carpeta vacía)");
                }
            });
        });
    }

    fn ui_actions(&mut self, ui: &mut egui::Ui) {
        ui.heading("Acciones");

        ui.label("Nueva carpeta:");
        ui.text_edit_singleline(&mut self.state.inputs.mkdir_name);
        if ui.button("Crear carpeta").clicked() {
            let name = self.state.inputs.mkdir_name.trim().to_string();
            if !name.is_empty() {
                let path = if self.state.at_root() {
                    format!("/{name}")
                } else {
                    format!("{}/{}", self.state.current_path(), name)
                };
                self.worker.send(Command::Mkdir(path));
                self.state.inputs.mkdir_name.clear();
            }
        }

        ui.separator();
        ui.label("Subir archivo (ruta local):");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.state.inputs.upload_local);
            if ui.button("…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    self.state.inputs.upload_local = path.display().to_string();
                }
            }
        });
        if ui.button("Subir").clicked() {
            if let Some(folder) = self.state.current_id().cloned() {
                let local = self.state.inputs.upload_local.trim().to_string();
                if !local.is_empty() {
                    self.worker.send(Command::Upload { local, folder });
                }
            }
        }

        ui.separator();
        self.ui_selection_actions(ui);
    }

    fn ui_selection_actions(&mut self, ui: &mut egui::Ui) {
        match self.state.selected.clone() {
            Some(Selection::Folder(folder)) => {
                ui.label(format!("Carpeta: {}", folder.name));
                if ui.button("Abrir").clicked() {
                    self.state.enter(folder.clone());
                    self.worker.send(Command::List(folder.id.clone()));
                }
                ui.checkbox(&mut self.state.inputs.permanent, "Borrado permanente");
                if ui.button("Eliminar carpeta…").clicked() {
                    self.state.pending_delete = Some(DeleteRequest {
                        remote: folder.id.as_str().to_string(),
                        label: format!("la carpeta '{}'", folder.name),
                        permanent: self.state.inputs.permanent,
                    });
                }
            }
            Some(Selection::File(file)) => {
                let name = file.name.clone().unwrap_or_default();
                ui.label(format!("Archivo: {name}"));

                ui.label("Descargar a (ruta local):");
                let default_name = file.name.clone();
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.state.inputs.download_dest);
                    if ui.button("…").clicked() {
                        let mut dialog = rfd::FileDialog::new();
                        if let Some(name) = &default_name {
                            dialog = dialog.set_file_name(name);
                        }
                        if let Some(path) = dialog.save_file() {
                            self.state.inputs.download_dest = path.display().to_string();
                        }
                    }
                });
                if ui.button("Descargar").clicked() {
                    let dest = self.state.inputs.download_dest.trim().to_string();
                    if !dest.is_empty() {
                        self.worker.send(Command::Download {
                            remote: file.id.as_str().to_string(),
                            dest,
                        });
                    }
                }

                ui.label("Mover a (carpeta remota):");
                ui.text_edit_singleline(&mut self.state.inputs.move_dest);
                if ui.button("Mover").clicked() {
                    let dest = self.state.inputs.move_dest.trim().to_string();
                    if !dest.is_empty() {
                        self.worker.send(Command::Move {
                            source: file.id.as_str().to_string(),
                            dest,
                        });
                    }
                }

                ui.checkbox(&mut self.state.inputs.permanent, "Borrado permanente");
                if ui.button("Eliminar…").clicked() {
                    self.state.pending_delete = Some(DeleteRequest {
                        remote: file.id.as_str().to_string(),
                        label: format!("el archivo '{name}'"),
                        permanent: self.state.inputs.permanent,
                    });
                }
            }
            None => {
                ui.label("(selecciona una carpeta o un archivo)");
            }
        }
    }

    fn ui_trash(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Papelera");
                if ui.button("Refrescar").clicked() {
                    self.worker.send(Command::TrashList);
                }
                if ui
                    .add_enabled(
                        !self.state.trash.is_empty(),
                        egui::Button::new("Vaciar papelera…"),
                    )
                    .clicked()
                {
                    self.state.pending_empty_trash = true;
                }
            });
            ui.separator();

            let entries = self.state.trash.clone();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for entry in &entries {
                    let icon = if entry.kind.as_deref() == Some("folder") {
                        "📁"
                    } else {
                        "📄"
                    };
                    ui.label(format!("{icon} {}    [{}]", entry.name, entry.id));
                }
                if entries.is_empty() {
                    ui.label("(papelera vacía)");
                }
            });
        });
    }

    fn ui_account(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Cuenta");
            match &self.state.status {
                Some(status) if status.authenticated => {
                    ui.label(format!(
                        "Sesión válida: {}",
                        status.identity.as_deref().unwrap_or("(sin identidad)")
                    ));
                }
                Some(_) => {
                    ui.label("Sin sesión. Ve a la pestaña «Sesión» para importar una cookie.");
                }
                None => {
                    ui.label("Cargando…");
                }
            }

            if let Some(quota) = &self.state.quota {
                ui.separator();
                egui::Grid::new("quota").striped(true).show(ui, |ui| {
                    ui.label("Usado");
                    ui.label(crate::util::format_bytes(quota.used));
                    ui.end_row();
                    ui.label("Libre");
                    ui.label(crate::util::format_bytes(quota.free));
                    ui.end_row();
                    ui.label("Cuota");
                    ui.label(if quota.nolimit {
                        "ilimitada".to_string()
                    } else {
                        crate::util::format_bytes(quota.quota)
                    });
                    ui.end_row();
                    ui.label("Papelera");
                    ui.label(crate::util::format_bytes(quota.softdeleted));
                    ui.end_row();
                });
            }
        });
    }

    fn ui_session(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Sesión");

            ui.label("Inicia sesión en O2 Cloud:");
            ui.horizontal(|ui| {
                if cfg!(all(feature = "webview", target_os = "linux"))
                    && ui.button("Iniciar sesión (WebView)").clicked()
                {
                    self.worker.send(Command::LoginWebview);
                }
                if ui.button("Abrir navegador").clicked() {
                    self.worker.send(Command::OpenLoginBrowser);
                }
            });
            ui.label(
                "La WebView abre el portal y captura la sesión automáticamente. \
                 Con «Abrir navegador» inicias sesión en tu navegador y pegas la cookie abajo.",
            );

            ui.separator();
            ui.label("… o pega la cabecera Cookie (JSESSIONID + validationKey) / «Copy as cURL»:");
            ui.add(
                egui::TextEdit::multiline(&mut self.state.inputs.cookie)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                if ui.button("Guardar sesión").clicked() {
                    let cookie = self.state.inputs.cookie.trim().to_string();
                    if !cookie.is_empty() {
                        self.worker.send(Command::SaveSession(cookie));
                    }
                }
                if ui.button("Cerrar sesión (logout)").clicked() {
                    self.worker.send(Command::ClearSession);
                }
            });
        });
    }

    fn ui_confirm_delete(&mut self, ui: &mut egui::Ui) {
        let Some(request) = self.state.pending_delete.clone() else {
            return;
        };
        egui::Window::new("Confirmar borrado")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                let mode = if request.permanent {
                    "permanentemente"
                } else {
                    "a la papelera"
                };
                ui.label(format!("¿Eliminar {} {mode}?", request.label));
                ui.horizontal(|ui| {
                    if ui.button("Cancelar").clicked() {
                        self.state.pending_delete = None;
                    }
                    if ui.button("Eliminar").clicked() {
                        self.worker.send(Command::Remove {
                            remote: request.remote.clone(),
                            permanent: request.permanent,
                        });
                        self.state.pending_delete = None;
                    }
                });
            });
    }

    fn ui_confirm_empty_trash(&mut self, ui: &mut egui::Ui) {
        if !self.state.pending_empty_trash {
            return;
        }
        egui::Window::new("Vaciar papelera")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                ui.label(format!(
                    "¿Eliminar permanentemente {} elemento(s) de la papelera?",
                    self.state.trash.len()
                ));
                ui.horizontal(|ui| {
                    if ui.button("Cancelar").clicked() {
                        self.state.pending_empty_trash = false;
                    }
                    if ui.button("Vaciar").clicked() {
                        self.worker.send(Command::TrashEmpty);
                        self.state.pending_empty_trash = false;
                    }
                });
            });
    }
}

impl eframe::App for O2CloudApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.apply_events();
        if self.refresh_after_action {
            self.refresh_after_action = false;
            self.refresh_view();
        }
        self.ui_top(ui);
        self.ui_bottom(ui);
        match self.state.tab {
            Tab::Browser => self.ui_browser(ui),
            Tab::Trash => self.ui_trash(ui),
            Tab::Account => self.ui_account(ui),
            Tab::Session => self.ui_session(ui),
        }
        self.ui_confirm_delete(ui);
        self.ui_confirm_empty_trash(ui);
    }
}
