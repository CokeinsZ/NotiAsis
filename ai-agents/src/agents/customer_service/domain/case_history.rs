use chrono::NaiveDateTime;

use super::actions::{Escalation, GuideUpdate};

/// Lo que el agente ya hizo en este chat: sus escalamientos a un asesor y
/// las novedades que registró. Con esto retoma la conversación después de
/// que un asesor la atendió, sin repetir ni contradecir lo que ya pasó.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseHistory {
    /// Del más antiguo al más reciente.
    pub escalations: Vec<PastEscalation>,
    /// Del más antiguo al más reciente.
    pub updates: Vec<PastGuideUpdate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PastEscalation {
    /// Fecha y hora local.
    pub at: NaiveDateTime,
    pub escalation: Escalation,
    /// Un asesor escribió en el chat después de escalarlo.
    pub answered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PastGuideUpdate {
    /// Fecha y hora local.
    pub at: NaiveDateTime,
    pub update: GuideUpdate,
}

impl CaseHistory {
    pub fn is_empty(&self) -> bool {
        self.escalations.is_empty() && self.updates.is_empty()
    }
}
