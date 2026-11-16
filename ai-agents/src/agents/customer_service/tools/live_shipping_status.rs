use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::{CustomerToolContext, guide_not_found};
use crate::agent::{ToolError, TypedTool};
use crate::agents::customer_service::ports::GuideReader;
use crate::integrations::dropi::OrderLookup;

/// `estado_envio_en_vivo`: estado actual del pedido en Dropi. Primero
/// confirma que la guía es del cliente; el id del pedido sale de la base,
/// nunca del modelo.
pub struct LiveShippingStatusTool {
    guides: Arc<dyn GuideReader>,
    orders: Arc<dyn OrderLookup>,
}

impl LiveShippingStatusTool {
    pub fn new(guides: Arc<dyn GuideReader>, orders: Arc<dyn OrderLookup>) -> Self {
        Self { guides, orders }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LiveShippingStatusArgs {
    /// Número de guía tal como aparece en los pedidos del cliente.
    pub numero_guia: String,
}

#[async_trait]
impl TypedTool<CustomerToolContext> for LiveShippingStatusTool {
    type Args = LiveShippingStatusArgs;

    const NAME: &'static str = "estado_envio_en_vivo";
    const DESCRIPTION: &'static str = "Estado actual del pedido en la transportadora (consulta en vivo a Dropi). Es \
lenta y tiene límite de uso: úsala solo cuando el estado según el registro no alcance (el cliente dice que no lo \
encuentran en la oficina, que no le ha llegado, o el último aviso es de hace más de 2 días).";

    async fn call(
        &self,
        context: &CustomerToolContext,
        args: LiveShippingStatusArgs,
    ) -> Result<serde_json::Value, ToolError> {
        let guide_number = args.numero_guia.trim();
        let details = self
            .guides
            .details(&context.chat, guide_number)
            .await
            .map_err(|error| ToolError::Failed(error.to_string()))?;
        let Some(details) = details else {
            return Ok(guide_not_found());
        };
        let Some(order_id) = details.order_id else {
            return Ok(json!({ "disponible": false, "mensaje": "Esta guía no tiene un pedido de Dropi asociado." }));
        };

        let status = self
            .orders
            .order_status(context.chat.business_id, &order_id)
            .await
            .map_err(|error| ToolError::Failed(format!("no se pudo consultar Dropi: {error}")))?;
        Ok(match status {
            Some(status) => json!({
                "guia": guide_number,
                "estado_actual": status.status,
                "transportadora": status.shipping_company.or(details.summary.shipping_company),
                "ciudad": status.city.or(details.summary.city),
            }),
            None => json!({ "disponible": false, "mensaje": "Dropi no encontró el pedido de esta guía." }),
        })
    }
}
