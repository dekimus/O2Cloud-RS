//! Renderizado de resultados: JSON (stdout) y humano.

use serde::Serialize;
use serde_json::json;

use crate::api::models::{MediaItem, Quota};
use crate::error::Error;
use crate::service::{DownloadOutcome, Listing, StatusInfo, Tree, TreeNode, UploadOutcome};
use crate::util::format_bytes;

/// Renderizador de salida según el modo (`--json` o humano).
///
/// En modo JSON, stdout contiene exclusivamente JSON válido; los logs y el
/// progreso van a stderr. En modo humano, los resultados van a stdout y los
/// errores a stderr.
#[derive(Debug, Clone, Copy)]
pub struct Output {
    json: bool,
}

impl Output {
    /// Crea un renderizador.
    #[must_use]
    pub fn new(json: bool) -> Self {
        Self { json }
    }

    /// ¿Está en modo JSON?
    #[must_use]
    pub fn is_json(self) -> bool {
        self.json
    }

    /// Imprime un resultado con éxito. En modo JSON emite `{"ok":true,...}`.
    pub fn success<T: Serialize>(self, data: &T) {
        if self.json {
            println!("{}", json!({ "ok": true, "data": data }));
        }
    }

    /// Imprime un error. En modo JSON emite `{"ok":false,...}` en stdout;
    /// en modo humano escribe a stderr.
    pub fn error(self, err: &Error) {
        if self.json {
            println!(
                "{}",
                json!({ "ok": false, "error": { "type": err.kind(), "message": err.to_string() } })
            );
        } else {
            eprintln!("Error: {err}");
        }
    }
}

/// Imprime la cuota en modo humano.
pub fn print_quota(quota: &Quota) {
    println!("Almacenamiento:");
    println!("  Usado:        {}", format_bytes(quota.used));
    println!("  Libre:        {}", format_bytes(quota.free));
    if quota.nolimit {
        println!("  Cuota:        ilimitada");
    } else {
        println!("  Cuota:        {}", format_bytes(quota.quota));
    }
    println!("  Papelera:     {}", format_bytes(quota.softdeleted));
    if let Some(individual) = &quota.individual {
        println!(
            "  Individual:   {} (papelera {})",
            format_bytes(individual.used),
            format_bytes(individual.softdeleted)
        );
    }
}

/// Imprime un listado en modo humano.
pub fn print_listing(listing: &Listing) {
    if listing.folders.is_empty() && listing.files.is_empty() {
        println!("(vacío)");
        return;
    }
    for folder in &listing.folders {
        println!("{:<16}  📁 {}/", folder.id, folder.name);
    }
    for file in &listing.files {
        let name = file.name.as_deref().unwrap_or("?");
        let size = file.size.map_or_else(|| "-".to_string(), format_bytes);
        println!("{:<16}  {:>10}  {}", file.id, size, name);
    }
}

/// Imprime el estado de sesión en modo humano.
pub fn print_status(status: &StatusInfo) {
    if status.authenticated {
        match &status.identity {
            Some(identity) => println!("Conectado como {identity}"),
            None => println!("Conectado (sesión válida)"),
        }
    } else {
        println!("No conectado. Ejecuta `o2cloud login --import-cookie '<Cookie: ...>'`.");
    }
}

/// Imprime el detalle de un archivo en modo humano.
pub fn print_media(item: &MediaItem) {
    println!("id:               {}", item.id);
    println!("nombre:           {}", item.name.as_deref().unwrap_or("?"));
    if let Some(size) = item.size {
        println!("tamaño:           {} ({size} bytes)", format_bytes(size));
    }
    if let Some(mediatype) = &item.mediatype {
        println!("tipo:             {mediatype}");
    }
    if let Some(date) = item.date {
        println!("fecha:            {date} (epoch-ms)");
    }
    if let Some(etag) = &item.etag {
        println!("etag:             {etag}");
    }
    if let Some(status) = &item.status {
        println!("estado:           {status}");
    }
}

/// Imprime el resultado de una o varias subidas en modo humano.
pub fn print_uploads(outcomes: &[UploadOutcome]) {
    for outcome in outcomes {
        let validation = if outcome.validated {
            "disponible".to_string()
        } else {
            match &outcome.validation_status {
                Some(status) => format!("validación pendiente ({status})"),
                None => "validación pendiente".to_string(),
            }
        };
        println!(
            "✓ {} ({}) → id {} [{}]",
            outcome.name,
            format_bytes(i64::try_from(outcome.size).unwrap_or(i64::MAX)),
            outcome.id,
            validation
        );
    }
}

/// Imprime el resultado de una o varias descargas en modo humano.
pub fn print_downloads(outcomes: &[DownloadOutcome]) {
    for outcome in outcomes {
        println!(
            "✓ {} → {} ({})",
            outcome.name,
            outcome.path,
            format_bytes(i64::try_from(outcome.size).unwrap_or(i64::MAX))
        );
    }
}

/// Imprime el árbol de carpetas en modo humano.
pub fn print_tree(tree: &Tree) {
    if tree.root.name == "/" {
        println!("📁 /");
    } else {
        println!("📁 {}/", tree.root.name);
    }
    print_tree_children(&tree.root.children, "");
    println!();
    println!("({} carpetas)", tree.folder_count);
    if !tree.hierarchical {
        println!("(nota: el servidor no devolvió jerarquía; lista plana)");
    }
}

fn print_tree_children(children: &[TreeNode], prefix: &str) {
    let count = children.len();
    for (index, child) in children.iter().enumerate() {
        let last = index + 1 == count;
        let connector = if last { "└── " } else { "├── " };
        println!("{prefix}{connector}📁 {}/", child.name);
        let child_prefix = format!("{prefix}{}", if last { "    " } else { "│   " });
        print_tree_children(&child.children, &child_prefix);
    }
}
