use async_trait::async_trait;

use super::config::DeepSeekConfig;
use super::wire::{WireResponse, build_request, parse_response};
use crate::llm::{ChatCompletion, ChatModel, ChatRequest, LlmError};
use crate::shared::http_json::{HttpError, send_json};

/// Modelo de chat de DeepSeek.
pub struct DeepSeekChatModel {
    http: reqwest::Client,
    config: DeepSeekConfig,
}

impl DeepSeekChatModel {
    pub fn new(config: DeepSeekConfig) -> Result<Self, LlmError> {
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|error| LlmError::Configuration(error.to_string()))?;
        Ok(Self { http, config })
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'))
    }
}

#[async_trait]
impl ChatModel for DeepSeekChatModel {
    fn name(&self) -> &str {
        &self.config.model
    }

    async fn complete(&self, request: ChatRequest<'_>) -> Result<ChatCompletion, LlmError> {
        let body = build_request(&self.config, &request);
        let endpoint = self.endpoint();
        let response: WireResponse = self
            .config
            .retry
            .run(
                || send_json(self.http.post(&endpoint).bearer_auth(&self.config.api_key).json(&body)),
                HttpError::is_retryable,
            )
            .await?;

        let completion = parse_response(response)?;
        tracing::debug!(
            model = %self.config.model,
            finish_reason = ?completion.finish_reason,
            prompt_tokens = completion.usage.prompt_tokens,
            cached_prompt_tokens = completion.usage.cached_prompt_tokens,
            completion_tokens = completion.usage.completion_tokens,
            reasoning_tokens = completion.usage.reasoning_tokens,
            tool_calls = completion.message.tool_calls.len(),
            "deepseek completion"
        );
        Ok(completion)
    }
}
