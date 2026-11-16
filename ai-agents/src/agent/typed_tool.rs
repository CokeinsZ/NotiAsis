use async_trait::async_trait;
use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde::de::DeserializeOwned;

use super::tool::{Tool, ToolError};
use crate::llm::ToolSpec;

/// Herramienta con argumentos tipados.
///
/// El JSON Schema que ve el modelo se genera de `Args` (los comentarios
/// `///` de cada campo se vuelven su descripción) y los argumentos se
/// validan al deserializarlos. Agregar un parámetro es agregar un campo.
#[async_trait]
pub trait TypedTool<C: Sync>: Send + Sync {
    type Args: DeserializeOwned + JsonSchema + Send;

    /// Nombre con el que el modelo llama la herramienta.
    const NAME: &'static str;
    /// Cuándo y para qué usarla (lo lee el modelo).
    const DESCRIPTION: &'static str;

    async fn call(&self, context: &C, args: Self::Args) -> Result<serde_json::Value, ToolError>;
}

#[async_trait]
impl<C, T> Tool<C> for T
where
    C: Sync,
    T: TypedTool<C>,
{
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: T::NAME.to_string(),
            description: T::DESCRIPTION.to_string(),
            parameters: parameters_schema::<T::Args>(),
        }
    }

    async fn invoke(&self, context: &C, arguments: &str) -> Result<serde_json::Value, ToolError> {
        // Algunos modelos mandan "" cuando la herramienta no tiene argumentos.
        let arguments = if arguments.trim().is_empty() { "{}" } else { arguments };
        let args: T::Args =
            serde_json::from_str(arguments).map_err(|error| ToolError::InvalidArguments(error.to_string()))?;
        self.call(context, args).await
    }
}

/// JSON Schema de los argumentos de una herramienta, autocontenido (sin
/// `$ref` ni `$defs`, que no todos los proveedores entienden) y sin
/// metadatos que el modelo no necesita.
pub fn parameters_schema<A: JsonSchema>() -> serde_json::Value {
    let schema = SchemaSettings::draft2020_12()
        .with(|settings| {
            settings.inline_subschemas = true;
            settings.meta_schema = None;
        })
        .into_generator()
        .into_root_schema_for::<A>();
    let mut value = schema.to_value();
    if let Some(object) = value.as_object_mut() {
        object.remove("title");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    enum Mood {
        Happy,
        Sad,
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    struct EchoArgs {
        /// Texto a repetir.
        text: String,
        /// Ánimo opcional.
        #[serde(default)]
        mood: Option<Mood>,
    }

    struct Echo;

    #[async_trait]
    impl TypedTool<()> for Echo {
        type Args = EchoArgs;
        const NAME: &'static str = "echo";
        const DESCRIPTION: &'static str = "Repite el texto.";

        async fn call(&self, _: &(), args: EchoArgs) -> Result<serde_json::Value, ToolError> {
            Ok(serde_json::json!({ "text": args.text, "mood": format!("{:?}", args.mood) }))
        }
    }

    #[test]
    fn schema_is_self_contained_with_descriptions() {
        let spec = Echo.spec();
        let schema = spec.parameters;
        assert_eq!(spec.name, "echo");
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["required"], serde_json::json!(["text"]));
        assert_eq!(schema["properties"]["text"]["description"], "Texto a repetir.");
        let text = schema.to_string();
        assert!(!text.contains("$ref") && !text.contains("$defs") && !text.contains("$schema"), "{text}");
        assert!(text.contains("happy") && text.contains("sad"), "{text}");
    }

    #[tokio::test]
    async fn invalid_arguments_are_reported_not_panicked() {
        let error = Echo.invoke(&(), "{\"mood\": \"angry\"}").await.unwrap_err();
        assert!(matches!(error, ToolError::InvalidArguments(_)));
        let output = Echo.invoke(&(), "{\"text\": \"hola\", \"mood\": \"happy\"}").await.unwrap();
        assert_eq!(output["text"], "hola");
    }
}
