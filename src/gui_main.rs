//! Punto de entrada de la GUI de O2 Cloud (feature `gui`).

fn main() -> eframe::Result<()> {
    // Logs a stderr (configurables con `RUST_LOG`, p. ej. `o2cloud=debug`).
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("o2cloud=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    o2cloud::gui::run()
}
