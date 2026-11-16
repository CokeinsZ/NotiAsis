use crate::llm::{AssistantMessage, ChatMessage};

/// Historial de mensajes que se le envía al modelo en cada ronda.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Conversation {
    messages: Vec<ChatMessage>,
}

impl Conversation {
    pub fn new(system_prompt: impl Into<String>, user_prompt: impl Into<String>) -> Self {
        Self { messages: vec![ChatMessage::System(system_prompt.into()), ChatMessage::User(user_prompt.into())] }
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    /// Agrega la respuesta del modelo COMPLETA, con su razonamiento, para
    /// reenviarla tal cual en la siguiente ronda.
    pub fn push_assistant(&mut self, message: AssistantMessage) {
        self.messages.push(ChatMessage::Assistant(message));
    }

    pub fn push_tool_result(&mut self, call_id: impl Into<String>, content: impl Into<String>) {
        self.messages.push(ChatMessage::ToolResult { call_id: call_id.into(), content: content.into() });
    }
}
