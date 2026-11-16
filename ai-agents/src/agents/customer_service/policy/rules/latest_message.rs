use crate::agents::customer_service::policy::{ReplyRule, RuleInput};

/// Solo responde el turno del último mensaje del cliente: si escribió algo
/// más, el turno de ese mensaje responde todo junto.
pub struct LatestMessage;

impl ReplyRule for LatestMessage {
    fn name(&self) -> &'static str {
        "latest_message"
    }

    fn check(&self, input: &RuleInput<'_>) -> Result<(), String> {
        match input.snapshot.latest_incoming {
            Some(latest) if latest.id == input.event.message_id => Ok(()),
            Some(latest) => Err(format!("el cliente escribió después (mensaje {})", latest.id)),
            None => Err("el chat no tiene mensajes del cliente".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::customer_service::domain::IncomingRef;
    use crate::agents::customer_service::policy::fixtures::{at, event, snapshot};

    #[test]
    fn newer_messages_skip_this_turn() {
        let event = event();
        let mut snapshot = snapshot();
        assert!(LatestMessage.check(&RuleInput { event: &event, snapshot: &snapshot }).is_ok());

        snapshot.latest_incoming = Some(IncomingRef { id: 101, at: at(15, 0) });
        assert!(LatestMessage.check(&RuleInput { event: &event, snapshot: &snapshot }).is_err());
    }
}
