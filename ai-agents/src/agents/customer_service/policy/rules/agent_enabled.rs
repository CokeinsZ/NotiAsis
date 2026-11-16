use crate::agents::customer_service::policy::{ReplyRule, RuleInput};

/// El business tiene el agente activado (`business_ai_agents.enabled`).
pub struct AgentEnabled;

impl ReplyRule for AgentEnabled {
    fn name(&self) -> &'static str {
        "agent_enabled"
    }

    fn check(&self, input: &RuleInput<'_>) -> Result<(), String> {
        match input.snapshot.agent {
            Some(agent) if agent.enabled => Ok(()),
            Some(_) => Err("el agente está desactivado para este business".into()),
            None => Err("el business no tiene el agente configurado".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::customer_service::domain::AgentSwitch;
    use crate::agents::customer_service::policy::fixtures::{event, snapshot};

    #[test]
    fn requires_an_enabled_agent() {
        let event = event();
        let mut snapshot = snapshot();
        assert!(AgentEnabled.check(&RuleInput { event: &event, snapshot: &snapshot }).is_ok());

        snapshot.agent = Some(AgentSwitch { enabled: false, human_takeover_hours: 12 });
        assert!(AgentEnabled.check(&RuleInput { event: &event, snapshot: &snapshot }).is_err());
        snapshot.agent = None;
        assert!(AgentEnabled.check(&RuleInput { event: &event, snapshot: &snapshot }).is_err());
    }
}
