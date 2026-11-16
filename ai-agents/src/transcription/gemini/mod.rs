//! Adaptador de Gemini 3.5 Transcribe (`models/gemini-3.5-transcribe:generateContent`).
//!
//! - `config`: modelo, idiomas y modo (verbatim o smart).
//! - `wire`: formato JSON de la API.
//! - `client`: la llamada HTTP.

mod client;
mod config;
mod wire;

pub use client::GeminiTranscriber;
pub use config::{GeminiTranscribeConfig, TranscriptionMode};
