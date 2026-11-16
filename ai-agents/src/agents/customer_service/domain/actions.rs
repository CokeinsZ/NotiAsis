//! Lo que el agente puede registrar. Los enums se usan directo como
//! argumentos de las herramientas (de ahí `Deserialize` y `JsonSchema`):
//! agregar un valor aquí lo ofrece al modelo sin tocar nada más.

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::Deserialize;

/// Lo que dijo el cliente sobre una guía.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuideUpdateKind {
    WillPickup,
    PickedUp,
    Declined,
}

impl GuideUpdateKind {
    /// Valor en `guide_customer_updates.kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WillPickup => "will_pickup",
            Self::PickedUp => "picked_up",
            Self::Declined => "declined",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideUpdate {
    pub guide_number: String,
    pub kind: GuideUpdateKind,
    pub promised_date: Option<NaiveDate>,
    pub note: Option<String>,
}

/// Por qué el agente le pasa la conversación a un asesor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EscalationCategory {
    Cancellation,
    ShippingIssue,
    ProductIssue,
    ProductQuestion,
    OptOut,
    Complaint,
    HumanRequested,
    Other,
}

impl EscalationCategory {
    /// Valor en `ai_escalations.category`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cancellation => "cancellation",
            Self::ShippingIssue => "shipping_issue",
            Self::ProductIssue => "product_issue",
            Self::ProductQuestion => "product_question",
            Self::OptOut => "opt_out",
            Self::Complaint => "complaint",
            Self::HumanRequested => "human_requested",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Escalation {
    pub category: EscalationCategory,
    pub reason: String,
    pub guide_number: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_values_match_the_wire_names() {
        for (kind, text) in [(GuideUpdateKind::WillPickup, "will_pickup"), (GuideUpdateKind::PickedUp, "picked_up")] {
            assert_eq!(kind.as_str(), text);
            assert_eq!(serde_json::from_value::<GuideUpdateKind>(text.into()).unwrap(), kind);
        }
        let category: EscalationCategory = serde_json::from_value("human_requested".into()).unwrap();
        assert_eq!(category.as_str(), "human_requested");
    }
}
