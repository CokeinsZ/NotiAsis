//! Formas de entregar la respuesta. Se elige una en `assembly.rs` según
//! `CUSTOMER_AGENT_DRY_RUN`.

use std::sync::Arc;

use async_trait::async_trait;

use super::ports::{DeliveryError, DeliveryReceipt, ReplyDelivery};
use crate::domain::ChatKey;
use crate::integrations::backend::AiMessageSender;

/// Envía por el backend: WhatsApp + historial con `sender = 'ai'`.
pub struct BackendReplyDelivery {
    sender: Arc<dyn AiMessageSender>,
}

impl BackendReplyDelivery {
    pub fn new(sender: Arc<dyn AiMessageSender>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl ReplyDelivery for BackendReplyDelivery {
    async fn deliver(&self, chat: &ChatKey, text: &str) -> Result<DeliveryReceipt, DeliveryError> {
        let sent = self.sender.send_ai_message(chat, text).await?;
        Ok(DeliveryReceipt { message_id: Some(sent.id) })
    }
}

/// Modo borrador: la respuesta queda en los logs y no se envía (para
/// revisar la calidad antes de activarlo con clientes reales).
pub struct DryRunReplyDelivery;

#[async_trait]
impl ReplyDelivery for DryRunReplyDelivery {
    async fn deliver(&self, chat: &ChatKey, text: &str) -> Result<DeliveryReceipt, DeliveryError> {
        tracing::info!(%chat, reply = %text, "DRY RUN: reply not sent");
        Ok(DeliveryReceipt { message_id: None })
    }
}
