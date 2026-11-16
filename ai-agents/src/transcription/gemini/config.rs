use std::str::FromStr;
use std::time::Duration;

use serde::Serialize;

use crate::shared::retry::RetryPolicy;

/// Modo de transcripción de Gemini 3.5 Transcribe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TranscriptionMode {
    /// Palabra por palabra, con muletillas y repeticiones.
    Verbatim,
    /// Texto limpio: quita muletillas, aplica autocorrecciones del
    /// hablante y da formato a cifras y monedas.
    Smart,
}

#[derive(Debug, Clone)]
pub struct GeminiTranscribeConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    /// Códigos BCP-47. Vacío = detección automática.
    pub language_codes: Vec<String>,
    pub mode: TranscriptionMode,
    /// Máximo de palabras de vocabulario por audio (la API admite 1.000;
    /// Google recomienda hasta ~100 para mejores resultados).
    pub max_vocabulary: usize,
    pub timeout: Duration,
    pub retry: RetryPolicy,
}

impl GeminiTranscribeConfig {
    pub const DEFAULT_BASE_URL: &'static str = "https://generativelanguage.googleapis.com/v1beta";
    pub const DEFAULT_MODEL: &'static str = "gemini-3.5-transcribe";
    /// Español de Latinoamérica (Colombia y Guatemala).
    pub const DEFAULT_LANGUAGE: &'static str = "es-419";

    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: Self::DEFAULT_BASE_URL.to_string(),
            model: Self::DEFAULT_MODEL.to_string(),
            language_codes: vec![Self::DEFAULT_LANGUAGE.to_string()],
            mode: TranscriptionMode::Smart,
            max_vocabulary: 100,
            timeout: Duration::from_secs(120),
            retry: RetryPolicy::default(),
        }
    }
}

impl FromStr for TranscriptionMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "verbatim" => Ok(Self::Verbatim),
            "smart" => Ok(Self::Smart),
            other => Err(format!("transcription mode must be verbatim or smart, got {other:?}")),
        }
    }
}
