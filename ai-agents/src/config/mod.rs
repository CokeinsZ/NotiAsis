//! Configuración desde variables de entorno. Cada sección se lee en su
//! archivo; [`Settings`] solo las junta.

mod customer_service;
mod env;
mod integrations;
mod models;
mod server;

pub use env::{ConfigError, Env, EnvSource, MapEnv, ProcessEnv};
pub use integrations::{BackendSettings, DropiSettings};
pub use server::{DatabaseSettings, ServerSettings};

use crate::agents::customer_service::CustomerServiceConfig;
use crate::llm::deepseek::DeepSeekConfig;
use crate::transcription::gemini::GeminiTranscribeConfig;

#[derive(Debug, Clone)]
pub struct Settings {
    pub server: ServerSettings,
    pub database: DatabaseSettings,
    pub deepseek: DeepSeekConfig,
    pub gemini: GeminiTranscribeConfig,
    pub backend: BackendSettings,
    pub dropi: DropiSettings,
    pub customer_service: CustomerServiceConfig,
}

impl Settings {
    pub fn from_env(source: &dyn EnvSource) -> Result<Self, ConfigError> {
        let env = Env::new(source);
        Ok(Self {
            server: server::server(&env)?,
            database: server::database(&env)?,
            deepseek: models::deepseek(&env)?,
            gemini: models::gemini(&env)?,
            backend: integrations::backend(&env)?,
            dropi: integrations::dropi(&env)?,
            customer_service: customer_service::customer_service(&env)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::deepseek::ReasoningEffort;

    fn required() -> MapEnv {
        MapEnv::from([
            ("AI_AGENTS_API_KEY", "secret"),
            ("DATABASE_URL", "postgres://localhost/notiasis"),
            ("DEEPSEEK_API_KEY", "ds"),
            ("GOOGLE_AI_API_KEY", "gm"),
            ("BACKEND_API_URL", "http://backend:3001"),
            ("BACKEND_API_KEY", "bk"),
            ("N8N_API_KEY", "n8n"),
        ])
    }

    #[test]
    fn defaults_keep_every_model_capability_on() {
        let settings = Settings::from_env(&required()).unwrap();
        assert_eq!(settings.server.port, 3002);
        assert_eq!(settings.deepseek.model, "deepseek-flash");
        assert_eq!(settings.deepseek.reasoning_effort, ReasoningEffort::Max);
        assert_eq!(settings.gemini.model, "gemini-3.5-transcribe");
        assert_eq!(settings.gemini.language_codes, vec!["es-419".to_string()]);
        assert!(!settings.customer_service.dry_run);
    }

    #[test]
    fn missing_required_values_are_reported_by_name() {
        let mut env = required();
        env.remove("DEEPSEEK_API_KEY");
        let error = Settings::from_env(&env).unwrap_err();
        assert_eq!(error, ConfigError::Missing("DEEPSEEK_API_KEY"));
    }

    #[test]
    fn overrides_are_parsed() {
        let mut env = required();
        env.set("DEEPSEEK_REASONING_EFFORT", "high");
        env.set("CUSTOMER_AGENT_ALLOWED_PHONES", "+573003579384, 50241571896");
        env.set("CUSTOMER_AGENT_DRY_RUN", "true");
        env.set("CUSTOMER_AGENT_QUIET_SECONDS", "5");
        let settings = Settings::from_env(&env).unwrap();
        assert_eq!(settings.deepseek.reasoning_effort, ReasoningEffort::High);
        assert!(settings.customer_service.dry_run);
        assert_eq!(settings.customer_service.quiet_period.as_secs(), 5);
        assert!(settings.customer_service.allowed_phones.contains("573003579384"));
        assert!(settings.customer_service.allowed_phones.contains("50241571896"));
    }

    #[test]
    fn invalid_values_are_errors() {
        let mut env = required();
        env.set("CUSTOMER_AGENT_ALLOWED_PHONES", "123");
        assert!(matches!(Settings::from_env(&env), Err(ConfigError::Invalid { .. })));
    }
}
