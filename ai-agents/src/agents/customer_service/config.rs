use std::collections::HashSet;
use std::time::Duration;

/// Parámetros del agente de atención al cliente. Lo que cambia por
/// business (activarlo, nombre, políticas, horas de toma humana) vive en
/// la tabla `business_ai_agents`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerServiceConfig {
    /// Espera tras un mensaje por si el cliente sigue escribiendo.
    pub quiet_period: Duration,
    /// Rondas máximas modelo ↔ herramientas por respuesta.
    pub max_tool_rounds: usize,
    /// Mensajes del historial que ve el agente.
    pub history_limit: i64,
    /// Guías del cliente que ve el agente.
    pub guide_limit: i64,
    /// Respuestas máximas del agente por chat en una hora (anti-bucles con
    /// respuestas automáticas de otros negocios).
    pub max_replies_per_hour: i64,
    /// Horas que el agente se calla en un chat después de escalarlo, si
    /// ningún asesor responde antes.
    pub escalation_quiet_hours: i64,
    /// Si no está vacío, el agente solo responde a estos teléfonos (pruebas).
    pub allowed_phones: HashSet<String>,
    /// Turnos que pueden estar usando el modelo al mismo tiempo.
    pub max_concurrent_turns: usize,
    /// Notas de voz que se transcriben como máximo por turno.
    pub max_voice_notes_per_turn: i64,
    /// Modo borrador: genera la respuesta y la registra en los logs, sin enviarla.
    pub dry_run: bool,
    /// Palabras frecuentes para la transcripción (además de las del cliente).
    pub base_vocabulary: Vec<String>,
}

impl Default for CustomerServiceConfig {
    fn default() -> Self {
        Self {
            quiet_period: Duration::from_secs(15),
            max_tool_rounds: 8,
            history_limit: 100,
            guide_limit: 10,
            max_replies_per_hour: 6,
            escalation_quiet_hours: 8,
            allowed_phones: HashSet::new(),
            max_concurrent_turns: 8,
            max_voice_notes_per_turn: 5,
            dry_run: false,
            base_vocabulary: [
                "Interrapidísimo",
                "Servientrega",
                "Coordinadora",
                "Envía",
                "Forza",
                "Nequi",
                "Daviplata",
                "contra entrega",
                "guía",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        }
    }
}
