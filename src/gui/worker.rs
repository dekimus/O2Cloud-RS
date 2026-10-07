//! Puente entre la UI (síncrona) y la capa de servicio async (tokio).
//!
//! La UI envía [`Command`]s y recibe [`Event`]s; un hilo con su propio runtime
//! de tokio ejecuta las operaciones y despierta a la UI con `request_repaint`.

use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;

use crate::api::models::{Folder, Id, Quota};
use crate::auth::{self, KeyringStore, SecretStore};
use crate::config::AppConfig;
use crate::error::Result;
use crate::progress::{ProgressEvent, ProgressReporter};
use crate::service::{self, DownloadOptions, StatusInfo, UploadOptions};
use crate::O2CloudClient;

/// Operación solicitada por la UI.
#[derive(Debug)]
pub enum Command {
    /// Inicializa: sesión, cuota y listado de la raíz.
    Init,
    /// Refresca estado de sesión y cuota.
    Refresh,
    /// Lista una carpeta por id.
    List(Id),
    /// Crea una carpeta en la ruta remota indicada.
    Mkdir(String),
    /// Sube un archivo local a una carpeta remota.
    Upload { local: String, folder: Id },
    /// Descarga un archivo remoto a una ruta local.
    Download { remote: String, dest: String },
    /// Mueve un elemento a una carpeta destino.
    Move { source: String, dest: String },
    /// Elimina un elemento (papelera o permanente).
    Remove { remote: String, permanent: bool },
    /// Guarda una cookie de sesión (login).
    SaveSession(String),
    /// Borra la sesión (logout).
    ClearSession,
}

/// Resultado enviado por el worker a la UI.
#[derive(Debug)]
pub enum Event {
    /// Hay (o no) una operación en curso.
    Busy(bool),
    /// Carpeta raíz.
    Root(Folder),
    /// Estado de la sesión.
    Status(StatusInfo),
    /// Cuota.
    Quota(Quota),
    /// Contenido de una carpeta.
    Listing(service::Listing),
    /// Progreso de una transferencia.
    Progress(ProgressEvent),
    /// Mensaje de éxito.
    Message(String),
    /// Error.
    Error(String),
}

/// Worker async.
#[derive(Debug)]
pub struct Worker {
    tx: Sender<Command>,
    rx: Receiver<Event>,
    _handle: JoinHandle<()>,
}

impl Worker {
    /// Lanza el worker; `ctx` se usa para despertar a la UI al llegar eventos.
    #[must_use]
    pub fn spawn(ctx: egui::Context) -> Self {
        let (cmd_tx, cmd_rx) = channel();
        let (evt_tx, evt_rx) = channel();
        let handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime");
            while let Ok(command) = cmd_rx.recv() {
                emit(&evt_tx, &ctx, Event::Busy(true));
                match runtime.block_on(run_command(command, &evt_tx, &ctx)) {
                    Ok(events) => {
                        for event in events {
                            emit(&evt_tx, &ctx, event);
                        }
                    }
                    Err(err) => emit(&evt_tx, &ctx, Event::Error(err.to_string())),
                }
                emit(&evt_tx, &ctx, Event::Busy(false));
            }
        });
        Self {
            tx: cmd_tx,
            rx: evt_rx,
            _handle: handle,
        }
    }

    /// Encola un comando.
    pub fn send(&self, command: Command) {
        let _ = self.tx.send(command);
    }

    /// Recoge los eventos pendientes.
    #[must_use]
    pub fn drain(&self) -> Vec<Event> {
        self.rx.try_iter().collect()
    }
}

fn emit(tx: &Sender<Event>, ctx: &egui::Context, event: Event) {
    let _ = tx.send(event);
    ctx.request_repaint();
}

/// Reportero de progreso que reenvía cada evento a la UI.
fn progress_reporter(tx: &Sender<Event>, ctx: &egui::Context) -> ProgressReporter {
    let tx = tx.clone();
    let ctx = ctx.clone();
    ProgressReporter::callback(move |event| {
        let _ = tx.send(Event::Progress(event));
        ctx.request_repaint();
    })
}

/// Construye un cliente con la sesión almacenada (puede no haber sesión).
fn make_client() -> Result<O2CloudClient> {
    let config = AppConfig::new(None);
    let session = auth::load_stored_session(&config)?;
    O2CloudClient::new(&config, session)
}

fn keyring() -> Result<KeyringStore> {
    KeyringStore::new(&AppConfig::new(None).keyring_service())
}

async fn run_command(
    command: Command,
    tx: &Sender<Event>,
    ctx: &egui::Context,
) -> Result<Vec<Event>> {
    match command {
        Command::SaveSession(cookie) => {
            let session = auth::save_session(&keyring()?, &cookie)?;
            let note = if session.has_jsession_id() {
                "Sesión guardada."
            } else {
                "Sesión guardada (sin JSESSIONID; podría fallar)."
            };
            Ok(vec![Event::Message(note.to_string())])
        }
        Command::ClearSession => {
            keyring()?.clear()?;
            Ok(vec![Event::Message("Sesión eliminada.".to_string())])
        }
        other => {
            let client = make_client()?;
            run_client_command(&client, other, tx, ctx).await
        }
    }
}

async fn run_client_command(
    client: &O2CloudClient,
    command: Command,
    tx: &Sender<Event>,
    ctx: &egui::Context,
) -> Result<Vec<Event>> {
    match command {
        Command::Init => {
            let status = service::status(client).await?;
            if !status.authenticated {
                return Ok(vec![Event::Status(status)]);
            }
            let root = service::root_folder(client).await?;
            let quota = service::quota(client).await?;
            let listing = service::list_folder(client, &root.id).await?;
            Ok(vec![
                Event::Root(root),
                Event::Status(status),
                Event::Quota(quota),
                Event::Listing(listing),
            ])
        }
        Command::Refresh => {
            let status = service::status(client).await?;
            let mut events = Vec::new();
            let authenticated = status.authenticated;
            events.push(Event::Status(status));
            if authenticated {
                events.push(Event::Quota(service::quota(client).await?));
            }
            Ok(events)
        }
        Command::List(folder) => {
            let listing = service::list_folder(client, &folder).await?;
            Ok(vec![Event::Listing(listing)])
        }
        Command::Mkdir(path) => {
            let created = service::mkdir(client, &path).await?;
            Ok(vec![Event::Message(format!(
                "Carpeta '{}' creada (id {})",
                created.name, created.id
            ))])
        }
        Command::Upload { local, folder } => {
            let outcome = service::upload(
                client,
                Path::new(&local),
                &folder,
                &UploadOptions {
                    reporter: Some(progress_reporter(tx, ctx)),
                    ..Default::default()
                },
            )
            .await?;
            Ok(vec![Event::Message(format!(
                "Subido '{}' ({} bytes)",
                outcome.name, outcome.size
            ))])
        }
        Command::Download { remote, dest } => {
            let outcome = service::download(
                client,
                &remote,
                Path::new(&dest),
                false,
                &DownloadOptions {
                    reporter: Some(progress_reporter(tx, ctx)),
                    ..Default::default()
                },
            )
            .await?;
            Ok(vec![Event::Message(format!(
                "Descargado '{}' → {}",
                outcome.name, outcome.path
            ))])
        }
        Command::Move { source, dest } => {
            let outcome = service::move_item(client, &source, &dest).await?;
            Ok(vec![Event::Message(format!(
                "Movido {} → {}",
                outcome.source, outcome.dest
            ))])
        }
        Command::Remove { remote, permanent } => {
            let target = service::resolve_removable(client, &remote).await?;
            service::remove(client, &target, permanent).await?;
            Ok(vec![Event::Message(format!("Eliminado {}", target.name()))])
        }
        Command::SaveSession(_) | Command::ClearSession => Ok(Vec::new()),
    }
}
