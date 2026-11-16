/// Mensaje de una conversación con un modelo de chat, independiente del
/// proveedor.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatMessage {
    System(String),
    User(String),
    Assistant(AssistantMessage),
    /// Resultado de una herramienta, ligado a la llamada que lo pidió.
    ToolResult {
        call_id: String,
        content: String,
    },
}

/// Respuesta del modelo.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AssistantMessage {
    pub content: Option<String>,
    /// Razonamiento (chain of thought) que devolvió el modelo.
    ///
    /// Se conserva junto al mensaje y se reenvía en las siguientes
    /// peticiones: DeepSeek en modo thinking con herramientas responde 400
    /// ("reasoning_content ... must be passed back") si falta.
    pub reasoning: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

/// Herramienta que el modelo pidió ejecutar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Argumentos en JSON, tal cual los generó el modelo (pueden venir
    /// mal formados: quien ejecuta la herramienta los valida).
    pub arguments: String,
}

impl AssistantMessage {
    pub fn wants_tools(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}
