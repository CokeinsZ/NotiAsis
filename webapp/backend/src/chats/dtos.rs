use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Fila de la tabla `chats`.
#[derive(Debug, Clone)]
#[derive(sqlx::FromRow)]
pub struct Chat {
    pub business_id: i32,
    pub user_id: String,
    pub last_user_message_timestamp: Option<NaiveDateTime>,
    pub last_user_message: Option<String>,
}

/// Chat enriquecido para la bandeja de entrada de la webapp.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(sqlx::FromRow)]
pub struct ChatSummary {
    pub business_id: i32,
    pub user_id: String,
    pub user_full_name: String,
    pub last_user_message: Option<String>,
    pub last_user_message_timestamp: Option<NaiveDateTime>,
    /// Última actividad del chat (mensaje de cualquiera de las dos partes).
    pub last_activity: Option<NaiveDateTime>,
    /// Si la ventana de 24h de Meta está abierta (calculado en el servicio,
    /// no viene de la base de datos).
    #[sqlx(skip)]
    pub window_open: bool,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct ChatFilters {
    #[validate(range(min = 1, message = "El business_id debe ser positivo"))]
    pub business_id: i32,
}
