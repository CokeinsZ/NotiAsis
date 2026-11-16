use tokio::sync::OwnedSemaphorePermit;

use super::domain::{CustomerContext, IncomingMessageEvent, TranscriptEntry};
use crate::llm::TokenUsage;
use crate::shared::keyed_lock::KeyedGuard;

/// Estado de un turno: lo que cada paso del pipeline deja para los
/// siguientes.
pub struct Turn {
    pub event: IncomingMessageEvent,
    /// Exclusión por chat (paso `LockChat`), hasta que termine el turno.
    pub chat_guard: Option<KeyedGuard>,
    /// Cupo de turnos simultáneos (paso `ReserveCapacity`).
    pub capacity: Option<OwnedSemaphorePermit>,
    pub context: Option<CustomerContext>,
    pub transcript: Vec<TranscriptEntry>,
    /// Respuesta lista para enviar.
    pub reply: Option<String>,
    pub usage: TokenUsage,
}

/// Un paso necesita algo que un paso anterior debía dejar (error de orden
/// en el pipeline).
#[derive(Debug, thiserror::Error)]
#[error("turn state is missing `{0}`: check the step order in assembly.rs")]
pub struct MissingState(pub &'static str);

impl Turn {
    pub fn new(event: IncomingMessageEvent) -> Self {
        Self {
            event,
            chat_guard: None,
            capacity: None,
            context: None,
            transcript: Vec::new(),
            reply: None,
            usage: TokenUsage::default(),
        }
    }

    pub fn context(&self) -> Result<&CustomerContext, MissingState> {
        self.context.as_ref().ok_or(MissingState("context"))
    }

    pub fn reply(&self) -> Result<&str, MissingState> {
        self.reply.as_deref().ok_or(MissingState("reply"))
    }
}
