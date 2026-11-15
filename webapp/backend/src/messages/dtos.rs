use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::tools::custom_validators::validate_non_blank;
use crate::tools::phones::PHONE_REGEX;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(sqlx::Type)]
#[sqlx(type_name = "media_type_enum", rename_all = "lowercase")]
pub enum MediaType {
    Text,
    Document,
    Audio,
    Image,
    Video,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(sqlx::Type)]
#[sqlx(type_name = "message_status_enum", rename_all = "lowercase")]
pub enum MessageStatus {
    Sent,
    Delivered,
    Read,
    /// Meta aceptó el envío pero no lo pudo entregar (el detalle está en
    /// messages_errors). Se registra con POST /messages/{id}/errors.
    Failed,
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
    /// Guía de la notificación (salientes) o de la notificación que el
    /// cliente citó al responder (entrantes).
    pub guide_number: Option<String>,
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
    #[validate(regex(path = *PHONE_REGEX, message = "El teléfono debe tener entre 8 y 15 dígitos, con '+' opcional"))]
    pub user_phone: String,

    #[validate(length(max = 255))]
    pub user_name: Option<String>,

    /// Si no viene, se deduce: el business del mensaje citado
    /// (`reply_to_meta_message_id`) o el del chat con actividad más reciente.
    pub business_id: Option<i32>,

    /// wamid del mensaje que el cliente citó al responder (`context.id`
    /// en el webhook de Meta; también llega al tocar un botón de plantilla).
    #[validate(length(min = 1, max = 255))]
    pub reply_to_meta_message_id: Option<String>,

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

    #[validate(regex(path = *PHONE_REGEX, message = "El teléfono debe tener entre 8 y 15 dígitos, con '+' opcional"))]
    pub user_phone: String,

    #[validate(length(max = 255))]
    pub user_name: Option<String>,

    #[validate(custom(function = "validate_non_blank"))]
    pub meta_message_id: String,

    pub media_type: MediaType,

    pub message: Option<String>,
    pub media_id: Option<String>,

    /// Guía que notifica la plantilla.
    #[validate(length(min = 1, max = 20))]
    pub guide_number: Option<String>,
}

/// Actualización de estado (sent/delivered/read) reportada por Meta.
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateMessageStatusDto {
    pub status: MessageStatus,
}

/// Error de un mensaje enviado, reportado por Meta en el webhook (estado
/// 'failed'). El bot lo reenvía tal cual llega.
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct RegisterMessageErrorDto {
    /// errors[0].code, ej. 131048.
    pub error_code: Option<i32>,

    /// errors[0].title, ej. "Spam Rate limit hit".
    #[validate(length(max = 255))]
    pub error_title: Option<String>,

    /// El estado completo que envió Meta (id, status, timestamp,
    /// recipient_id, errors...).
    #[validate(custom(function = "validate_json_object"))]
    pub payload: serde_json::Value,
}

/// Notificación de una guía deshecha porque su mensaje falló: la guía
/// volvió al paso (kind 'office') o nivel (kind 'tracking') anterior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[derive(sqlx::FromRow)]
pub struct UndoneNotification {
    pub guide_number: Option<String>,
    pub kind: String,
    pub notification_step: i32,
}

/// Resultado de registrar el error de un mensaje.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageFailure {
    /// Si el mensaje estaba registrado (y quedó en 'failed'). Si aún no,
    /// nace 'failed' cuando el bot lo registre.
    pub message_found: bool,
    pub undone: Vec<UndoneNotification>,
}

fn validate_json_object(value: &serde_json::Value) -> Result<(), validator::ValidationError> {
    if value.is_object() {
        return Ok(());
    }
    let mut error = validator::ValidationError::new("object");
    error.message = Some("payload debe ser el objeto del estado que envió Meta".into());
    Err(error)
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
            reply_to_meta_message_id: None,
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

    #[test]
    fn error_payload_must_be_the_status_object() {
        let dto: RegisterMessageErrorDto = serde_json::from_str(
            r#"{"error_code": 131048, "error_title": "Spam Rate limit hit",
                "payload": {"id": "wamid.1", "status": "failed", "errors": [{"code": 131048}]}}"#,
        )
        .unwrap();
        assert!(dto.validate().is_ok());

        let not_object = RegisterMessageErrorDto { payload: serde_json::json!("failed"), ..dto };
        assert!(not_object.validate().is_err());
    }
}
