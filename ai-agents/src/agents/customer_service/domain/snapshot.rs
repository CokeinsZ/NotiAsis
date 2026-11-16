use chrono::NaiveDateTime;

/// Foto del chat con lo que las reglas necesitan para decidir si el agente
/// responde. Todas las fechas son UTC, con el reloj de la base de datos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSnapshot {
    pub now: NaiveDateTime,
    /// Último mensaje del cliente.
    pub latest_incoming: Option<IncomingRef>,
    /// Id del último mensaje enviado por el agente (`sender = 'ai'`).
    pub last_ai_reply_id: Option<i32>,
    /// Último mensaje de un asesor desde la webapp (`sender = 'associate'`).
    pub last_associate_message_at: Option<NaiveDateTime>,
    pub last_escalation_at: Option<NaiveDateTime>,
    pub ai_replies_last_hour: i64,
    /// Configuración del agente para el business (`business_ai_agents`).
    pub agent: Option<AgentSwitch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncomingRef {
    pub id: i32,
    pub at: NaiveDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentSwitch {
    pub enabled: bool,
    /// Horas que el agente se calla después de que escribe un asesor.
    pub human_takeover_hours: i64,
}
