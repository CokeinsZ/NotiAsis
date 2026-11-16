use crate::agents::customer_service::policy::{ReplyRule, RuleInput};

/// Tope de respuestas del agente por chat en la última hora. Corta los
/// bucles con respuestas automáticas de otros negocios.
pub struct RateLimit {
    max_per_hour: i64,
}

impl RateLimit {
    pub fn new(max_per_hour: i64) -> Self {
        Self { max_per_hour }
    }
}

impl ReplyRule for RateLimit {
    fn name(&self) -> &'static str {
        "rate_limit"
    }

    fn check(&self, input: &RuleInput<'_>) -> Result<(), String> {
        if input.snapshot.ai_replies_last_hour < self.max_per_hour {
            Ok(())
        } else {
            Err(format!("el agente ya envió {} mensajes en la última hora", input.snapshot.ai_replies_last_hour))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::customer_service::policy::fixtures::{event, snapshot};

    #[test]
    fn stops_at_the_hourly_cap() {
        let event = event();
        let mut snapshot = snapshot();
        snapshot.ai_replies_last_hour = 5;
        assert!(RateLimit::new(6).check(&RuleInput { event: &event, snapshot: &snapshot }).is_ok());
        snapshot.ai_replies_last_hour = 6;
        assert!(RateLimit::new(6).check(&RuleInput { event: &event, snapshot: &snapshot }).is_err());
    }
}
