//! Adaptador de DeepSeek (API compatible con OpenAI, `POST /chat/completions`).
//!
//! - `config`: parámetros del modelo (thinking, esfuerzo de razonamiento...).
//! - `wire`: formato JSON de la API y su traducción a los tipos de `llm`.
//! - `client`: la llamada HTTP.

mod client;
mod config;
mod wire;

pub use client::DeepSeekChatModel;
pub use config::{DeepSeekConfig, ReasoningEffort, ThinkingMode};
