//! Modelo de errores tipado del cliente.

use thiserror::Error;

use crate::util::redact_secrets;

/// Error del cliente O2 Cloud.
///
/// Distingue explícitamente entre fallos de autenticación, error HTTP, error
/// SAPI (que puede llegar dentro de un HTTP 200), error de protocolo y fallos
/// locales, tal como exige el modelo de errores del proyecto.
#[derive(Debug, Error)]
pub enum Error {
    /// No hay sesión local configurada (no se ha hecho `login`).
    #[error("authentication required: {0}")]
    Authentication(String),

    /// El servidor rechazó la sesión (HTTP 401/403 o código SAPI SEC/AUT).
    #[error("unauthorized (session expired or invalid): {0}")]
    Unauthorized(String),

    /// El recurso remoto no existe.
    #[error("not found: {0}")]
    NotFound(String),

    /// Conflicto (p. ej. ya existe un recurso con el mismo nombre).
    #[error("conflict: {0}")]
    Conflict(String),

    /// Se ha superado la cuota de almacenamiento.
    #[error("quota exceeded: {0}")]
    QuotaExceeded(String),

    /// El servidor aplicó limitación de tasa (HTTP 429).
    #[error("rate limited: {0}")]
    RateLimited(String),

    /// Fallo de transporte (conexión, timeout, DNS, TLS).
    #[error("transport error: {0}")]
    Transport(String),

    /// Respuesta HTTP con un estado inesperado para la operación.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// Error del servidor (HTTP 5xx).
    #[error("server error: {0}")]
    Server(String),

    /// Error SAPI devuelto en un envelope (posiblemente con HTTP 200).
    #[error("SAPI error {code}: {message}")]
    Sapi {
        /// Código de error SAPI (p. ej. `MED-1000`).
        code: String,
        /// Mensaje humano del error.
        message: String,
    },

    /// La respuesta no pudo interpretarse (JSON no válido o forma inesperada).
    #[error("invalid response: {0}")]
    InvalidResponse(String),

    /// Fallo de entrada/salida local.
    #[error("local I/O error: {0}")]
    LocalIo(String),

    /// La operación fue cancelada.
    #[error("cancelled")]
    Cancelled,
}

/// Alias de `Result` con el error del cliente.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Código de salida estable asociado a cada variante de error.
    ///
    /// Es parte del contrato para scripts: nunca debe cambiar.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Authentication(_) | Self::Unauthorized(_) => 2,
            Self::NotFound(_) => 3,
            Self::Conflict(_) => 4,
            Self::Transport(_) => 5,
            Self::RateLimited(_) => 6,
            Self::QuotaExceeded(_) => 7,
            Self::Server(_) | Self::Sapi { .. } => 8,
            Self::Protocol(_) | Self::InvalidResponse(_) => 9,
            Self::LocalIo(_) => 10,
            Self::Cancelled => 11,
        }
    }

    /// Identificador corto y estable del tipo de error (para salida JSON).
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Authentication(_) => "authentication",
            Self::Unauthorized(_) => "unauthorized",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::QuotaExceeded(_) => "quota_exceeded",
            Self::RateLimited(_) => "rate_limited",
            Self::Transport(_) => "transport",
            Self::Protocol(_) => "protocol",
            Self::Server(_) => "server",
            Self::Sapi { .. } => "sapi",
            Self::InvalidResponse(_) => "invalid_response",
            Self::LocalIo(_) => "local_io",
            Self::Cancelled => "cancelled",
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        // El mensaje de `reqwest` puede incluir la URL completa, que contiene
        // `?validationkey=<secreto>`. Redactamos antes de propagar.
        let msg = redact_secrets(&e.to_string());
        if e.is_timeout() {
            Self::Transport(format!("timeout: {msg}"))
        } else if e.is_connect() {
            Self::Transport(format!("connection failed: {msg}"))
        } else {
            Self::Transport(msg)
        }
    }
}
