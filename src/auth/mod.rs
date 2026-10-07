//! Autenticación: sesión SAPI y almacenamiento seguro.

pub mod session;
pub mod store;

pub use session::Session;
pub use store::{EnvStore, KeyringStore, MemoryStore, SecretStore};
