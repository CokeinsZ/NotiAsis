//! Composition root del servicio: crea los clientes externos una sola vez
//! y monta cada agente. Para cambiar de proveedor (otro LLM u otro
//! transcriptor) solo se cambia la construcción en [`Services::connect`].

use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::agents::customer_service::{self, CustomerServiceDeps};
use crate::config::Settings;
use crate::http::api_key::{ApiKey, require_api_key};
use crate::http::health::health;
use crate::integrations::backend::{ApiKeyTokenProvider, BackendClient};
use crate::integrations::dropi::{N8nDropiClient, OrderLookup};
use crate::llm::ChatModel;
use crate::llm::deepseek::DeepSeekChatModel;
use crate::shared::error::BoxError;
use crate::shared::retry::RetryPolicy;
use crate::transcription::Transcriber;
use crate::transcription::gemini::GeminiTranscriber;

/// Clientes compartidos por los agentes.
#[derive(Clone)]
pub struct Services {
    pub pool: PgPool,
    pub chat_model: Arc<dyn ChatModel>,
    pub transcriber: Arc<dyn Transcriber>,
    pub backend: Arc<BackendClient>,
    pub orders: Arc<dyn OrderLookup>,
}

impl Services {
    pub async fn connect(settings: &Settings) -> Result<Self, BoxError> {
        let pool = PgPoolOptions::new()
            .max_connections(settings.database.max_connections)
            .connect(&settings.database.url)
            .await?;
        Self::with_pool(settings, pool)
    }

    /// Igual que `connect`, con un pool ya creado (o perezoso, en tests).
    pub fn with_pool(settings: &Settings, pool: PgPool) -> Result<Self, BoxError> {
        let backend_http = reqwest::Client::builder().timeout(settings.backend.timeout).build()?;
        let tokens = Arc::new(ApiKeyTokenProvider::new(
            backend_http.clone(),
            &settings.backend.base_url,
            &settings.backend.api_key,
        ));
        let dropi_http = reqwest::Client::builder().timeout(settings.dropi.timeout).build()?;

        Ok(Self {
            pool,
            chat_model: Arc::new(DeepSeekChatModel::new(settings.deepseek.clone())?),
            transcriber: Arc::new(GeminiTranscriber::new(settings.gemini.clone())?),
            backend: Arc::new(BackendClient::new(
                backend_http,
                &settings.backend.base_url,
                tokens,
                RetryPolicy::default(),
            )),
            orders: Arc::new(N8nDropiClient::new(
                dropi_http,
                &settings.dropi.base_url,
                &settings.dropi.api_key,
                RetryPolicy::default(),
            )),
        })
    }
}

/// Todas las rutas. `/health` es pública; los agentes exigen `api-key`.
pub fn router(settings: &Settings, services: Services) -> Router {
    let agents = Router::new()
        .nest(
            "/agents/customer-service",
            customer_service::router(
                &settings.customer_service,
                CustomerServiceDeps {
                    pool: services.pool.clone(),
                    chat_model: services.chat_model.clone(),
                    transcriber: services.transcriber.clone(),
                    media: services.backend.clone(),
                    messages: services.backend.clone(),
                    orders: services.orders.clone(),
                },
            ),
        )
        .layer(from_fn_with_state(ApiKey::new(&settings.server.api_key), require_api_key));

    Router::new().route("/health", get(health)).merge(agents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MapEnv;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn app() -> Router {
        let env = MapEnv::from([
            ("AI_AGENTS_API_KEY", "secret"),
            ("DATABASE_URL", "postgres://localhost/notiasis"),
            ("DEEPSEEK_API_KEY", "ds"),
            ("GOOGLE_AI_API_KEY", "gm"),
            ("BACKEND_API_URL", "http://backend:3001"),
            ("BACKEND_API_KEY", "bk"),
            ("N8N_API_KEY", "n8n"),
        ]);
        let settings = Settings::from_env(&env).unwrap();
        // Pool perezoso: no conecta hasta la primera consulta.
        let pool = PgPoolOptions::new().connect_lazy(&settings.database.url).unwrap();
        router(&settings, Services::with_pool(&settings, pool).unwrap())
    }

    #[tokio::test]
    async fn health_is_public() {
        let response = app().oneshot(Request::get("/health").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn agent_routes_require_the_api_key() {
        let request = |key: Option<&str>| {
            let mut builder =
                Request::post("/agents/customer-service/events").header("Content-Type", "application/json");
            if let Some(key) = key {
                builder = builder.header("api-key", key);
            }
            builder.body(Body::from(r#"{"message_id": 0, "business_id": 1, "user_phone": "573003579384"}"#)).unwrap()
        };
        assert_eq!(app().oneshot(request(None)).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        assert_eq!(app().oneshot(request(Some("wrong"))).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        // Con la clave correcta llega al controlador (message_id 0 es inválido).
        assert_eq!(app().oneshot(request(Some("secret"))).await.unwrap().status(), StatusCode::BAD_REQUEST);
    }
}
