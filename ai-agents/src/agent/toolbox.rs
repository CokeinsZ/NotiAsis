use std::sync::Arc;

use super::tool::{Tool, ToolError};
use crate::llm::{ToolCall, ToolSpec};

/// Herramientas disponibles para un agente.
pub struct ToolBox<C> {
    tools: Vec<Arc<dyn Tool<C>>>,
}

impl<C: Sync> ToolBox<C> {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Agrega una herramienta.
    ///
    /// # Panics
    /// Si ya hay una herramienta con el mismo nombre (error de
    /// programación: se detecta al arrancar, no en producción).
    pub fn with(mut self, tool: impl Tool<C> + 'static) -> Self {
        let name = tool.spec().name;
        assert!(self.tools.iter().all(|existing| existing.spec().name != name), "duplicate tool name: {name}");
        self.tools.push(Arc::new(tool));
        self
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.iter().map(|tool| tool.spec()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Ejecuta la llamada y devuelve el texto para el modelo. Nunca falla:
    /// los errores (herramienta desconocida, argumentos inválidos, fallas)
    /// se le devuelven al modelo como `{"error": ...}`.
    pub async fn invoke(&self, context: &C, call: &ToolCall) -> String {
        let result = match self.tools.iter().find(|tool| tool.spec().name == call.name) {
            Some(tool) => tool.invoke(context, &call.arguments).await,
            None => Err(ToolError::Failed(format!("la herramienta {} no existe", call.name))),
        };
        match result {
            Ok(output) => output.to_string(),
            Err(error) => {
                tracing::warn!(tool = %call.name, %error, "tool call failed");
                serde_json::json!({ "error": error.to_string() }).to_string()
            }
        }
    }
}

impl<C: Sync> Default for ToolBox<C> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct Constant(&'static str);

    #[async_trait]
    impl Tool<()> for Constant {
        fn spec(&self) -> ToolSpec {
            ToolSpec { name: self.0.into(), description: String::new(), parameters: serde_json::json!({}) }
        }

        async fn invoke(&self, _: &(), _: &str) -> Result<serde_json::Value, ToolError> {
            Ok(serde_json::json!({ "from": self.0 }))
        }
    }

    fn call(name: &str) -> ToolCall {
        ToolCall { id: "1".into(), name: name.into(), arguments: "{}".into() }
    }

    #[tokio::test]
    async fn dispatches_by_name_and_reports_unknown_tools() {
        let toolbox = ToolBox::new().with(Constant("a")).with(Constant("b"));
        assert_eq!(toolbox.invoke(&(), &call("b")).await, r#"{"from":"b"}"#);
        assert!(toolbox.invoke(&(), &call("zzz")).await.contains("no existe"));
    }

    #[test]
    #[should_panic(expected = "duplicate tool name")]
    fn duplicate_names_are_rejected() {
        let _ = ToolBox::new().with(Constant("a")).with(Constant("a"));
    }
}
