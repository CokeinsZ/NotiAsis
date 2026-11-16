use chrono::Duration;

use crate::agents::customer_service::policy::{ReplyRule, RuleInput};

/// Si un asesor escribió en el chat desde la webapp, el agente se calla
/// durante `business_ai_agents.human_takeover_hours` para no contradecirlo.
pub struct HumanTakeover;

impl ReplyRule for HumanTakeover {
    fn name(&self) -> &'static str {
        "human_takeover"
    }

    fn check(&self, input: &RuleInput<'_>) -> Result<(), String> {
        let (Some(last_human), Some(agent)) = (input.snapshot.last_associate_message_at, input.snapshot.agent) else {
            return Ok(());
        };
        if input.snapshot.now - last_human < Duration::hours(agent.human_takeover_hours) {
            Err(format!("un asesor escribió hace menos de {} h", agent.human_takeover_hours))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::customer_service::policy::fixtures::{at, event, snapshot};

    #[test]
    fn a_recent_associate_message_pauses_the_agent() {
        let event = event();
        let mut snapshot = snapshot();
        snapshot.last_associate_message_at = Some(at(5, 0));
        assert!(HumanTakeover.check(&RuleInput { event: &event, snapshot: &snapshot }).is_err());

        snapshot.last_associate_message_at = Some(at(2, 0));
        assert!(HumanTakeover.check(&RuleInput { event: &event, snapshot: &snapshot }).is_ok());
    }
}
