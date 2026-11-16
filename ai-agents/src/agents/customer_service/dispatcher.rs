use std::sync::Arc;
use std::time::Instant;

use tracing::Instrument;

use super::domain::IncomingMessageEvent;
use super::turn::Turn;
use crate::pipeline::{Outcome, Pipeline};

/// Recibe los avisos y corre un turno por cada uno, sin bloquear la
/// respuesta HTTP al bot.
pub trait TurnDispatcher: Send + Sync {
    fn dispatch(&self, event: IncomingMessageEvent);
}

/// Corre cada turno en una tarea de tokio.
pub struct SpawningDispatcher {
    pipeline: Arc<Pipeline<Turn>>,
}

impl SpawningDispatcher {
    pub fn new(pipeline: Arc<Pipeline<Turn>>) -> Self {
        Self { pipeline }
    }
}

impl TurnDispatcher for SpawningDispatcher {
    fn dispatch(&self, event: IncomingMessageEvent) {
        let span = tracing::info_span!("turn", chat = %event.chat, message_id = event.message_id);
        tokio::spawn(run_turn(self.pipeline.clone(), event).instrument(span));
    }
}

async fn run_turn(pipeline: Arc<Pipeline<Turn>>, event: IncomingMessageEvent) {
    let started = Instant::now();
    let mut turn = Turn::new(event);
    match pipeline.run(&mut turn).await {
        Ok(Outcome::Completed) => {
            tracing::info!(elapsed_ms = started.elapsed().as_millis() as u64, "turn completed")
        }
        Ok(Outcome::Halted { step, reason }) => {
            tracing::info!(step, %reason, elapsed_ms = started.elapsed().as_millis() as u64, "turn halted")
        }
        Err(failure) => tracing::error!(step = failure.step, error = %failure.source, "turn failed"),
    }
}
