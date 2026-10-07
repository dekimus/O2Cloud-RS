//! Librería cliente para O2 Cloud (Telefónica España).
//!
//! Cliente de interoperabilidad **no oficial** que habla el SAPI de Funambol
//! `OneMediaHub` observado en el portal web `cloud.o2online.es`. No existe una
//! API pública; el contrato se ha reverse-ingeniado y está documentado en
//! `docs/protocol.md`.
//!
//! La librería está pensada para ser reutilizada por distintas interfaces
//! (CLI, TUI, FUSE, servicios de sincronización): expone un
//! [`O2CloudClient`] de alto nivel, modelos tipados y un modelo de errores
//! estable, sin acoplar nada a la CLI.

#![warn(
    clippy::all,
    clippy::pedantic,
    rust_2018_idioms,
    missing_debug_implementations
)]
// `pedantic` produce avisos muy ruidosos que no aportan valor a este primer
// slice; se permite un conjunto reducido y justificado.
#![allow(
    clippy::module_name_repetitions,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::struct_excessive_bools
)]

pub mod api;
pub mod auth;
pub mod cli;
pub mod config;
pub mod error;
#[cfg(feature = "gui")]
pub mod gui;
pub mod output;
pub mod progress;
pub mod service;
pub mod util;

pub use api::client::O2CloudClient;
pub use api::models::{Folder, Id, MediaItem, MediaListing, Profile, Quota};
pub use auth::session::Session;
pub use config::AppConfig;
pub use error::{Error, Result};
pub use progress::ProgressReporter;
