use std::collections::HashSet;

use super::env::{ConfigError, Env};
use crate::agents::customer_service::CustomerServiceConfig;
use crate::shared::text::normalize_phone;

/// Variables `CUSTOMER_AGENT_*` (ver `.env.example`).
pub fn customer_service(env: &Env) -> Result<CustomerServiceConfig, ConfigError> {
    let defaults = CustomerServiceConfig::default();
    Ok(CustomerServiceConfig {
        quiet_period: env.seconds("CUSTOMER_AGENT_QUIET_SECONDS", defaults.quiet_period.as_secs())?,
        max_tool_rounds: env.parse("CUSTOMER_AGENT_MAX_TOOL_ROUNDS", defaults.max_tool_rounds)?,
        history_limit: env.parse("CUSTOMER_AGENT_HISTORY_LIMIT", defaults.history_limit)?,
        guide_limit: env.parse("CUSTOMER_AGENT_GUIDE_LIMIT", defaults.guide_limit)?,
        max_replies_per_hour: env.parse("CUSTOMER_AGENT_MAX_REPLIES_PER_HOUR", defaults.max_replies_per_hour)?,
        escalation_quiet_hours: env.parse("CUSTOMER_AGENT_ESCALATION_QUIET_HOURS", defaults.escalation_quiet_hours)?,
        allowed_phones: allowed_phones(env)?,
        max_concurrent_turns: env.parse("CUSTOMER_AGENT_MAX_CONCURRENT_TURNS", defaults.max_concurrent_turns)?,
        max_voice_notes_per_turn: env.parse("CUSTOMER_AGENT_MAX_VOICE_NOTES", defaults.max_voice_notes_per_turn)?,
        dry_run: env.flag("CUSTOMER_AGENT_DRY_RUN", defaults.dry_run)?,
        base_vocabulary: env.list_or("CUSTOMER_AGENT_VOCABULARY", &defaults.base_vocabulary),
    })
}

fn allowed_phones(env: &Env) -> Result<HashSet<String>, ConfigError> {
    env.list("CUSTOMER_AGENT_ALLOWED_PHONES")
        .into_iter()
        .map(|phone| {
            normalize_phone(&phone).ok_or(ConfigError::Invalid {
                key: "CUSTOMER_AGENT_ALLOWED_PHONES",
                reason: format!("{phone:?} no es un teléfono de 8 a 15 dígitos"),
            })
        })
        .collect()
}
