//! Reglas que deciden si el agente responde un mensaje.
//!
//! Cada regla vive en su archivo de `rules/` y es una función pura sobre
//! la foto del chat, así se prueba sin base de datos. Agregar una regla es
//! escribir el archivo y sumarla en [`ReplyPolicy::standard`].

pub mod rules;

use chrono::Duration;

use super::config::CustomerServiceConfig;
use super::domain::{ChatSnapshot, IncomingMessageEvent};

/// Datos con los que se evalúa cada regla.
#[derive(Debug, Clone, Copy)]
pub struct RuleInput<'a> {
    pub event: &'a IncomingMessageEvent,
    pub snapshot: &'a ChatSnapshot,
}

/// Una regla: `Err(motivo)` si el agente NO debe responder.
pub trait ReplyRule: Send + Sync {
    fn name(&self) -> &'static str;
    fn check(&self, input: &RuleInput<'_>) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Reply,
    Skip { rule: &'static str, reason: String },
}

/// Lista de reglas; la primera que falle decide.
pub struct ReplyPolicy {
    rules: Vec<Box<dyn ReplyRule>>,
}

impl ReplyPolicy {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn with(mut self, rule: impl ReplyRule + 'static) -> Self {
        self.rules.push(Box::new(rule));
        self
    }

    /// Las reglas del agente, de la más barata/obvia a la más específica.
    pub fn standard(config: &CustomerServiceConfig) -> Self {
        Self::new()
            .with(rules::AgentEnabled)
            .with(rules::Allowlist::new(config.allowed_phones.clone()))
            .with(rules::LatestMessage)
            .with(rules::ServiceWindow)
            .with(rules::NotYetAnswered)
            .with(rules::HumanTakeover)
            .with(rules::OpenEscalation::new(Duration::hours(config.escalation_quiet_hours)))
            .with(rules::RateLimit::new(config.max_replies_per_hour))
    }

    pub fn rule_names(&self) -> Vec<&'static str> {
        self.rules.iter().map(|rule| rule.name()).collect()
    }

    pub fn evaluate(&self, input: &RuleInput<'_>) -> Verdict {
        for rule in &self.rules {
            if let Err(reason) = rule.check(input) {
                return Verdict::Skip { rule: rule.name(), reason };
            }
        }
        Verdict::Reply
    }
}

impl Default for ReplyPolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use chrono::{NaiveDate, NaiveDateTime};

    use crate::agents::customer_service::domain::{AgentSwitch, ChatSnapshot, IncomingMessageEvent, IncomingRef};
    use crate::domain::ChatKey;

    pub fn at(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 15).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    pub fn event() -> IncomingMessageEvent {
        IncomingMessageEvent { message_id: 100, chat: ChatKey::new(1, "573003579384") }
    }

    /// Chat listo para responder: el mensaje 100 llegó a las 15:00 y son las 15:01.
    pub fn snapshot() -> ChatSnapshot {
        ChatSnapshot {
            now: at(15, 1),
            latest_incoming: Some(IncomingRef { id: 100, at: at(15, 0) }),
            last_ai_reply_id: Some(90),
            last_associate_message_at: None,
            last_escalation_at: None,
            ai_replies_last_hour: 0,
            agent: Some(AgentSwitch { enabled: true, human_takeover_hours: 12 }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{event, snapshot};
    use super::*;

    #[test]
    fn standard_policy_replies_to_a_fresh_message() {
        let policy = ReplyPolicy::standard(&CustomerServiceConfig::default());
        let (event, snapshot) = (event(), snapshot());
        assert_eq!(policy.evaluate(&RuleInput { event: &event, snapshot: &snapshot }), Verdict::Reply);
    }

    #[test]
    fn the_first_failing_rule_decides() {
        let policy = ReplyPolicy::standard(&CustomerServiceConfig::default());
        let event = event();
        let mut snapshot = snapshot();
        snapshot.agent = None;
        snapshot.ai_replies_last_hour = 99;
        let verdict = policy.evaluate(&RuleInput { event: &event, snapshot: &snapshot });
        assert!(matches!(verdict, Verdict::Skip { rule: "agent_enabled", .. }));
    }

    #[test]
    fn standard_policy_lists_every_rule() {
        let names = ReplyPolicy::standard(&CustomerServiceConfig::default()).rule_names();
        assert_eq!(
            names,
            vec![
                "agent_enabled",
                "allowlist",
                "latest_message",
                "service_window",
                "not_yet_answered",
                "human_takeover",
                "open_escalation",
                "rate_limit"
            ]
        );
    }
}
