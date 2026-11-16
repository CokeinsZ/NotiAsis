use tracing_subscriber::EnvFilter;

/// Logs a stdout. Nivel con `RUST_LOG` (por defecto `info`; `debug` muestra
/// cada llamada al modelo y a las herramientas).
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).with_target(false).init();
}
