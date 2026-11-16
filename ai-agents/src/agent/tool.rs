use async_trait::async_trait;

use crate::llm::ToolSpec;

/// Error de una herramienta. El runner no aborta: se lo devuelve al modelo
/// como resultado de la herramienta para que corrija o siga sin ella.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ToolError {
    /// El modelo mandó argumentos inválidos.
    #[error("argumentos inválidos: {0}")]
    InvalidArguments(String),
    /// La herramienta no pudo completar la operación.
    #[error("{0}")]
    Failed(String),
}

/// Herramienta que un agente puede ofrecerle al modelo.
///
/// Normalmente no se implementa directamente: se implementa
/// [`TypedTool`](super::TypedTool), que genera el esquema y valida los
/// argumentos a partir de un struct.
#[async_trait]
pub trait Tool<C: Sync>: Send + Sync {
    fn spec(&self) -> ToolSpec;

    /// Ejecuta la herramienta con los argumentos en JSON que generó el
    /// modelo. Devuelve el resultado en JSON para el modelo.
    async fn invoke(&self, context: &C, arguments: &str) -> Result<serde_json::Value, ToolError>;
}
