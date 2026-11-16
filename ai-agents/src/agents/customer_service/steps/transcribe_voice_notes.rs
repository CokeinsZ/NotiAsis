use std::sync::Arc;

use async_trait::async_trait;

use crate::agents::customer_service::domain::PendingVoiceNote;
use crate::agents::customer_service::ports::VoiceNoteStore;
use crate::agents::customer_service::turn::Turn;
use crate::integrations::backend::MediaDownloader;
use crate::pipeline::{Flow, Step};
use crate::shared::error::BoxError;
use crate::transcription::{AudioClip, Transcriber, TranscriptionHints};

/// Transcribe las notas de voz pendientes del cliente y guarda el texto en
/// `messages.transcription` (el historial lo muestra y no se repite). Si
/// una falla, el turno sigue: el historial dirá "(sin transcripción)".
pub struct TranscribeVoiceNotes {
    notes: Arc<dyn VoiceNoteStore>,
    media: Arc<dyn MediaDownloader>,
    transcriber: Arc<dyn Transcriber>,
    base_vocabulary: Vec<String>,
    max_per_turn: i64,
}

impl TranscribeVoiceNotes {
    pub fn new(
        notes: Arc<dyn VoiceNoteStore>,
        media: Arc<dyn MediaDownloader>,
        transcriber: Arc<dyn Transcriber>,
        base_vocabulary: Vec<String>,
        max_per_turn: i64,
    ) -> Self {
        Self { notes, media, transcriber, base_vocabulary, max_per_turn }
    }

    async fn transcribe(&self, note: &PendingVoiceNote, hints: &TranscriptionHints) -> Result<String, BoxError> {
        let media = self.media.download(&note.media_id).await?;
        let clip = AudioClip { bytes: media.bytes, mime_type: media.mime_type };
        Ok(self.transcriber.transcribe(&clip, hints).await?)
    }
}

#[async_trait]
impl Step<Turn> for TranscribeVoiceNotes {
    fn name(&self) -> &'static str {
        "transcribe_voice_notes"
    }

    async fn run(&self, turn: &mut Turn) -> Result<Flow, BoxError> {
        let pending = self.notes.pending(&turn.event.chat, self.max_per_turn).await?;
        if pending.is_empty() {
            return Ok(Flow::Continue);
        }

        // Las palabras del cliente primero: son las más probables.
        let mut vocabulary = turn.context()?.vocabulary();
        vocabulary.extend(self.base_vocabulary.iter().cloned());
        let hints = TranscriptionHints { vocabulary };

        for note in &pending {
            match self.transcribe(note, &hints).await {
                Ok(text) => self.notes.save_transcription(note.message_id, &text).await?,
                Err(error) => {
                    tracing::warn!(chat = %turn.event.chat, message_id = note.message_id, %error, "voice note not transcribed")
                }
            }
        }
        Ok(Flow::Continue)
    }
}
