use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::{CustomerToolContext, guide_not_found};
use crate::agent::{ToolError, TypedTool};
use crate::agents::customer_service::domain::{GuideUpdate, GuideUpdateKind};
use crate::agents::customer_service::ports::GuideUpdateWriter;
use crate::shared::text::truncate_chars;

/// `registrar_novedad`: guarda lo que el cliente dijo de una guía (cuándo
/// la recoge, que ya la recogió o que no la quiere).
pub struct RegisterGuideUpdateTool {
    updates: Arc<dyn GuideUpdateWriter>,
}

impl RegisterGuideUpdateTool {
    pub fn new(updates: Arc<dyn GuideUpdateWriter>) -> Self {
        Self { updates }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RegisterGuideUpdateArgs {
    /// Número de guía tal como aparece en los pedidos del cliente.
    pub numero_guia: String,
    /// will_pickup: dijo cuándo va a recoger; picked_up: ya lo recogió o recibió; declined: no lo quiere.
    pub tipo: GuideUpdateKind,
    /// Fecha (YYYY-MM-DD) en que dijo que va a recoger. Obligatoria con will_pickup.
    #[serde(default)]
    pub fecha_compromiso: Option<String>,
    /// Detalle breve opcional (por ejemplo, "espera la quincena").
    #[serde(default)]
    pub nota: Option<String>,
}

#[async_trait]
impl TypedTool<CustomerToolContext> for RegisterGuideUpdateTool {
    type Args = RegisterGuideUpdateArgs;

    const NAME: &'static str = "registrar_novedad";
    const DESCRIPTION: &'static str = "Registra lo que el cliente dijo sobre una guía: cuándo va a recoger \
(will_pickup, con la fecha calculada desde hoy), que ya la recogió (picked_up) o que no la quiere (declined). No se \
lo cuentes al cliente.";

    async fn call(
        &self,
        context: &CustomerToolContext,
        args: RegisterGuideUpdateArgs,
    ) -> Result<serde_json::Value, ToolError> {
        let update = to_update(args)?;
        let recorded =
            self.updates.record(&context.chat, &update).await.map_err(|error| ToolError::Failed(error.to_string()))?;
        Ok(if recorded { json!({ "registrado": true }) } else { guide_not_found() })
    }
}

fn to_update(args: RegisterGuideUpdateArgs) -> Result<GuideUpdate, ToolError> {
    let promised_date = args
        .fecha_compromiso
        .as_deref()
        .map(str::trim)
        .filter(|date| !date.is_empty())
        .map(|date| {
            NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| {
                ToolError::InvalidArguments(format!("fecha_compromiso debe ser YYYY-MM-DD, llegó {date:?}"))
            })
        })
        .transpose()?;
    if args.tipo == GuideUpdateKind::WillPickup && promised_date.is_none() {
        return Err(ToolError::InvalidArguments("fecha_compromiso es obligatoria con will_pickup".into()));
    }
    Ok(GuideUpdate {
        guide_number: args.numero_guia.trim().to_string(),
        kind: args.tipo,
        promised_date,
        note: args.nota.map(|note| truncate_chars(note.trim(), 500)).filter(|note| !note.is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(kind: GuideUpdateKind, date: Option<&str>) -> RegisterGuideUpdateArgs {
        RegisterGuideUpdateArgs {
            numero_guia: " 240064842467 ".into(),
            tipo: kind,
            fecha_compromiso: date.map(str::to_string),
            nota: Some("  espera la quincena ".into()),
        }
    }

    #[test]
    fn will_pickup_requires_a_valid_date() {
        assert!(to_update(args(GuideUpdateKind::WillPickup, None)).is_err());
        assert!(to_update(args(GuideUpdateKind::WillPickup, Some("16/11/2026"))).is_err());

        let update = to_update(args(GuideUpdateKind::WillPickup, Some("2026-11-16"))).unwrap();
        assert_eq!(update.guide_number, "240064842467");
        assert_eq!(update.promised_date, NaiveDate::from_ymd_opt(2026, 11, 16));
        assert_eq!(update.note.as_deref(), Some("espera la quincena"));
    }

    #[test]
    fn other_kinds_do_not_need_a_date() {
        let update = to_update(args(GuideUpdateKind::PickedUp, None)).unwrap();
        assert_eq!(update.promised_date, None);
    }
}
