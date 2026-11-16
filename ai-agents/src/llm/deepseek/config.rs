use std::str::FromStr;
use std::time::Duration;

use serde::Serialize;

use crate::shared::retry::RetryPolicy;

/// Modo thinking: el modelo razona antes de responder. Es el modo por
/// defecto de `deepseek-flash`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingMode {
    Enabled,
    Disabled,
}

/// Esfuerzo de razonamiento (`reasoning_effort`). `Max` es la máxima
/// capacidad del modelo (y sube el `max_tokens` por defecto a 128K).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    High,
    Max,
}

#[derive(Debug, Clone)]
pub struct DeepSeekConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub thinking: ThinkingMode,
    pub reasoning_effort: ReasoningEffort,
    /// `None` deja el máximo por defecto del modelo (128K con esfuerzo `max`).
    pub max_tokens: Option<u32>,
    pub timeout: Duration,
    pub retry: RetryPolicy,
}

impl DeepSeekConfig {
    pub const DEFAULT_BASE_URL: &'static str = "https://api.deepseek.com";
    pub const DEFAULT_MODEL: &'static str = "deepseek-flash";

    /// Configuración con todas las capacidades activas: thinking y
    /// esfuerzo máximo.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: Self::DEFAULT_BASE_URL.to_string(),
            model: Self::DEFAULT_MODEL.to_string(),
            thinking: ThinkingMode::Enabled,
            reasoning_effort: ReasoningEffort::Max,
            max_tokens: None,
            // El razonamiento máximo puede tardar varios minutos.
            timeout: Duration::from_secs(600),
            retry: RetryPolicy::default(),
        }
    }
}

impl FromStr for ThinkingMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "enabled" | "true" | "on" => Ok(Self::Enabled),
            "disabled" | "false" | "off" => Ok(Self::Disabled),
            other => Err(format!("thinking mode must be enabled or disabled, got {other:?}")),
        }
    }
}

impl FromStr for ReasoningEffort {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "low" => Ok(Self::Low),
            "high" => Ok(Self::High),
            "max" => Ok(Self::Max),
            other => Err(format!("reasoning effort must be low, high or max, got {other:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_enable_every_capability() {
        let config = DeepSeekConfig::new("key");
        assert_eq!(config.model, "deepseek-flash");
        assert_eq!(config.thinking, ThinkingMode::Enabled);
        assert_eq!(config.reasoning_effort, ReasoningEffort::Max);
        assert_eq!(config.max_tokens, None);
    }

    #[test]
    fn parses_settings_from_text() {
        assert_eq!("MAX".parse::<ReasoningEffort>(), Ok(ReasoningEffort::Max));
        assert_eq!("disabled".parse::<ThinkingMode>(), Ok(ThinkingMode::Disabled));
        assert!("medium".parse::<ReasoningEffort>().is_err());
    }
}
