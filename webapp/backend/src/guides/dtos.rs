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
    pub business_id: i32,
    pub last_notification_timestamp: Option<NaiveDateTime>,
    pub notification_count: i32,
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

    #[validate(range(min = 1, message = "El business_id debe ser positivo"))]
    pub business_id: i32,

    /// Datos del pedido (opcional). Se guardan aunque la guía ya
    /// existiera: así se completan guías registradas sin esta información.
    #[serde(default)]
    #[validate(nested)]
    pub details: Option<GuideDetailsDto>,
}

/// Datos del pedido (cliente y productos) asociados a una guía. El bot
/// los envía la primera vez que notifica un pedido de Dropi para no
/// volver a consultar el MCP en los recordatorios.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct GuideDetailsDto {
    #[validate(length(max = 32))]
    pub order_id: Option<String>,

    #[validate(length(max = 255))]
    pub customer_name: Option<String>,

    #[validate(length(max = 32))]
    pub customer_document: Option<String>,

    #[validate(length(max = 1000))]
    pub delivery_address: Option<String>,

    #[validate(length(max = 128))]
    pub city: Option<String>,

    #[validate(length(max = 128))]
    pub shipping_company: Option<String>,

    pub total: Option<f64>,

    #[validate(length(max = 8))]
    pub currency: Option<String>,

    /// Si llega vacío se conservan los productos ya guardados.
    #[serde(default)]
    #[validate(nested)]
    pub products: Vec<GuideProductDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct GuideProductDto {
    pub product_id: Option<i64>,

    #[validate(custom(function = "validate_non_blank"), length(max = 512, message = "El nombre del producto debe tener máximo 512 caracteres"))]
    pub product_name: String,

    #[serde(default = "default_quantity")]
    #[validate(range(min = 1, message = "La cantidad debe ser positiva"))]
    pub quantity: i32,
}

fn default_quantity() -> i32 {
    1
}

/// Consulta en lote de guías (el bot la usa antes de cada corrida de
/// notificación de Dropi).
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct GuideLookupDto {
    #[validate(length(min = 1, max = 500, message = "Se deben consultar entre 1 y 500 guías"))]
    pub numbers: Vec<String>,
}

/// Datos del pedido guardados en las columnas de `guides`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[derive(sqlx::FromRow)]
pub struct GuideDetails {
    pub order_id: Option<String>,
    pub customer_name: Option<String>,
    pub customer_document: Option<String>,
    pub delivery_address: Option<String>,
    pub city: Option<String>,
    pub shipping_company: Option<String>,
    pub total: Option<f64>,
    pub currency: Option<String>,
}

/// Fila de `guide_products`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(sqlx::FromRow)]
pub struct GuideProduct {
    /// Solo para agrupar los productos por guía; no se expone.
    #[serde(skip)]
    pub guide_number: String,
    pub product_id: Option<i64>,
    pub product_name: String,
    pub quantity: i32,
}

/// Guía con sus datos de pedido: con esto el bot decide el paso de
/// notificación y arma las plantillas sin consultar a Dropi.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuideWithDetails {
    #[serde(flatten)]
    pub guide: Guide,
    pub details: GuideDetails,
    pub products: Vec<GuideProduct>,
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

/// Fila de estadísticas: notificaciones de guías por día y tipo
/// (notification_count: 1=inicial, 2=recordatorio, 3=recordatorio final).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(sqlx::FromRow)]
pub struct DailyNotificationStat {
    pub day: chrono::NaiveDate,
    pub notification_count: i32,
    pub total: i64,
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
            business_id: 1,
            details: None,
        };
        assert!(valid.validate().is_ok());

        let mut invalid_phone = RegisterGuideDto {
            number: valid.number.clone(),
            user_phone: "abc".into(),
            user_name: None,
            business_id: 1,
            details: None,
        };
        assert!(invalid_phone.validate().is_err());
        invalid_phone.user_phone = "573003579384".into();
        invalid_phone.business_id = 0;
        assert!(invalid_phone.validate().is_err());
    }

    #[test]
    fn register_guide_without_details_still_deserializes() {
        // Los flujos de PDF y Google Sheet no envían `details`.
        let dto: RegisterGuideDto = serde_json::from_str(
            r#"{"number": "G1", "user_phone": "573003579384", "user_name": "Juan", "business_id": 1}"#,
        ).unwrap();
        assert!(dto.details.is_none());
    }

    #[test]
    fn register_guide_with_dropi_details() {
        // unit_price ya no se guarda: un bot viejo que lo envíe no falla.
        let dto: RegisterGuideDto = serde_json::from_str(r#"{
            "number": "240063856178",
            "user_phone": "573105409038",
            "business_id": 1,
            "details": {
                "order_id": "93632719",
                "customer_name": "Cesar Ochoa",
                "delivery_address": "Oficina Interrapidisimo",
                "city": "TARAZA",
                "total": 84900,
                "currency": "COP",
                "products": [{"product_id": 2298431, "product_name": "Collar Girasol", "unit_price": 24900}]
            }
        }"#).unwrap();
        assert!(dto.validate().is_ok());

        let details = dto.details.unwrap();
        assert_eq!(details.total, Some(84900.0));
        assert_eq!(details.products[0].quantity, 1); // valor por defecto
    }

    #[test]
    fn invalid_products_fail_validation() {
        let product = |name: &str, quantity: i32| GuideProductDto {
            product_id: None,
            product_name: name.into(),
            quantity,
        };
        let with_products = |products| RegisterGuideDto {
            number: "G1".into(),
            user_phone: "573003579384".into(),
            user_name: None,
            business_id: 1,
            details: Some(GuideDetailsDto { products, ..Default::default() }),
        };

        assert!(with_products(vec![product("Collar", 1)]).validate().is_ok());
        assert!(with_products(vec![product("   ", 1)]).validate().is_err());
        assert!(with_products(vec![product("Collar", 0)]).validate().is_err());
    }

    #[test]
    fn lookup_limits_batch_size() {
        assert!(GuideLookupDto { numbers: vec!["G1".into()] }.validate().is_ok());
        assert!(GuideLookupDto { numbers: vec![] }.validate().is_err());
        assert!(GuideLookupDto { numbers: vec!["G".into(); 501] }.validate().is_err());
    }
}
