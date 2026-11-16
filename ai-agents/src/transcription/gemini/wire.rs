//! Formato JSON de `generateContent` para Gemini 3.5 Transcribe.
//!
//! Petición: el audio va inline en base64 (`inlineData`) y la
//! configuración en `generationConfig.audioTranscriptionConfig`.
//! Respuesta: el texto llega en `candidates[0].content.parts[].text`.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

use super::config::{GeminiTranscribeConfig, TranscriptionMode};
use crate::transcription::{AudioClip, TranscriptionHints};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireRequest {
    contents: Vec<WireContent>,
    generation_config: WireGenerationConfig,
}

#[derive(Debug, Serialize)]
struct WireContent {
    parts: Vec<WirePart>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WirePart {
    inline_data: WireInlineData,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireInlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireGenerationConfig {
    audio_transcription_config: WireTranscriptionConfig,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireTranscriptionConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    language_codes: Vec<String>,
    mode: TranscriptionMode,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    custom_vocabulary: Vec<String>,
}

pub fn build_request(config: &GeminiTranscribeConfig, clip: &AudioClip, hints: &TranscriptionHints) -> WireRequest {
    WireRequest {
        contents: vec![WireContent {
            parts: vec![WirePart {
                inline_data: WireInlineData {
                    mime_type: clip.base_mime_type().to_string(),
                    data: BASE64.encode(&clip.bytes),
                },
            }],
        }],
        generation_config: WireGenerationConfig {
            audio_transcription_config: WireTranscriptionConfig {
                language_codes: config.language_codes.clone(),
                mode: config.mode,
                custom_vocabulary: vocabulary(hints, config.max_vocabulary),
            },
        },
    }
}

/// Vocabulario sin vacíos ni repetidos (sin importar mayúsculas), hasta
/// `limit` palabras en el orden recibido.
fn vocabulary(hints: &TranscriptionHints, limit: usize) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    hints
        .vocabulary
        .iter()
        .map(|term| term.trim())
        .filter(|term| !term.is_empty() && seen.insert(term.to_lowercase()))
        .take(limit)
        .map(str::to_string)
        .collect()
}

#[derive(Debug, Deserialize)]
pub struct WireResponse {
    #[serde(default)]
    candidates: Vec<WireCandidate>,
}

#[derive(Debug, Deserialize)]
struct WireCandidate {
    #[serde(default)]
    content: Option<WireResponseContent>,
}

#[derive(Debug, Deserialize)]
struct WireResponseContent {
    #[serde(default)]
    parts: Vec<WireResponsePart>,
}

#[derive(Debug, Deserialize)]
struct WireResponsePart {
    #[serde(default)]
    text: Option<String>,
}

/// Texto del primer candidato (todas sus partes, en orden).
pub fn transcript_text(response: WireResponse) -> String {
    response
        .candidates
        .into_iter()
        .next()
        .and_then(|candidate| candidate.content)
        .map(|content| content.parts.into_iter().filter_map(|part| part.text).collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_sends_inline_audio_and_transcription_config() {
        let config = GeminiTranscribeConfig::new("key");
        let clip = AudioClip { bytes: b"OggS".to_vec(), mime_type: "audio/ogg; codecs=opus".into() };
        let hints = TranscriptionHints {
            vocabulary: vec!["Interrapidísimo".into(), " ".into(), "interrapidísimo".into(), "Nequi".into()],
        };

        let body = serde_json::to_value(build_request(&config, &clip, &hints)).unwrap();
        assert_eq!(
            body,
            json!({
                "contents": [{"parts": [{"inlineData": {"mimeType": "audio/ogg", "data": "T2dnUw=="}}]}],
                "generationConfig": {"audioTranscriptionConfig": {
                    "languageCodes": ["es-419"],
                    "mode": "SMART",
                    "customVocabulary": ["Interrapidísimo", "Nequi"]
                }}
            })
        );
    }

    #[test]
    fn vocabulary_is_capped() {
        let mut config = GeminiTranscribeConfig::new("key");
        config.max_vocabulary = 1;
        let clip = AudioClip { bytes: vec![], mime_type: "audio/ogg".into() };
        let hints = TranscriptionHints { vocabulary: vec!["a".into(), "b".into()] };
        let body = serde_json::to_value(build_request(&config, &clip, &hints)).unwrap();
        assert_eq!(body["generationConfig"]["audioTranscriptionConfig"]["customVocabulary"], json!(["a"]));
    }

    #[test]
    fn joins_text_parts_of_the_first_candidate() {
        let response: WireResponse = serde_json::from_value(json!({
            "candidates": [{"content": {"parts": [{"text": "Mañana paso"}, {"text": "por el pedido."}]},
                            "finishReason": "STOP"}]
        }))
        .unwrap();
        assert_eq!(transcript_text(response), "Mañana paso por el pedido.");
    }

    #[test]
    fn missing_candidates_give_empty_text() {
        let response: WireResponse = serde_json::from_value(json!({})).unwrap();
        assert_eq!(transcript_text(response), "");
    }
}
