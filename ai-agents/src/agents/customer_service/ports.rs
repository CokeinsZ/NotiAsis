//! Puertos del agente: lo que necesita del mundo exterior, como traits
//! pequeños (uno por responsabilidad). Las implementaciones con Postgres
//! están en `repositories/`; los tests usan fakes.

use async_trait::async_trait;

use super::domain::{
    ChatSnapshot, CustomerContext, Escalation, GuideDetails, GuideUpdate, PendingVoiceNote, ProductSheet,
    TranscriptEntry,
};
use crate::domain::ChatKey;
use crate::integrations::backend::BackendError;

#[derive(Debug, thiserror::Error)]
#[error("database error: {0}")]
pub struct RepositoryError(#[from] pub sqlx::Error);

/// Lo que las reglas necesitan para decidir si el agente responde.
#[async_trait]
pub trait ChatStateReader: Send + Sync {
    /// `None` si el chat no existe.
    async fn snapshot(&self, chat: &ChatKey) -> Result<Option<ChatSnapshot>, RepositoryError>;
}

#[async_trait]
pub trait CustomerContextReader: Send + Sync {
    /// `None` si el chat no existe. Trae hasta `guide_limit` guías.
    async fn load(&self, chat: &ChatKey, guide_limit: i64) -> Result<Option<CustomerContext>, RepositoryError>;
}

#[async_trait]
pub trait TranscriptReader: Send + Sync {
    /// Los últimos `limit` mensajes del chat, del más antiguo al más nuevo.
    async fn recent(&self, chat: &ChatKey, limit: i64) -> Result<Vec<TranscriptEntry>, RepositoryError>;
}

#[async_trait]
pub trait VoiceNoteStore: Send + Sync {
    /// Notas de voz recientes del cliente sin transcribir.
    async fn pending(&self, chat: &ChatKey, limit: i64) -> Result<Vec<PendingVoiceNote>, RepositoryError>;
    async fn save_transcription(&self, message_id: i32, text: &str) -> Result<(), RepositoryError>;
}

#[async_trait]
pub trait GuideReader: Send + Sync {
    /// Una guía del cliente del chat. `None` si no existe o es de otro
    /// cliente o business.
    async fn details(&self, chat: &ChatKey, guide_number: &str) -> Result<Option<GuideDetails>, RepositoryError>;
}

#[async_trait]
pub trait ProductCatalog: Send + Sync {
    /// Productos activos del catálogo del business, por nombre.
    async fn products(&self, business_id: i32) -> Result<Vec<ProductSheet>, RepositoryError>;
}

#[async_trait]
pub trait GuideUpdateWriter: Send + Sync {
    /// `false` si la guía no es del cliente del chat.
    async fn record(&self, chat: &ChatKey, update: &GuideUpdate) -> Result<bool, RepositoryError>;
}

#[async_trait]
pub trait EscalationWriter: Send + Sync {
    /// Registra el escalamiento y marca el chat como importante.
    async fn escalate(&self, chat: &ChatKey, escalation: &Escalation) -> Result<(), RepositoryError>;
}

/// Comprobante de una respuesta entregada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryReceipt {
    /// `messages.id` del mensaje enviado (`None` en modo borrador).
    pub message_id: Option<i32>,
}

#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    #[error(transparent)]
    Backend(#[from] BackendError),
}

/// Entrega la respuesta del agente al cliente.
#[async_trait]
pub trait ReplyDelivery: Send + Sync {
    async fn deliver(&self, chat: &ChatKey, text: &str) -> Result<DeliveryReceipt, DeliveryError>;
}
