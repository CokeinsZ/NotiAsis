use async_trait::async_trait;

use crate::shared::http_json::HttpError;

/// Audio a transcribir, en memoria.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioClip {
    pub bytes: Vec<u8>,
    /// MIME del audio, con o sin parámetros (`audio/ogg; codecs=opus`).
    pub mime_type: String,
}

impl AudioClip {
    /// MIME sin parámetros (`audio/ogg; codecs=opus` → `audio/ogg`).
    pub fn base_mime_type(&self) -> &str {
        self.mime_type.split(';').next().unwrap_or_default().trim()
    }
}

/// Pistas para mejorar la transcripción.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscriptionHints {
    /// Palabras que probablemente aparecen en el audio (productos,
    /// transportadoras, ciudades...).
    pub vocabulary: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum TranscriptionError {
    #[error("transcription HTTP error: {0}")]
    Http(#[from] HttpError),
    #[error("the transcription came back empty")]
    Empty,
    #[error("transcription client configuration error: {0}")]
    Configuration(String),
}

/// Convierte audio en texto.
#[async_trait]
pub trait Transcriber: Send + Sync {
    async fn transcribe(&self, clip: &AudioClip, hints: &TranscriptionHints) -> Result<String, TranscriptionError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_mime_type_drops_codec_parameters() {
        let clip = AudioClip { bytes: vec![], mime_type: "audio/ogg; codecs=opus".into() };
        assert_eq!(clip.base_mime_type(), "audio/ogg");
    }
}
