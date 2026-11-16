use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::AgentRunner;
use crate::agents::customer_service::prompts::PromptBuilder;
use crate::agents::customer_service::reply::AgentReply;
use crate::agents::customer_service::tools::CustomerToolContext;
use crate::agents::customer_service::turn::Turn;
use crate::pipeline::{Flow, Step};
use crate::shared::error::BoxError;

/// El agente lee el contexto y el historial, usa sus herramientas y
/// escribe la respuesta (o decide no responder).
pub struct GenerateReply {
    runner: Arc<AgentRunner<CustomerToolContext>>,
    prompts: Arc<PromptBuilder>,
}

impl GenerateReply {
    pub fn new(runner: Arc<AgentRunner<CustomerToolContext>>, prompts: Arc<PromptBuilder>) -> Self {
        Self { runner, prompts }
    }
}

#[async_trait]
impl Step<Turn> for GenerateReply {
    fn name(&self) -> &'static str {
        "generate_reply"
    }

    async fn run(&self, turn: &mut Turn) -> Result<Flow, BoxError> {
        let conversation = self.prompts.conversation(turn.context()?, &turn.transcript)?;
        let tool_context = CustomerToolContext { chat: turn.event.chat.clone() };
        let run = self.runner.run(&tool_context, conversation).await?;
        turn.usage = run.usage;
        tracing::info!(
            chat = %turn.event.chat,
            rounds = run.rounds,
            prompt_tokens = run.usage.prompt_tokens,
            cached_prompt_tokens = run.usage.cached_prompt_tokens,
            completion_tokens = run.usage.completion_tokens,
            reasoning_tokens = run.usage.reasoning_tokens,
            "reply generated"
        );

        Ok(match AgentReply::from_model_output(run.reply.as_deref()) {
            AgentReply::Send(text) => {
                turn.reply = Some(text);
                Flow::Continue
            }
            AgentReply::Silent => Flow::Halt("el agente decidió no responder".into()),
        })
    }
}
