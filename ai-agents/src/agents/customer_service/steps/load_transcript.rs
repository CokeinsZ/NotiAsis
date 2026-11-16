use std::sync::Arc;

use async_trait::async_trait;

use crate::agents::customer_service::ports::TranscriptReader;
use crate::agents::customer_service::turn::Turn;
use crate::pipeline::{Flow, Step};
use crate::shared::error::BoxError;

/// Últimos mensajes del chat: notificaciones, asesores, el agente y el
/// cliente, desde la tabla `messages` (la misma que ve la webapp).
pub struct LoadTranscript {
    reader: Arc<dyn TranscriptReader>,
    limit: i64,
}

impl LoadTranscript {
    pub fn new(reader: Arc<dyn TranscriptReader>, limit: i64) -> Self {
        Self { reader, limit }
    }
}

#[async_trait]
impl Step<Turn> for LoadTranscript {
    fn name(&self) -> &'static str {
        "load_transcript"
    }

    async fn run(&self, turn: &mut Turn) -> Result<Flow, BoxError> {
        turn.transcript = self.reader.recent(&turn.event.chat, self.limit).await?;
        Ok(Flow::Continue)
    }
}
