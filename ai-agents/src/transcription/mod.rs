//! Puerto de transcripción de audio ([`Transcriber`]) y sus adaptadores.

pub mod gemini;
mod transcriber;

pub use transcriber::{AudioClip, Transcriber, TranscriptionError, TranscriptionHints};
