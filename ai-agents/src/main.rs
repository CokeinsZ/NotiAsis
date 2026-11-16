use ai_agents::app::{self, Services};
use ai_agents::config::{ProcessEnv, Settings, load_dotenv};
use ai_agents::observability;

#[tokio::main]
async fn main() {
    load_dotenv();
    observability::init();

    let settings = match Settings::from_env(&ProcessEnv) {
        Ok(settings) => settings,
        Err(error) => {
            tracing::error!(%error, "invalid configuration");
            std::process::exit(1);
        }
    };
    let services = match Services::connect(&settings).await {
        Ok(services) => services,
        Err(error) => {
            tracing::error!(%error, "could not start services");
            std::process::exit(1);
        }
    };

    let app = app::router(&settings, services);
    let listener =
        tokio::net::TcpListener::bind(("0.0.0.0", settings.server.port)).await.expect("could not bind the HTTP port");
    tracing::info!(port = settings.server.port, "ai-agents listening");
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await.expect("server error");
}

/// Ctrl+C o SIGTERM (docker stop).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down");
}
