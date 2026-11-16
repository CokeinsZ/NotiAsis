use async_trait::async_trait;

use super::config::GeminiTranscribeConfig;
use super::wire::{WireResponse, build_request, transcript_text};
use crate::shared::http_json::{HttpError, send_json};
use crate::transcription::{AudioClip, Transcriber, TranscriptionError, TranscriptionHints};

/// Transcriptor de audio con Gemini 3.5 Transcribe.
pub struct GeminiTranscriber {
    http: reqwest::Client,
    config: GeminiTranscribeConfig,
}

impl GeminiTranscriber {
    pub fn new(config: GeminiTranscribeConfig) -> Result<Self, TranscriptionError> {
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|error| TranscriptionError::Configuration(error.to_string()))?;
        Ok(Self { http, config })
    }

    fn endpoint(&self) -> String {
        format!("{}/models/{}:generateContent", self.config.base_url.trim_end_matches('/'), self.config.model)
    }
}

#[async_trait]
impl Transcriber for GeminiTranscriber {
    async fn transcribe(&self, clip: &AudioClip, hints: &TranscriptionHints) -> Result<String, TranscriptionError> {
        let body = build_request(&self.config, clip, hints);
        let endpoint = self.endpoint();
        let response: WireResponse = self
            .config
            .retry
            .run(
                || send_json(self.http.post(&endpoint).header("x-goog-api-key", &self.config.api_key).json(&body)),
                HttpError::is_retryable,
            )
            .await?;

        let text = transcript_text(response);
        if text.is_empty() {
            return Err(TranscriptionError::Empty);
        }
        Ok(text)
    }
}
