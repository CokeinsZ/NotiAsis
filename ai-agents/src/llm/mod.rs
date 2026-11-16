//! Puerto de los modelos de chat ([`ChatModel`]) y sus adaptadores.
//!
//! Para usar otro proveedor basta con agregar un submódulo que implemente
//! [`ChatModel`] y elegirlo en el composition root (`app.rs`): ningún
//! agente cambia.

pub mod deepseek;
mod message;
mod model;

pub use message::{AssistantMessage, ChatMessage, ToolCall};
pub use model::{ChatCompletion, ChatModel, ChatRequest, FinishReason, LlmError, TokenUsage, ToolSpec};
