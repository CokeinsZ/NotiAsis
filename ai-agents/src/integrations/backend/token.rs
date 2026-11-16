use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::Mutex;

use super::BackendError;
use crate::shared::http_json::send_json;

/// Margen para renovar el token antes de que venza.
const RENEW_MARGIN: Duration = Duration::from_secs(60);

/// Entrega el JWT con el que se llama al backend.
#[async_trait]
pub trait TokenProvider: Send + Sync {
    async fn token(&self) -> Result<String, BackendError>;
    /// Descarta el token en caché (el backend respondió 401).
    async fn invalidate(&self);
}

/// JWT obtenido con una api_key de la tabla `api_keys`
/// (`POST /auth/api-key`), en caché hasta poco antes de vencer.
pub struct ApiKeyTokenProvider {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    cached: Mutex<Option<CachedToken>>,
}

struct CachedToken {
    value: String,
    renew_at: Instant,
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
    /// Segundos hasta el vencimiento.
    expires_in: i64,
}

impl ApiKeyTokenProvider {
    pub fn new(http: reqwest::Client, base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self { http, base_url: base_url.into(), api_key: api_key.into(), cached: Mutex::new(None) }
    }

    async fn login(&self) -> Result<CachedToken, BackendError> {
        let url = format!("{}/auth/api-key", self.base_url.trim_end_matches('/'));
        let response: TokenResponse =
            send_json(self.http.post(url).json(&serde_json::json!({ "api_key": self.api_key }))).await?;
        let lifetime = Duration::from_secs(response.expires_in.max(0) as u64);
        tracing::info!(expires_in = response.expires_in, "backend JWT obtained");
        Ok(CachedToken { value: response.token, renew_at: Instant::now() + lifetime.saturating_sub(RENEW_MARGIN) })
    }
}

#[async_trait]
impl TokenProvider for ApiKeyTokenProvider {
    async fn token(&self) -> Result<String, BackendError> {
        // El Mutex evita que varias tareas hagan login a la vez.
        let mut cached = self.cached.lock().await;
        if let Some(token) = cached.as_ref().filter(|token| Instant::now() < token.renew_at) {
            return Ok(token.value.clone());
        }
        let fresh = self.login().await?;
        let value = fresh.value.clone();
        *cached = Some(fresh);
        Ok(value)
    }

    async fn invalidate(&self) {
        *self.cached.lock().await = None;
    }
}
