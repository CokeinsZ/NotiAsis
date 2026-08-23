use chrono::NaiveDateTime;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::tools::custom_validators::validate_non_blank;

static PHONE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\+?\d{10,15}$").unwrap()
});

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(sqlx::Type)]
#[sqlx(type_name = "media_type_enum", rename_all = "lowercase")]
pub enum MediaType {
    Text,
    Document,
    Audio,
    Image,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(sqlx::Type)]
#[sqlx(type_name = "message_status_enum", rename_all = "lowercase")]
pub enum MessageStatus {
    Sent,
    Delivered,
    Read,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[derive(sqlx::FromRow)]
pub struct Message {
    pub id: i32,
    pub meta_message_id: String,
    pub business_id: i32,
    pub user_id: String,
    pub media_id: Option<String>,
    pub media_type: MediaType,
    pub message: Option<String>,
    /// Solo aplica a mensajes salientes; los entrantes lo dejan en NULL.
    pub status: Option<MessageStatus>,
    pub from_user: bool,
    pub created_at: NaiveDateTime,
}

/// Mensaje libre que la webapp envía dentro de la ventana de 24h.
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SendMessageDto {
    #[validate(custom(function = "validate_non_blank"), length(min = 1, max = 4096, message = "El mensaje debe tener entre 1 y 4096 caracteres"))]
    pub message: String,
}

/// Mensaje entrante reportado por el bot (webhook de Meta).
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct IncomingMessageDto {
    #[validate(regex(path = *PHONE_REGEX, message = "El teléfono debe tener entre 10 y 15 dígitos, con '+' opcional"))]
    pub user_phone: String,

    #[validate(length(max = 255))]
    pub user_name: Option<String>,

    /// Si no viene, se usa el business del chat más reciente del usuario.
    pub business_id: Option<i32>,

    #[validate(custom(function = "validate_non_blank"))]
    pub meta_message_id: String,

    pub media_type: MediaType,

    pub message: Option<String>,
    pub media_id: Option<String>,

    /// Timestamp del mensaje según Meta (segundos unix).
    pub timestamp: Option<i64>,
}

/// Mensaje saliente (plantilla) reportado por el bot para que quede
/// registrado en el historial del chat.
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct OutgoingMessageDto {
    pub business_id: i32,

    #[validate(regex(path = *PHONE_REGEX, message = "El teléfono debe tener entre 10 y 15 dígitos, con '+' opcional"))]
    pub user_phone: String,

    #[validate(length(max = 255))]
    pub user_name: Option<String>,

    #[validate(custom(function = "validate_non_blank"))]
    pub meta_message_id: String,

    pub media_type: MediaType,

    pub message: Option<String>,
    pub media_id: Option<String>,
}

/// Actualización de estado (sent/delivered/read) reportada por Meta.
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateMessageStatusDto {
    pub status: MessageStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_message_rejects_blank_and_huge_texts() {
        assert!(SendMessageDto { message: "hola".into() }.validate().is_ok());
        assert!(SendMessageDto { message: "   ".into() }.validate().is_err());
        assert!(SendMessageDto { message: "x".repeat(4097) }.validate().is_err());
    }

    #[test]
    fn incoming_requires_valid_phone() {
        let mut dto = IncomingMessageDto {
            user_phone: "573003579384".into(),
            user_name: None,
            business_id: None,
            meta_message_id: "wamid.123".into(),
            media_type: MediaType::Text,
            message: Some("Hola".into()),
            media_id: None,
            timestamp: None,
        };
        assert!(dto.validate().is_ok());

        dto.user_phone = "123".into();
        assert!(dto.validate().is_err());
    }

    #[test]
    fn status_deserializes_from_meta_lowercase() {
        let dto: UpdateMessageStatusDto = serde_json::from_str(r#"{"status": "delivered"}"#).unwrap();
        assert_eq!(dto.status, MessageStatus::Delivered);
    }
}
