use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::CustomerToolContext;
use crate::agent::{ToolError, TypedTool};
use crate::agents::customer_service::domain::{Escalation, EscalationCategory};
use crate::agents::customer_service::ports::EscalationWriter;
use crate::shared::text::truncate_chars;

/// `escalar_a_humano`: deja el caso a un asesor y marca el chat como
/// importante en la webapp. El agente se calla en ese chat hasta que
/// responda un asesor (regla `open_escalation`).
pub struct EscalateToHumanTool {
    escalations: Arc<dyn EscalationWriter>,
}

impl EscalateToHumanTool {
    pub fn new(escalations: Arc<dyn EscalationWriter>) -> Self {
        Self { escalations }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EscalateToHumanArgs {
    /// cancellation: quiere cancelar o no recogerá a tiempo; shipping_issue: dirección, ciudad u oficina;
    /// product_issue: producto dañado, distinto o no pedido, cobro diferente o una reacción al tomarlo;
    /// product_question: un dato del producto que no está en su ficha;
    /// opt_out: no quiere más mensajes; complaint: molesto o grosero; human_requested: pidió una persona; other.
    pub categoria: EscalationCategory,
    /// Motivo en una frase, para el asesor.
    pub motivo: String,
    /// Guía relacionada, si aplica.
    #[serde(default)]
    pub numero_guia: Option<String>,
}

#[async_trait]
impl TypedTool<CustomerToolContext> for EscalateToHumanTool {
    type Args = EscalateToHumanArgs;

    const NAME: &'static str = "escalar_a_humano";
    const DESCRIPTION: &'static str = "Pasa la conversación a un asesor humano. Úsala cuando no puedas resolver \
algo con los datos que tienes o cuando las instrucciones lo indiquen. Después dile al cliente que un asesor revisa su \
caso y le escribe por este chat.";

    async fn call(
        &self,
        context: &CustomerToolContext,
        args: EscalateToHumanArgs,
    ) -> Result<serde_json::Value, ToolError> {
        let reason = truncate_chars(args.motivo.trim(), 500);
        if reason.is_empty() {
            return Err(ToolError::InvalidArguments("motivo no puede estar vacío".into()));
        }
        let escalation = Escalation {
            category: args.categoria,
            reason,
            guide_number: args.numero_guia.map(|number| number.trim().to_string()).filter(|number| !number.is_empty()),
        };
        self.escalations
            .escalate(&context.chat, &escalation)
            .await
            .map_err(|error| ToolError::Failed(error.to_string()))?;
        tracing::info!(chat = %context.chat, category = escalation.category.as_str(), "chat escalated to a human");
        Ok(json!({ "escalado": true }))
    }
}
