//! Estado de pedidos de Dropi a través de los webhooks de N8N que ya usa
//! el bot (`GET {DROPI_N8N_URL}/orden?order_id=&business_id=`).

mod client;

use async_trait::async_trait;

use crate::shared::http_json::HttpError;

pub use client::N8nDropiClient;

#[derive(Debug, thiserror::Error)]
pub enum DropiError {
    #[error("Dropi HTTP error: {0}")]
    Http(#[from] HttpError),
    #[error("Dropi client configuration error: {0}")]
    Configuration(String),
}

/// Estado actual de un pedido en Dropi.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct OrderStatus {
    pub status: String,
    pub tracking_code: Option<String>,
    pub shipping_company: Option<String>,
    pub city: Option<String>,
}

/// Consulta el estado de un pedido. `Ok(None)` si Dropi no lo encuentra
/// en la cuenta del business.
#[async_trait]
pub trait OrderLookup: Send + Sync {
    async fn order_status(&self, business_id: i32, order_id: &str) -> Result<Option<OrderStatus>, DropiError>;
}
