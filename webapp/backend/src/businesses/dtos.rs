use chrono::NaiveDateTime;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::tools::custom_validators::{validate_non_blank, validate_password};

pub static PHONE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\+?\d{10,15}$").unwrap()
});

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[derive(sqlx::Type)]
#[sqlx(type_name = "state_enum", rename_all = "PascalCase")]
pub enum EntityState {
    Active,
    Inactive,
    Blocked,
    Deleted,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[derive(sqlx::FromRow)]
pub struct Business {
    pub id: i32,
    pub name: String,
    pub state: EntityState,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Representación pública de un asociado. Nunca expone el password_hash.
#[derive(Debug, Deserialize, Serialize)]
#[derive(sqlx::FromRow)]
pub struct BusinessAssociate {
    pub id: i32,
    pub business_id: i32,
    pub phone_number: String,
    pub username: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateBusinessDto {
    #[validate(custom(function = "validate_non_blank"), length(min = 1, max = 255, message = "El nombre debe tener entre 1 y 255 caracteres"))]
    pub name: String,
}

/// Configuración del Google Sheet de usuarios a notificar de un business.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[derive(sqlx::FromRow)]
pub struct BusinessUsersSheet {
    pub id: i32,
    pub business_id: i32,
    pub document_id: String,
    pub office_id: Option<String>,
    pub delivered_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateAssociateDto {
    #[validate(regex(path = *PHONE_REGEX, message = "El teléfono debe tener entre 10 y 15 dígitos, con '+' opcional"))]
    pub phone_number: String,

    #[validate(custom(function = "validate_non_blank"), length(min = 3, max = 255, message = "El usuario debe tener entre 3 y 255 caracteres"))]
    pub username: String,

    #[validate(custom(function = "validate_password"))]
    pub password: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_associate() -> CreateAssociateDto {
        CreateAssociateDto {
            phone_number: "573003579384".to_string(),
            username: "stiven.carvajal".to_string(),
            password: "Passw0rd".to_string(),
        }
    }

    #[test]
    fn valid_associate_passes() {
        assert!(valid_associate().validate().is_ok());
    }

    #[test]
    fn invalid_phone_fails() {
        let cases = ["123", "cel:123456", "+57 300 3579384", "abc"];
        for phone in cases {
            let mut dto = valid_associate();
            dto.phone_number = phone.to_string();
            assert!(dto.validate().is_err(), "phone should fail: {phone}");
        }
    }

    #[test]
    fn phone_with_plus_is_valid() {
        let mut dto = valid_associate();
        dto.phone_number = "+573003579384".to_string();
        assert!(dto.validate().is_ok());
    }

    #[test]
    fn weak_password_fails() {
        let mut dto = valid_associate();
        dto.password = "123".to_string();
        assert!(dto.validate().is_err());
    }
}
