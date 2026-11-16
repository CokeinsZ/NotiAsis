//! Backend de NotiAsis (Rust/Axum): descarga de multimedia de Meta y envío
//! de las respuestas del agente (`POST /chats/{bid}/{phone}/ai-messages`,
//! que las envía por WhatsApp y las guarda en el historial con
//! `sender = 'ai'`).

mod client;
mod token;

use async_trait::async_trait;

use crate::domain::ChatKey;
use crate::shared::http_json::HttpError;

pub use client::BackendClient;
pub use token::{ApiKeyTokenProvider, TokenProvider};

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("backend HTTP error: {0}")]
    Http(#[from] HttpError),
    #[error("backend client configuration error: {0}")]
    Configuration(String),
}

impl BackendError {
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Http(error) => error.status(),
            Self::Configuration(_) => None,
        }
    }
}

/// Archivo multimedia descargado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedMedia {
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

/// Mensaje que el backend envió y guardó.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentMessage {
    pub id: i32,
    pub meta_message_id: String,
}

/// Descarga multimedia de los mensajes (notas de voz, imágenes...).
#[async_trait]
pub trait MediaDownloader: Send + Sync {
    async fn download(&self, media_id: &str) -> Result<DownloadedMedia, BackendError>;
}

/// Envía un mensaje libre escrito por el agente.
#[async_trait]
pub trait AiMessageSender: Send + Sync {
    async fn send_ai_message(&self, chat: &ChatKey, text: &str) -> Result<SentMessage, BackendError>;
}
