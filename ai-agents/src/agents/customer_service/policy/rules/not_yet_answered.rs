use crate::agents::customer_service::policy::{ReplyRule, RuleInput};

/// El agente aún no respondió el último mensaje del cliente (evita
/// responder dos veces si el mismo aviso llega repetido). Compara ids, que
/// siguen el orden de registro.
pub struct NotYetAnswered;

impl ReplyRule for NotYetAnswered {
    fn name(&self) -> &'static str {
        "not_yet_answered"
    }

    fn check(&self, input: &RuleInput<'_>) -> Result<(), String> {
        match (input.snapshot.latest_incoming, input.snapshot.last_ai_reply_id) {
            (Some(latest), Some(reply)) if reply > latest.id => {
                Err("el agente ya respondió el último mensaje del cliente".into())
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::customer_service::policy::fixtures::{event, snapshot};

    #[test]
    fn a_reply_after_the_message_means_it_was_answered() {
        let event = event();
        let mut snapshot = snapshot();
        assert!(NotYetAnswered.check(&RuleInput { event: &event, snapshot: &snapshot }).is_ok());

        snapshot.last_ai_reply_id = Some(101);
        assert!(NotYetAnswered.check(&RuleInput { event: &event, snapshot: &snapshot }).is_err());
    }
}
