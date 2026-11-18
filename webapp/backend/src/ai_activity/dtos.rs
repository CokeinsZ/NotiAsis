use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

/// Fila de `ai_escalations`: el agente le pasó el chat a un asesor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[derive(sqlx::FromRow)]
pub struct AiEscalation {
    pub id: i32,
    /// cancellation, shipping_issue, product_issue, product_question,
    /// opt_out, complaint, human_requested u other.
    pub category: String,
    /// Motivo en una frase, escrito por el agente para el asesor.
    pub reason: String,
    /// Guía relacionada (NULL si no aplica o no era del cliente).
    pub guide_number: Option<String>,
    pub created_at: NaiveDateTime,
}

/// Fila de `guide_customer_updates`: lo que el cliente le dijo al agente
/// sobre una guía.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[derive(sqlx::FromRow)]
pub struct GuideCustomerUpdate {
    pub id: i32,
    pub guide_number: String,
    /// will_pickup (con promised_date), picked_up o declined.
    pub kind: String,
    /// Día en que el cliente dijo que recoge (solo con will_pickup).
    pub promised_date: Option<NaiveDate>,
    pub note: Option<String>,
    pub created_at: NaiveDateTime,
}

/// Actividad del agente en un chat, cada lista en orden cronológico.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatAiActivity {
    pub escalations: Vec<AiEscalation>,
    pub guide_updates: Vec<GuideCustomerUpdate>,
}
