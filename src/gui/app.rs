//! Aplicación de escritorio (eframe/egui).

use eframe::egui;

use crate::gui::state::{AppState, DeleteRequest, Tab};
use crate::gui::worker::{Command, Event, Worker};

/// Aplicación GUI.
#[derive(Debug)]
pub struct O2CloudApp {
    state: AppState,
    worker: Worker,
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
        }
    }

    fn apply_events(&mut self) {
        for event in self.worker.drain() {
            match event {
                Event::Busy(busy) => self.state.busy = busy,
                Event::Root(folder) => self.state.set_root(folder),
                Event::Status(status) => self.state.status = Some(status),
                Event::Quota(quota) => self.state.quota = Some(quota),
                Event::Listing(listing) => {
                    self.state.folders = listing.folders;
                    self.state.files = listing.files;
                }
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
            });
            ui.separator();

            let folders = self.state.folders.clone();
            let files = self.state.files.clone();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for folder in &folders {
                    if ui
                        .selectable_label(false, format!("📁 {}/", folder.name))
                        .clicked()
                    {
                        self.state.enter(folder.clone());
                        self.worker.send(Command::List(folder.id.clone()));
                    }
                }
                for file in &files {
                    let name = file.name.as_deref().unwrap_or("?");
                    let size = file
                        .size
                        .map_or_else(String::new, crate::util::format_bytes);
                    let selected = self.state.selected_file.as_deref() == Some(file.id.as_str());
                    let label = format!("📄 {name}    {size}    [{}]", file.id);
                    if ui.selectable_label(selected, label).clicked() {
                        self.state.selected_file = Some(file.id.as_str().to_string());
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
        ui.text_edit_singleline(&mut self.state.inputs.upload_local);
        if ui.button("Subir").clicked() {
            if let Some(folder) = self.state.current_id().cloned() {
                let local = self.state.inputs.upload_local.trim().to_string();
                if !local.is_empty() {
                    self.worker.send(Command::Upload { local, folder });
                }
            }
        }

        ui.separator();
        if let Some(selected) = self.state.selected_file.clone() {
            let name = self.state.selected_name().unwrap_or("?").to_string();
            ui.label(format!("Seleccionado: {name}"));

            ui.label("Descargar a (ruta local):");
            ui.text_edit_singleline(&mut self.state.inputs.download_dest);
            if ui.button("Descargar").clicked() {
                let dest = self.state.inputs.download_dest.trim().to_string();
                if !dest.is_empty() {
                    self.worker.send(Command::Download {
                        remote: selected.clone(),
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
                        source: selected.clone(),
                        dest,
                    });
                }
            }

            ui.checkbox(&mut self.state.inputs.permanent, "Borrado permanente");
            if ui.button("Eliminar…").clicked() {
                self.state.pending_delete = Some(DeleteRequest {
                    remote: selected,
                    label: name,
                    permanent: self.state.inputs.permanent,
                });
            }
        } else {
            ui.label("(selecciona un archivo)");
        }
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
            ui.label("Pega la cabecera Cookie (JSESSIONID + validationKey) o el «Copy as cURL»:");
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
                ui.label(format!("¿Eliminar «{}» {mode}?", request.label));
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
}

impl eframe::App for O2CloudApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.apply_events();
        self.ui_top(ui);
        self.ui_bottom(ui);
        match self.state.tab {
            Tab::Browser => self.ui_browser(ui),
            Tab::Account => self.ui_account(ui),
            Tab::Session => self.ui_session(ui),
        }
        self.ui_confirm_delete(ui);
    }
}
