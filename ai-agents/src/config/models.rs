use super::env::{ConfigError, Env};
use crate::llm::deepseek::{DeepSeekConfig, ReasoningEffort, ThinkingMode};
use crate::transcription::gemini::{GeminiTranscribeConfig, TranscriptionMode};

/// DeepSeek. Por defecto: `deepseek-flash`, thinking activo y esfuerzo `max`.
pub fn deepseek(env: &Env) -> Result<DeepSeekConfig, ConfigError> {
    let defaults = DeepSeekConfig::new(env.required("DEEPSEEK_API_KEY")?);
    Ok(DeepSeekConfig {
        base_url: env.or("DEEPSEEK_BASE_URL", DeepSeekConfig::DEFAULT_BASE_URL),
        model: env.or("DEEPSEEK_MODEL", DeepSeekConfig::DEFAULT_MODEL),
        thinking: env.parse::<ThinkingMode>("DEEPSEEK_THINKING", defaults.thinking)?,
        reasoning_effort: env.parse::<ReasoningEffort>("DEEPSEEK_REASONING_EFFORT", defaults.reasoning_effort)?,
        max_tokens: env.parse_optional("DEEPSEEK_MAX_TOKENS")?,
        timeout: env.seconds("DEEPSEEK_TIMEOUT_SECS", defaults.timeout.as_secs())?,
        ..defaults
    })
}

/// Gemini 3.5 Transcribe. Por defecto: español de Latinoamérica, modo smart.
pub fn gemini(env: &Env) -> Result<GeminiTranscribeConfig, ConfigError> {
    let defaults = GeminiTranscribeConfig::new(env.required("GOOGLE_AI_API_KEY")?);
    Ok(GeminiTranscribeConfig {
        base_url: env.or("GEMINI_BASE_URL", GeminiTranscribeConfig::DEFAULT_BASE_URL),
        model: env.or("GEMINI_TRANSCRIBE_MODEL", GeminiTranscribeConfig::DEFAULT_MODEL),
        language_codes: env.list_or("GEMINI_TRANSCRIBE_LANGUAGES", &defaults.language_codes),
        mode: env.parse::<TranscriptionMode>("GEMINI_TRANSCRIBE_MODE", defaults.mode)?,
        timeout: env.seconds("GEMINI_TIMEOUT_SECS", defaults.timeout.as_secs())?,
        ..defaults
    })
}
