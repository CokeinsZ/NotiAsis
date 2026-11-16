//! Composition root del agente: construye repositorios, herramientas,
//! reglas y pasos, y los conecta. Es el único lugar que conoce las
//! implementaciones concretas.

use std::sync::Arc;

use axum::Router;
use sqlx::PgPool;
use tokio::sync::Semaphore;

use super::config::CustomerServiceConfig;
use super::controller;
use super::delivery::{BackendReplyDelivery, DryRunReplyDelivery};
use super::dispatcher::{SpawningDispatcher, TurnDispatcher};
use super::eligibility::EligibilityGate;
use super::policy::ReplyPolicy;
use super::ports::ReplyDelivery;
use super::prompts::PromptBuilder;
use super::repositories::{
    PgChatStateReader, PgCustomerContextReader, PgEscalationWriter, PgGuideReader, PgGuideUpdateWriter,
    PgTranscriptReader, PgVoiceNoteStore,
};
use super::steps::{
    CheckEligibility, DeliverReply, GenerateReply, LoadCustomerContext, LoadTranscript, LockChat, ReserveCapacity,
    TranscribeVoiceNotes, WaitForSilence,
};
use super::tools::{ToolDeps, toolbox};
use super::turn::Turn;
use crate::agent::AgentRunner;
use crate::integrations::backend::{AiMessageSender, MediaDownloader};
use crate::integrations::dropi::OrderLookup;
use crate::llm::ChatModel;
use crate::pipeline::Pipeline;
use crate::shared::keyed_lock::KeyedLocks;
use crate::transcription::Transcriber;

/// Servicios externos que el agente necesita.
#[derive(Clone)]
pub struct CustomerServiceDeps {
    pub pool: PgPool,
    pub chat_model: Arc<dyn ChatModel>,
    pub transcriber: Arc<dyn Transcriber>,
    pub media: Arc<dyn MediaDownloader>,
    pub messages: Arc<dyn AiMessageSender>,
    pub orders: Arc<dyn OrderLookup>,
}

/// Rutas del agente (`/events`), listas para montar.
pub fn router(config: &CustomerServiceConfig, deps: CustomerServiceDeps) -> Router {
    tracing::info!(
        model = deps.chat_model.name(),
        dry_run = config.dry_run,
        allowlist = config.allowed_phones.len(),
        "customer service agent ready"
    );
    let dispatcher: Arc<dyn TurnDispatcher> = Arc::new(SpawningDispatcher::new(Arc::new(build_pipeline(config, deps))));
    controller::routes(dispatcher)
}

/// Los pasos del turno, en orden.
pub fn build_pipeline(config: &CustomerServiceConfig, deps: CustomerServiceDeps) -> Pipeline<Turn> {
    let pool = deps.pool;
    let guides = Arc::new(PgGuideReader::new(pool.clone()));
    let gate =
        Arc::new(EligibilityGate::new(Arc::new(PgChatStateReader::new(pool.clone())), ReplyPolicy::standard(config)));
    let tools = toolbox(ToolDeps {
        guides,
        guide_updates: Arc::new(PgGuideUpdateWriter::new(pool.clone())),
        escalations: Arc::new(PgEscalationWriter::new(pool.clone())),
        orders: deps.orders,
    });
    let runner = Arc::new(AgentRunner::new(deps.chat_model, tools, config.max_tool_rounds));
    let delivery: Arc<dyn ReplyDelivery> =
        if config.dry_run { Arc::new(DryRunReplyDelivery) } else { Arc::new(BackendReplyDelivery::new(deps.messages)) };

    Pipeline::new()
        .then(WaitForSilence::new(config.quiet_period))
        .then(LockChat::new(Arc::new(KeyedLocks::new())))
        .then(CheckEligibility::new(gate.clone()))
        .then(ReserveCapacity::new(Arc::new(Semaphore::new(config.max_concurrent_turns.max(1)))))
        .then(LoadCustomerContext::new(Arc::new(PgCustomerContextReader::new(pool.clone())), config.guide_limit))
        .then(TranscribeVoiceNotes::new(
            Arc::new(PgVoiceNoteStore::new(pool.clone())),
            deps.media,
            deps.transcriber,
            config.base_vocabulary.clone(),
            config.max_voice_notes_per_turn,
        ))
        .then(LoadTranscript::new(Arc::new(PgTranscriptReader::new(pool)), config.history_limit))
        .then(GenerateReply::new(runner, Arc::new(PromptBuilder::standard())))
        .then(DeliverReply::new(gate, delivery))
}
