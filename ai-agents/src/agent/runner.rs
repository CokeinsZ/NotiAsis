use std::sync::Arc;

use super::conversation::Conversation;
use super::toolbox::ToolBox;
use crate::llm::{ChatModel, ChatRequest, FinishReason, LlmError, TokenUsage};

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error(transparent)]
    Llm(#[from] LlmError),
    #[error("the model kept calling tools after {0} rounds")]
    TooManyRounds(usize),
    #[error("the model stopped before finishing: {0:?}")]
    Incomplete(FinishReason),
}

/// Resultado de una ejecución del agente.
#[derive(Debug, Clone)]
pub struct AgentRun {
    /// Texto final del modelo (`None` si no escribió nada).
    pub reply: Option<String>,
    /// Rondas usadas (una por cada llamada al modelo).
    pub rounds: usize,
    pub usage: TokenUsage,
    /// Conversación completa, con razonamientos y herramientas (para logs).
    pub conversation: Conversation,
}

/// Bucle del agente: llama al modelo, ejecuta las herramientas que pida y
/// repite hasta que responda sin herramientas.
pub struct AgentRunner<C> {
    model: Arc<dyn ChatModel>,
    tools: ToolBox<C>,
    max_rounds: usize,
}

impl<C: Sync> AgentRunner<C> {
    pub fn new(model: Arc<dyn ChatModel>, tools: ToolBox<C>, max_rounds: usize) -> Self {
        Self { model, tools, max_rounds: max_rounds.max(1) }
    }

    pub async fn run(&self, context: &C, mut conversation: Conversation) -> Result<AgentRun, AgentError> {
        let specs = self.tools.specs();
        let mut usage = TokenUsage::default();

        for round in 1..=self.max_rounds {
            let completion =
                self.model.complete(ChatRequest { messages: conversation.messages(), tools: &specs }).await?;
            usage.add(&completion.usage);
            if !completion.finish_reason.is_complete() {
                return Err(AgentError::Incomplete(completion.finish_reason));
            }

            let message = completion.message;
            let calls = message.tool_calls.clone();
            let reply = message.content.clone();
            // El mensaje va completo (con `reasoning`): DeepSeek exige
            // recibir de vuelta el razonamiento de cada ronda.
            conversation.push_assistant(message);

            if calls.is_empty() {
                return Ok(AgentRun { reply, rounds: round, usage, conversation });
            }
            for call in &calls {
                tracing::debug!(tool = %call.name, arguments = %call.arguments, "tool call");
                let output = self.tools.invoke(context, call).await;
                conversation.push_tool_result(&call.id, output);
            }
        }
        Err(AgentError::TooManyRounds(self.max_rounds))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Tool, ToolError};
    use crate::llm::{AssistantMessage, ChatCompletion, ChatMessage, ToolCall, ToolSpec};
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// Modelo falso: devuelve respuestas en orden y guarda cada petición.
    struct ScriptedModel {
        replies: Mutex<Vec<ChatCompletion>>,
        requests: Mutex<Vec<Vec<ChatMessage>>>,
    }

    impl ScriptedModel {
        fn new(mut replies: Vec<ChatCompletion>) -> Arc<Self> {
            replies.reverse();
            Arc::new(Self { replies: Mutex::new(replies), requests: Mutex::new(Vec::new()) })
        }
    }

    #[async_trait]
    impl ChatModel for ScriptedModel {
        fn name(&self) -> &str {
            "scripted"
        }

        async fn complete(&self, request: ChatRequest<'_>) -> Result<ChatCompletion, LlmError> {
            self.requests.lock().unwrap().push(request.messages.to_vec());
            Ok(self.replies.lock().unwrap().pop().expect("no more scripted replies"))
        }
    }

    struct Lookup;

    #[async_trait]
    impl Tool<&'static str> for Lookup {
        fn spec(&self) -> ToolSpec {
            ToolSpec { name: "lookup".into(), description: String::new(), parameters: serde_json::json!({}) }
        }

        async fn invoke(&self, context: &&'static str, _: &str) -> Result<serde_json::Value, ToolError> {
            Ok(serde_json::json!({ "chat": *context }))
        }
    }

    fn tool_call_reply() -> ChatCompletion {
        ChatCompletion {
            message: AssistantMessage {
                content: None,
                reasoning: Some("necesito la guía".into()),
                tool_calls: vec![ToolCall { id: "c1".into(), name: "lookup".into(), arguments: "{}".into() }],
            },
            finish_reason: FinishReason::ToolCalls,
            usage: TokenUsage { prompt_tokens: 10, ..Default::default() },
        }
    }

    fn final_reply(text: &str) -> ChatCompletion {
        ChatCompletion {
            message: AssistantMessage {
                content: Some(text.into()),
                reasoning: Some("listo".into()),
                ..Default::default()
            },
            finish_reason: FinishReason::Stop,
            usage: TokenUsage { prompt_tokens: 20, ..Default::default() },
        }
    }

    #[tokio::test]
    async fn keeps_reasoning_and_tool_results_for_the_next_round() {
        let model = ScriptedModel::new(vec![tool_call_reply(), final_reply("Tu pedido está en oficina")]);
        let runner = AgentRunner::new(model.clone(), ToolBox::new().with(Lookup), 4);

        let run = runner.run(&"chat-1", Conversation::new("sys", "user")).await.unwrap();
        assert_eq!(run.reply.as_deref(), Some("Tu pedido está en oficina"));
        assert_eq!(run.rounds, 2);
        assert_eq!(run.usage.prompt_tokens, 30);

        let requests = model.requests.lock().unwrap();
        let second = &requests[1];
        // La segunda petición lleva la respuesta anterior con su razonamiento.
        assert!(matches!(&second[2], ChatMessage::Assistant(m) if m.reasoning.as_deref() == Some("necesito la guía")));
        assert_eq!(second[3], ChatMessage::ToolResult { call_id: "c1".into(), content: r#"{"chat":"chat-1"}"#.into() });
    }

    #[tokio::test]
    async fn stops_after_max_rounds() {
        let model = ScriptedModel::new(vec![tool_call_reply(), tool_call_reply()]);
        let runner = AgentRunner::new(model, ToolBox::new().with(Lookup), 2);
        let error = runner.run(&"chat", Conversation::new("s", "u")).await.unwrap_err();
        assert!(matches!(error, AgentError::TooManyRounds(2)));
    }

    #[tokio::test]
    async fn truncated_responses_are_errors() {
        let mut reply = final_reply("cortad");
        reply.finish_reason = FinishReason::Length;
        let runner = AgentRunner::new(ScriptedModel::new(vec![reply]), ToolBox::new().with(Lookup), 2);
        let error = runner.run(&"chat", Conversation::new("s", "u")).await.unwrap_err();
        assert!(matches!(error, AgentError::Incomplete(FinishReason::Length)));
    }
}
