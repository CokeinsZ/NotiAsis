//! Herramientas del agente de atención al cliente.
//!
//! Seguridad: el chat (business y teléfono) nunca lo elige el modelo; llega
//! en [`CustomerToolContext`] desde el aviso del bot. El modelo solo aporta
//! datos como el número de guía, y cada consulta filtra por el chat.
//!
//! Agregar una herramienta: un archivo con un struct que implemente
//! `TypedTool<CustomerToolContext>` y una línea en [`toolbox`].

mod escalate_to_human;
mod guide_details;
mod live_shipping_status;
mod register_guide_update;

use std::sync::Arc;

use super::ports::{EscalationWriter, GuideReader, GuideUpdateWriter};
use crate::agent::ToolBox;
use crate::domain::ChatKey;
use crate::integrations::dropi::OrderLookup;

pub use escalate_to_human::EscalateToHumanTool;
pub use guide_details::GuideDetailsTool;
pub use live_shipping_status::LiveShippingStatusTool;
pub use register_guide_update::RegisterGuideUpdateTool;

/// Lo que las herramientas saben del turno, fijado por el sistema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerToolContext {
    pub chat: ChatKey,
}

/// Dependencias de las herramientas.
#[derive(Clone)]
pub struct ToolDeps {
    pub guides: Arc<dyn GuideReader>,
    pub guide_updates: Arc<dyn GuideUpdateWriter>,
    pub escalations: Arc<dyn EscalationWriter>,
    pub orders: Arc<dyn OrderLookup>,
}

pub fn toolbox(deps: ToolDeps) -> ToolBox<CustomerToolContext> {
    ToolBox::new()
        .with(GuideDetailsTool::new(deps.guides.clone()))
        .with(LiveShippingStatusTool::new(deps.guides, deps.orders))
        .with(RegisterGuideUpdateTool::new(deps.guide_updates))
        .with(EscalateToHumanTool::new(deps.escalations))
}

/// Mensaje para el modelo cuando la guía no es del cliente.
pub(crate) fn guide_not_found() -> serde_json::Value {
    serde_json::json!({
        "encontrada": false,
        "mensaje": "No hay una guía con ese número asociada a este cliente."
    })
}
