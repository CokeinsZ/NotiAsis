use std::sync::Arc;

use axum::Json;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Header que deben mandar quienes llaman al servicio.
pub const API_KEY_HEADER: &str = "api-key";

/// Clave esperada en el header `api-key`.
#[derive(Clone)]
pub struct ApiKey(Arc<str>);

impl ApiKey {
    pub fn new(key: &str) -> Self {
        Self(Arc::from(key))
    }

    /// Comparación en tiempo constante (no filtra la clave por tiempos).
    pub fn matches(&self, candidate: &str) -> bool {
        let (expected, candidate) = (self.0.as_bytes(), candidate.as_bytes());
        expected.len() == candidate.len()
            && expected.iter().zip(candidate).fold(0u8, |diff, (a, b)| diff | (a ^ b)) == 0
    }
}

/// Middleware: 401 si el header `api-key` no coincide.
pub async fn require_api_key(State(key): State<ApiKey>, request: Request, next: Next) -> Response {
    let authorized = request
        .headers()
        .get(API_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|candidate| key.matches(candidate));
    if authorized {
        next.run(request).await
    } else {
        (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "message": "invalid or missing api-key" }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_only_the_exact_key() {
        let key = ApiKey::new("secret");
        assert!(key.matches("secret"));
        assert!(!key.matches("secreT"));
        assert!(!key.matches("secret2"));
        assert!(!key.matches(""));
    }
}
