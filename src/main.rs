//! Binario CLI.

use clap::Parser;
use o2cloud::auth::{
    self, BrowserOpener, KeyringStore, SecretStore, SystemBrowserOpener, INSTRUCTIONS, LOGIN_URL,
};
use o2cloud::cli::{Cli, Command};
use o2cloud::config::AppConfig;
use o2cloud::error::{Error, Result};
use o2cloud::output::{
    print_downloads, print_listing, print_media, print_quota, print_status, print_tree,
    print_uploads, Output,
};
use o2cloud::{service, O2CloudClient, Session};
use serde_json::json;
use std::path::Path;

fn main() {
    let cli = Cli::parse();
    init_logging(&cli);
    let output = Output::new(cli.json);

    // El login con WebView embebida necesita el bucle de GTK en el hilo
    // principal, así que se ejecuta antes de construir el runtime async.
    if let Some(code) = maybe_webview_login(&cli, output) {
        std::process::exit(code);
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    let code = match runtime.block_on(run(&cli, output)) {
        Ok(()) => 0,
        Err(err) => {
            output.error(&err);
            err.exit_code()
        }
    };
    std::process::exit(code);
}

/// ¿Es un `login` sin opciones (candidato a WebView embebida)?
#[allow(dead_code)]
fn is_bare_login(cli: &Cli) -> bool {
    matches!(
        &cli.command,
        Command::Login {
            import_cookie: None,
            import_token: None
        }
    )
}

/// Ejecuta el login con WebView si la feature está activa (Linux).
///
/// Devuelve `Some(exit_code)` si gestionó el login; `None` para que el flujo
/// normal (navegador del sistema) se encargue de él.
#[cfg(all(feature = "webview", target_os = "linux"))]
fn maybe_webview_login(cli: &Cli, output: Output) -> Option<i32> {
    use o2cloud::auth::webview::{capture_cookie_header, CaptureOutcome, TROUBLESHOOTING};

    if !is_bare_login(cli) {
        return None;
    }
    let config = AppConfig::new(cli.profile.clone());

    match capture_cookie_header() {
        CaptureOutcome::Captured(cookie) => {
            Some(report(store_and_report(&config, output, &cookie), output))
        }
        CaptureOutcome::Cancelled => {
            eprintln!("Login cancelado.");
            Some(11)
        }
        CaptureOutcome::Unavailable(reason) => {
            eprintln!("No se pudo usar la WebView embebida: {reason}");
            eprintln!("{TROUBLESHOOTING}");
            eprintln!("Se usará el login con el navegador del sistema.");
            Some(report(browser_login(&config, output), output))
        }
    }
}

/// Convierte el resultado de una operación en un código de salida.
#[cfg(all(feature = "webview", target_os = "linux"))]
fn report(result: Result<()>, output: Output) -> i32 {
    match result {
        Ok(()) => 0,
        Err(err) => {
            output.error(&err);
            err.exit_code()
        }
    }
}

/// Sin la feature `webview`, el login normal usa el navegador del sistema.
#[cfg(not(all(feature = "webview", target_os = "linux")))]
fn maybe_webview_login(_cli: &Cli, _output: Output) -> Option<i32> {
    None
}

/// Inicializa el logging (stderr). Respeta `RUST_LOG` si está definida.
fn init_logging(cli: &Cli) {
    let level = if cli.quiet {
        "error"
    } else if cli.verbose {
        "debug"
    } else {
        "info"
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(format!("o2cloud={level}")));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

async fn run(cli: &Cli, output: Output) -> Result<()> {
    let config = AppConfig::new(cli.profile.clone());

    match &cli.command {
        Command::Login {
            import_cookie,
            import_token,
        } => login(
            &config,
            output,
            import_cookie.as_deref(),
            import_token.as_deref(),
        ),

        Command::Logout => logout(&config, output),

        Command::Status => {
            let client = build_client(&config)?;
            let info = service::status(&client).await?;
            output.success(&info);
            if !output.is_json() {
                print_status(&info);
            }
            Ok(())
        }

        Command::Quota => {
            let client = build_client(&config)?;
            require_session(&client)?;
            let quota = service::quota(&client).await?;
            output.success(&quota);
            if !output.is_json() {
                print_quota(&quota);
            }
            Ok(())
        }

        Command::Ls { remote } => {
            let client = build_client(&config)?;
            require_session(&client)?;
            let remote = remote.as_deref().unwrap_or("/");
            let listing = service::list(&client, remote).await?;
            output.success(&listing);
            if !output.is_json() {
                print_listing(&listing);
            }
            Ok(())
        }

        Command::Tree { remote } => {
            let client = build_client(&config)?;
            require_session(&client)?;
            let remote = remote.as_deref().unwrap_or("/");
            let tree = service::tree(&client, remote).await?;
            output.success(&tree);
            if !output.is_json() {
                print_tree(&tree);
            }
            Ok(())
        }

        Command::Stat { remote } => {
            let client = build_client(&config)?;
            require_session(&client)?;
            let item = service::stat(&client, remote).await?;
            output.success(&item);
            if !output.is_json() {
                print_media(&item);
            }
            Ok(())
        }

        Command::Upload { locals, to } => {
            let client = build_client(&config)?;
            require_session(&client)?;

            if locals.is_empty() {
                return Err(Error::Protocol("no local files given".to_string()));
            }
            let folder_id = service::resolve_folder_id(&client, to).await?;
            let options = service::UploadOptions {
                show_progress: show_progress(cli, output),
                ..Default::default()
            };
            let mut outcomes = Vec::with_capacity(locals.len());
            for local in locals {
                let outcome =
                    service::upload(&client, Path::new(local), &folder_id, &options).await?;
                outcomes.push(outcome);
            }
            output.success(&outcomes);
            if !output.is_json() {
                print_uploads(&outcomes);
            }
            Ok(())
        }

        Command::Download { remotes, to, force } => {
            let client = build_client(&config)?;
            require_session(&client)?;

            if remotes.is_empty() {
                return Err(Error::Protocol("no remote files given".to_string()));
            }
            let multiple = remotes.len() > 1;
            let options = service::DownloadOptions {
                show_progress: show_progress(cli, output),
                force: *force,
            };
            let mut outcomes = Vec::with_capacity(remotes.len());
            for remote in remotes {
                let outcome =
                    service::download(&client, remote, Path::new(to), multiple, &options).await?;
                outcomes.push(outcome);
            }
            output.success(&outcomes);
            if !output.is_json() {
                print_downloads(&outcomes);
            }
            Ok(())
        }

        Command::Mkdir { remote } => {
            let client = build_client(&config)?;
            require_session(&client)?;
            let created = service::mkdir(&client, remote).await?;
            output.success(&created);
            if !output.is_json() {
                println!("✓ Carpeta creada: {} (id {})", created.name, created.id);
            }
            Ok(())
        }

        Command::Cp { source, dest } => {
            let client = build_client(&config)?;
            require_session(&client)?;
            let outcome = service::copy(&client, source, dest).await?;
            output.success(&outcome);
            if !output.is_json() {
                println!("✓ Copiado {} → {}", outcome.source, outcome.dest);
            }
            Ok(())
        }

        Command::Mv { source, dest } => {
            let client = build_client(&config)?;
            require_session(&client)?;
            let outcome = service::move_item(&client, source, dest).await?;
            output.success(&outcome);
            if !output.is_json() {
                println!("✓ Movido {} → {}", outcome.source, outcome.dest);
            }
            Ok(())
        }

        Command::Rm {
            remote,
            permanent,
            yes,
        } => {
            let client = build_client(&config)?;
            require_session(&client)?;

            let target = service::resolve_removable(&client, remote).await?;
            ensure_confirmation(*yes, output.is_json(), &target.label())?;
            service::remove(&client, &target, *permanent).await?;

            let message = json!({
                "name": target.name(),
                "id": target.id().as_str(),
                "permanent": permanent,
            });
            output.success(&message);
            if !output.is_json() {
                let mode = if *permanent {
                    "permanentemente"
                } else {
                    "a la papelera"
                };
                println!("✓ Eliminado {} ({mode}).", target.name());
            }
            Ok(())
        }
    }
}

/// Exige confirmación para una operación destructiva.
///
/// `--yes` la salta. En modo `--json` o sin TTY no se pregunta: se exige
/// `--yes` explícito. Si el usuario rechaza, la operación se cancela.
fn ensure_confirmation(yes: bool, json: bool, label: &str) -> Result<()> {
    if yes {
        return Ok(());
    }

    use std::io::{IsTerminal, Write};
    if json || !std::io::stdin().is_terminal() {
        return Err(Error::Protocol(format!(
            "refusing to delete {label} without confirmation; re-run with --yes"
        )));
    }

    eprint!("¿Eliminar {label}? [y/N] ");
    std::io::stderr()
        .flush()
        .map_err(|e| Error::LocalIo(e.to_string()))?;

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| Error::LocalIo(e.to_string()))?;

    if matches!(input.trim(), "y" | "Y" | "yes" | "YES" | "Yes") {
        Ok(())
    } else {
        eprintln!("Cancelado.");
        Err(Error::Cancelled)
    }
}

/// ¿Mostrar barra de progreso? No en modo `--json`/`--quiet` ni sin TTY.
fn show_progress(cli: &Cli, output: Output) -> bool {
    use std::io::IsTerminal;
    !cli.quiet && !output.is_json() && std::io::stderr().is_terminal()
}

/// Construye un cliente cargando la sesión (env `O2CLOUD_COOKIE` o keyring).
fn build_client(config: &AppConfig) -> Result<O2CloudClient> {
    let session = load_session(config)?;
    O2CloudClient::new(config, session)
}

/// Carga la sesión: primero la variable de entorno `O2CLOUD_COOKIE` (testing/
/// automatización), después el keyring.
fn load_session(config: &AppConfig) -> Result<Option<Session>> {
    if let Ok(cookie) = std::env::var("O2CLOUD_COOKIE") {
        let cookie = cookie.trim();
        if !cookie.is_empty() {
            return Ok(Some(Session::from_cookie_header(cookie)?));
        }
    }
    let store = KeyringStore::new(&config.keyring_service())?;
    match store.get_cookie()? {
        Some(cookie) => Ok(Some(Session::from_cookie_header(&cookie)?)),
        None => Ok(None),
    }
}

/// Exige una sesión para operaciones que la requieren.
fn require_session(client: &O2CloudClient) -> Result<()> {
    if client.session().is_none() {
        return Err(Error::Authentication(
            "not authenticated — run 'o2cloud login --import-cookie' to import a session"
                .to_string(),
        ));
    }
    Ok(())
}

fn login(
    config: &AppConfig,
    output: Output,
    import_cookie: Option<&str>,
    import_token: Option<&str>,
) -> Result<()> {
    match (import_cookie, import_token) {
        (Some(cookie), _) => store_and_report(config, output, cookie),
        (None, Some(token)) => {
            let cookie = format!("validationKey={token}");
            store_and_report(config, output, &cookie)
        }
        (None, None) => browser_login(config, output),
    }
}

/// Login asistido por navegador: abre el portal y captura la cookie pegada.
fn browser_login(config: &AppConfig, output: Output) -> Result<()> {
    use std::io::{IsTerminal, Write};

    if !std::io::stdin().is_terminal() {
        return Err(Error::Authentication(
            "no interactive terminal — use 'o2cloud login --import-cookie' instead".to_string(),
        ));
    }

    match SystemBrowserOpener.open(LOGIN_URL) {
        Ok(()) => eprintln!("Se ha abierto el navegador en {LOGIN_URL}"),
        Err(e) => {
            eprintln!("No se pudo abrir el navegador ({e}).");
            eprintln!("Abre manualmente: {LOGIN_URL}");
        }
    }
    eprintln!();
    eprintln!("{INSTRUCTIONS}");

    eprint!("\nPega la cookie: ");
    std::io::stderr()
        .flush()
        .map_err(|e| Error::LocalIo(e.to_string()))?;

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| Error::LocalIo(e.to_string()))?;

    let cookie = input.trim();
    if cookie.is_empty() {
        return Err(Error::Cancelled);
    }
    store_and_report(config, output, cookie)
}

/// Valida, guarda en el keyring y reporta una sesión importada.
fn store_and_report(config: &AppConfig, output: Output, cookie: &str) -> Result<()> {
    let store = KeyringStore::new(&config.keyring_service())?;
    let session = auth::save_session(&store, cookie)?;

    if !session.has_jsession_id() {
        eprintln!("Aviso: no se detectó JSESSIONID en la cookie; la sesión podría fallar con 401.");
    }

    if output.is_json() {
        output.success(&json!({ "message": "session stored in keyring" }));
    } else {
        println!("Sesión guardada en el keyring. Comprueba con `o2cloud status`.");
    }
    Ok(())
}

fn logout(config: &AppConfig, output: Output) -> Result<()> {
    let store = KeyringStore::new(&config.keyring_service())?;
    store.clear()?;

    if output.is_json() {
        output.success(&json!({ "message": "logged out" }));
    } else {
        println!("Sesión eliminada.");
    }
    Ok(())
}
