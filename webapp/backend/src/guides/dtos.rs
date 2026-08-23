use chrono::NaiveDateTime;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::tools::custom_validators::validate_non_blank;

static PHONE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\+?\d{10,15}$").unwrap()
});

#[derive(Debug, Clone, Deserialize, Serialize)]
#[derive(sqlx::FromRow)]
pub struct Guide {
    pub number: String,
    pub user_id: String,
    pub last_notification_timestamp: Option<NaiveDateTime>,
}

/// El bot registra aquí cada guía recibida. Si el número de guía ya
/// existía, NO se debe volver a notificar al usuario.
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct RegisterGuideDto {
    #[validate(custom(function = "validate_non_blank"), length(min = 1, max = 20, message = "El número de guía debe tener entre 1 y 20 caracteres"))]
    pub number: String,

    #[validate(regex(path = *PHONE_REGEX, message = "El teléfono debe tener entre 10 y 15 dígitos, con '+' opcional"))]
    pub user_phone: String,

    #[validate(length(max = 255))]
    pub user_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GuideRegistration {
    pub guide: Guide,
    /// true si la guía es nueva y hay que notificar; false si es duplicada.
    pub created: bool,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct GuideFilters {
    pub user_phone: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_guide_validation() {
        let valid = RegisterGuideDto {
            number: "GUIA123".into(),
            user_phone: "573003579384".into(),
            user_name: Some("Stiven".into()),
        };
        assert!(valid.validate().is_ok());

        let invalid_phone = RegisterGuideDto { number: valid.number, user_phone: "abc".into(), user_name: None };
        assert!(invalid_phone.validate().is_err());
    }
}
