use std::sync::Arc;

use async_trait::async_trait;

use crate::agents::customer_service::eligibility::EligibilityGate;
use crate::agents::customer_service::policy::Verdict;
use crate::agents::customer_service::ports::ReplyDelivery;
use crate::agents::customer_service::turn::Turn;
use crate::pipeline::{Flow, Step};
use crate::shared::error::BoxError;

/// Revisa las reglas otra vez (mientras el modelo pensaba pudo escribir un
/// asesor o el cliente) y envía la respuesta.
pub struct DeliverReply {
    gate: Arc<EligibilityGate>,
    delivery: Arc<dyn ReplyDelivery>,
}

impl DeliverReply {
    pub fn new(gate: Arc<EligibilityGate>, delivery: Arc<dyn ReplyDelivery>) -> Self {
        Self { gate, delivery }
    }
}

#[async_trait]
impl Step<Turn> for DeliverReply {
    fn name(&self) -> &'static str {
        "deliver_reply"
    }

    async fn run(&self, turn: &mut Turn) -> Result<Flow, BoxError> {
        if let Verdict::Skip { rule, reason } = self.gate.verdict(&turn.event).await? {
            return Ok(Flow::Halt(format!("descartada antes de enviar, {rule}: {reason}")));
        }
        let receipt = self.delivery.deliver(&turn.event.chat, turn.reply()?).await?;
        tracing::info!(chat = %turn.event.chat, message_id = ?receipt.message_id, "reply delivered");
        Ok(Flow::Continue)
    }
}
