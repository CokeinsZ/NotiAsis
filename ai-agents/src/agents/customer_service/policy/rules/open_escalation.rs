use chrono::Duration;

use crate::agents::customer_service::policy::{ReplyRule, RuleInput};

/// Después de escalar un chat, el agente se calla hasta que responda un
/// asesor o pase `quiet_period` (para no resolver lo que ya pasó a una
/// persona).
pub struct OpenEscalation {
    quiet_period: Duration,
}

impl OpenEscalation {
    pub fn new(quiet_period: Duration) -> Self {
        Self { quiet_period }
    }
}

impl ReplyRule for OpenEscalation {
    fn name(&self) -> &'static str {
        "open_escalation"
    }

    fn check(&self, input: &RuleInput<'_>) -> Result<(), String> {
        let Some(escalated_at) = input.snapshot.last_escalation_at else {
            return Ok(());
        };
        let answered_by_human = input.snapshot.last_associate_message_at.is_some_and(|human| human >= escalated_at);
        let expired = input.snapshot.now - escalated_at >= self.quiet_period;
        if answered_by_human || expired {
            Ok(())
        } else {
            Err("el chat está escalado y ningún asesor ha respondido".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::customer_service::policy::fixtures::{at, event, snapshot};

    #[test]
    fn escalation_stays_open_until_a_human_answers_or_it_expires() {
        let rule = OpenEscalation::new(Duration::hours(24));
        let event = event();
        let mut snapshot = snapshot();
        snapshot.last_escalation_at = Some(at(14, 0));
        assert!(rule.check(&RuleInput { event: &event, snapshot: &snapshot }).is_err());

        snapshot.last_associate_message_at = Some(at(14, 30));
        assert!(rule.check(&RuleInput { event: &event, snapshot: &snapshot }).is_ok());

        snapshot.last_associate_message_at = None;
        snapshot.now = at(14, 0) + Duration::hours(24);
        assert!(rule.check(&RuleInput { event: &event, snapshot: &snapshot }).is_ok());
    }
}
