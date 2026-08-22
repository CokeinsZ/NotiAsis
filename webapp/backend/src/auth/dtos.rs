use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::tools::custom_validators::validate_non_blank;

/// Duración del token de un business associate (webapp).
pub const ASSOCIATE_TOKEN_MINUTES: i64 = 15;
/// Duración del token entregado con api_key (bot).
pub const API_KEY_TOKEN_HOURS: i64 = 24;

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct LoginDto {
    #[validate(custom(function = "validate_non_blank"))]
    pub username: String,

    #[validate(custom(function = "validate_non_blank"))]
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct ApiKeyLoginDto {
    #[validate(custom(function = "validate_non_blank"))]
    pub api_key: String,
}

/// Claims del JWT. `kind` distingue el origen del token:
/// "associate" (usuario de la webapp) o "api_key" (bot).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    /// username del asociado, o "api_key:{id}" para tokens del bot.
    pub sub: String,
    pub kind: String,
    /// Business del asociado (None en tokens de api_key, que son globales).
    pub business_id: Option<i32>,
    /// Teléfono del asociado (None en tokens de api_key).
    pub phone_number: Option<String>,
    pub iat: usize,
    pub exp: usize,
}

impl Claims {
    pub fn is_associate(&self) -> bool {
        self.kind == "associate"
    }

    /// Autorización de acceso a los recursos de un business:
    /// - tokens de api_key (bot): acceso global.
    /// - tokens de associate: solo su propio business.
    pub fn can_access_business(&self, business_id: i32) -> bool {
        !self.is_associate() || self.business_id == Some(business_id)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub token: String,
    /// Segundos hasta el vencimiento.
    pub expires_in: i64,
    pub business_id: Option<i32>,
    pub phone_number: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn associate_claims(business_id: i32) -> Claims {
        Claims {
            sub: "user".into(),
            kind: "associate".into(),
            business_id: Some(business_id),
            phone_number: Some("573003579384".into()),
            iat: 0,
            exp: 0,
        }
    }

    fn api_key_claims() -> Claims {
        Claims {
            sub: "api_key:1".into(),
            kind: "api_key".into(),
            business_id: None,
            phone_number: None,
            iat: 0,
            exp: 0,
        }
    }

    #[test]
    fn associate_only_accesses_own_business() {
        assert!(associate_claims(1).can_access_business(1));
        assert!(!associate_claims(1).can_access_business(2));
    }

    #[test]
    fn api_key_accesses_any_business() {
        assert!(api_key_claims().can_access_business(1));
        assert!(api_key_claims().can_access_business(99));
    }
}
