use std::sync::Arc;

use super::domain::IncomingMessageEvent;
use super::policy::{ReplyPolicy, RuleInput, Verdict};
use super::ports::{ChatStateReader, RepositoryError};

/// Lee la foto del chat y aplica las reglas. Se consulta antes de generar
/// la respuesta y otra vez justo antes de enviarla (mientras el modelo
/// pensaba pudo escribir un asesor o el cliente).
pub struct EligibilityGate {
    state: Arc<dyn ChatStateReader>,
    policy: ReplyPolicy,
}

impl EligibilityGate {
    pub fn new(state: Arc<dyn ChatStateReader>, policy: ReplyPolicy) -> Self {
        Self { state, policy }
    }

    pub async fn verdict(&self, event: &IncomingMessageEvent) -> Result<Verdict, RepositoryError> {
        let Some(snapshot) = self.state.snapshot(&event.chat).await? else {
            return Ok(Verdict::Skip { rule: "chat_exists", reason: "el chat no existe".into() });
        };
        Ok(self.policy.evaluate(&RuleInput { event, snapshot: &snapshot }))
    }
}
