use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;

use super::{AiMessageSender, BackendError, DownloadedMedia, MediaDownloader, SentMessage, TokenProvider};
use crate::domain::ChatKey;
use crate::shared::http_json::{BinaryBody, HttpError, send_binary, send_json};
use crate::shared::retry::RetryPolicy;

/// Cliente HTTP del backend de NotiAsis. Adjunta el JWT en cada llamada y,
/// si el backend responde 401, renueva el token y reintenta una vez.
pub struct BackendClient {
    http: reqwest::Client,
    base_url: String,
    tokens: Arc<dyn TokenProvider>,
    retry: RetryPolicy,
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}

#[derive(Deserialize)]
struct MessageData {
    id: i32,
    meta_message_id: String,
}

impl BackendClient {
    pub fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        tokens: Arc<dyn TokenProvider>,
        retry: RetryPolicy,
    ) -> Self {
        Self { http, base_url: base_url.into(), tokens, retry }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }

    /// Ejecuta `call` con el token vigente; ante un 401 renueva y reintenta.
    async fn authorized<T, F, Fut>(&self, call: F) -> Result<T, BackendError>
    where
        F: Fn(String) -> Fut,
        Fut: std::future::Future<Output = Result<T, HttpError>>,
    {
        let token = self.tokens.token().await?;
        match call(token).await {
            Err(error) if error.status() == Some(401) => {
                self.tokens.invalidate().await;
                let token = self.tokens.token().await?;
                Ok(call(token).await?)
            }
            other => Ok(other?),
        }
    }
}

#[async_trait]
impl MediaDownloader for BackendClient {
    async fn download(&self, media_id: &str) -> Result<DownloadedMedia, BackendError> {
        let url = self.url(&format!("/messages/media/{media_id}"));
        let body: BinaryBody = self
            .authorized(|token| {
                let url = url.clone();
                async move {
                    self.retry
                        .run(|| send_binary(self.http.get(&url).bearer_auth(&token)), HttpError::is_retryable)
                        .await
                }
            })
            .await?;
        Ok(DownloadedMedia {
            mime_type: body.content_type.unwrap_or_else(|| "application/octet-stream".to_string()),
            bytes: body.bytes,
        })
    }
}

#[async_trait]
impl AiMessageSender for BackendClient {
    async fn send_ai_message(&self, chat: &ChatKey, text: &str) -> Result<SentMessage, BackendError> {
        let url = self.url(&format!("/chats/{}/{}/ai-messages", chat.business_id, chat.user_phone));
        let body = serde_json::json!({ "message": text });
        // Sin reintentos automáticos: si Meta recibió el mensaje y la
        // respuesta se perdió, reintentar lo duplicaría al cliente.
        let envelope: Envelope<MessageData> =
            self.authorized(|token| send_json(self.http.post(&url).bearer_auth(token).json(&body))).await?;
        Ok(SentMessage { id: envelope.data.id, meta_message_id: envelope.data.meta_message_id })
    }
}
