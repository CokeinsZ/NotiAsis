use reqwest::RequestBuilder;
use serde::de::DeserializeOwned;

use crate::shared::text::truncate_chars;

/// Máximo de caracteres del cuerpo de una respuesta de error que se
/// conserva (para logs, sin inundarlos).
const ERROR_BODY_LIMIT: usize = 2_000;

/// Error de una llamada HTTP a un servicio externo.
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("HTTP {status}: {body}")]
    Status { status: u16, body: String },
    #[error("invalid response body: {0}")]
    Decode(String),
}

impl HttpError {
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Status { status, .. } => Some(*status),
            Self::Transport(error) => error.status().map(|status| status.as_u16()),
            Self::Decode(_) => None,
        }
    }

    /// Errores pasajeros que vale la pena reintentar: timeouts, fallas de
    /// conexión, 429 (rate limit) y 5xx.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Transport(error) => error.is_timeout() || error.is_connect(),
            Self::Status { status, .. } => *status == 429 || *status >= 500,
            Self::Decode(_) => false,
        }
    }
}

/// Envía la petición y decodifica la respuesta JSON. Cualquier estado que
/// no sea 2xx es un [`HttpError::Status`] con el cuerpo de la respuesta.
pub async fn send_json<T: DeserializeOwned>(request: RequestBuilder) -> Result<T, HttpError> {
    let response = request.send().await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(HttpError::Status { status: status.as_u16(), body: truncate_chars(&body, ERROR_BODY_LIMIT) });
    }
    serde_json::from_str(&body)
        .map_err(|error| HttpError::Decode(format!("{error}; body: {}", truncate_chars(&body, 500))))
}

/// Respuesta binaria (archivos multimedia).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryBody {
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

/// Envía la petición y devuelve el cuerpo en bytes con su Content-Type.
pub async fn send_binary(request: RequestBuilder) -> Result<BinaryBody, HttpError> {
    let response = request.send().await?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(HttpError::Status { status: status.as_u16(), body: truncate_chars(&body, ERROR_BODY_LIMIT) });
    }
    let content_type =
        response.headers().get(reqwest::header::CONTENT_TYPE).and_then(|value| value.to_str().ok()).map(str::to_string);
    let bytes = response.bytes().await?.to_vec();
    Ok(BinaryBody { content_type, bytes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limits_and_server_errors_are_retryable() {
        let status = |status| HttpError::Status { status, body: String::new() };
        assert!(status(429).is_retryable());
        assert!(status(503).is_retryable());
        assert!(!status(400).is_retryable());
        assert!(!status(401).is_retryable());
        assert!(!HttpError::Decode("x".into()).is_retryable());
    }
}
