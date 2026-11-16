use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use super::{CustomerToolContext, guide_not_found};
use crate::agent::{ToolError, TypedTool};
use crate::agents::customer_service::ports::GuideReader;
use crate::agents::customer_service::presentation::guides::guide_details_json;

/// `detalle_guia`: una guía del cliente con todos sus avisos.
pub struct GuideDetailsTool {
    guides: Arc<dyn GuideReader>,
}

impl GuideDetailsTool {
    pub fn new(guides: Arc<dyn GuideReader>) -> Self {
        Self { guides }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GuideDetailsArgs {
    /// Número de guía tal como aparece en los pedidos del cliente.
    pub numero_guia: String,
}

#[async_trait]
impl TypedTool<CustomerToolContext> for GuideDetailsTool {
    type Args = GuideDetailsArgs;

    const NAME: &'static str = "detalle_guia";
    const DESCRIPTION: &'static str = "Productos, dirección, valor y la lista de avisos enviados (con fecha) de una \
guía del cliente. Úsala cuando el cliente pregunte por avisos anteriores o necesites más detalle del que tienes.";

    async fn call(
        &self,
        context: &CustomerToolContext,
        args: GuideDetailsArgs,
    ) -> Result<serde_json::Value, ToolError> {
        let details = self
            .guides
            .details(&context.chat, args.numero_guia.trim())
            .await
            .map_err(|error| ToolError::Failed(error.to_string()))?;
        Ok(details.map(|details| guide_details_json(&details)).unwrap_or_else(guide_not_found))
    }
}
