//! Definición de la interfaz de línea de comandos (clap).

use clap::{Parser, Subcommand};

/// Cliente Linux no oficial para O2 Cloud (Telefónica España).
#[derive(Debug, Parser)]
#[command(name = "o2cloud", version, about)]
pub struct Cli {
    /// Salida JSON exclusiva en stdout (logs a stderr).
    #[arg(long, global = true)]
    pub json: bool,

    /// Más detalle en los logs (stderr).
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Silenciar logs no esenciales.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Perfil de configuración (namespaca el keyring).
    #[arg(long, global = true)]
    pub profile: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

/// Comandos disponibles.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Inicia sesión importando una cookie capturada en el navegador.
    Login {
        /// Cabecera `Cookie` completa (contiene JSESSIONID y validationKey).
        #[arg(long)]
        import_cookie: Option<String>,

        /// Solo el validationKey (insuficiente sin JSESSIONID).
        #[arg(long)]
        import_token: Option<String>,
    },

    /// Cierra sesión (borra la cookie del keyring).
    Logout,

    /// Muestra el estado de la sesión.
    Status,

    /// Muestra la cuota de almacenamiento.
    Quota,

    /// Lista carpetas y archivos (por defecto, la raíz).
    Ls {
        /// Ruta remota (p. ej. `/fotos`).
        remote: Option<String>,
    },

    /// Muestra el detalle de un archivo remoto (id o ruta).
    Stat {
        /// Id numérico o ruta del archivo.
        remote: String,
    },

    /// Sube uno o varios archivos locales a una carpeta remota.
    Upload {
        /// Rutas locales a subir.
        locals: Vec<String>,

        /// Carpeta remota destino.
        #[arg(long, default_value = "/")]
        to: String,
    },
}
