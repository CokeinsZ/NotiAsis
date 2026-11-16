use async_trait::async_trait;

use crate::llm::message::{AssistantMessage, ChatMessage};
use crate::shared::http_json::HttpError;

/// Herramienta que se le ofrece al modelo: nombre, descripción y JSON
/// Schema de sus argumentos.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Petición a un modelo de chat.
#[derive(Debug, Clone, Copy)]
pub struct ChatRequest<'a> {
    pub messages: &'a [ChatMessage],
    pub tools: &'a [ToolSpec],
}

/// Por qué el modelo dejó de generar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    ToolCalls,
    /// Se acabaron los tokens de salida: la respuesta quedó cortada.
    Length,
    ContentFilter,
    /// El proveedor no tuvo capacidad para terminar.
    InsufficientResources,
    Other(String),
}

impl FinishReason {
    /// Si la respuesta está completa y se puede usar.
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Stop | Self::ToolCalls)
    }
}

/// Consumo de tokens (para logs y costos).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub cached_prompt_tokens: u64,
    pub completion_tokens: u64,
    pub reasoning_tokens: u64,
}

impl TokenUsage {
    pub fn add(&mut self, other: &TokenUsage) {
        self.prompt_tokens += other.prompt_tokens;
        self.cached_prompt_tokens += other.cached_prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.reasoning_tokens += other.reasoning_tokens;
    }
}

/// Respuesta de un modelo de chat.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatCompletion {
    pub message: AssistantMessage,
    pub finish_reason: FinishReason,
    pub usage: TokenUsage,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("LLM HTTP error: {0}")]
    Http(#[from] HttpError),
    #[error("LLM returned no choices")]
    EmptyResponse,
    #[error("LLM client configuration error: {0}")]
    Configuration(String),
}

/// Modelo de chat con soporte de herramientas (function calling).
#[async_trait]
pub trait ChatModel: Send + Sync {
    /// Nombre del modelo (para logs).
    fn name(&self) -> &str;

    async fn complete(&self, request: ChatRequest<'_>) -> Result<ChatCompletion, LlmError>;
}
