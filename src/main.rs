//! Binario CLI.

use clap::Parser;
use o2cloud::auth::{KeyringStore, SecretStore};
use o2cloud::cli::{Cli, Command};
use o2cloud::config::AppConfig;
use o2cloud::error::{Error, Result};
use o2cloud::output::{
    print_listing, print_media, print_quota, print_status, print_uploads, Output,
};
use o2cloud::{service, O2CloudClient, Session};
use serde_json::json;
use std::path::Path;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    init_logging(&cli);
    let output = Output::new(cli.json);

    let code = match run(&cli, output).await {
        Ok(()) => 0,
        Err(err) => {
            output.error(&err);
            err.exit_code()
        }
    };
    std::process::exit(code);
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
    let cookie = match (import_cookie, import_token) {
        (Some(c), _) => c.to_string(),
        (None, Some(t)) => format!("validationKey={t}"),
        (None, None) => {
            return Err(Error::Authentication(
                "provide --import-cookie or --import-token".to_string(),
            ));
        }
    };

    let session = Session::from_cookie_header(&cookie)?;
    if !session.has_jsession_id() {
        eprintln!("Aviso: no se detectó JSESSIONID en la cookie; la sesión podría fallar con 401.");
    }

    let store = KeyringStore::new(&config.keyring_service())?;
    store.set_cookie(&cookie)?;

    if output.is_json() {
        output.success(&json!({ "message": "session stored in keyring" }));
    } else {
        println!("Sesión guardada en el keyring.");
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
