use async_trait::async_trait;
use serde_json::Value;

use super::{DropiError, OrderLookup, OrderStatus};
use crate::shared::http_json::{HttpError, send_json};
use crate::shared::retry::RetryPolicy;

/// Cliente de los webhooks de N8N conectados al MCP de Dropi.
pub struct N8nDropiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    retry: RetryPolicy,
}

impl N8nDropiClient {
    pub fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        retry: RetryPolicy,
    ) -> Self {
        Self { http, base_url: base_url.into(), api_key: api_key.into(), retry }
    }
}

#[async_trait]
impl OrderLookup for N8nDropiClient {
    async fn order_status(&self, business_id: i32, order_id: &str) -> Result<Option<OrderStatus>, DropiError> {
        let url = format!("{}/orden", self.base_url.trim_end_matches('/'));
        let business_id = business_id.to_string();
        let detail: Value = self
            .retry
            .run(
                || {
                    send_json(
                        self.http
                            .get(&url)
                            .header("api-key", &self.api_key)
                            .query(&[("order_id", order_id), ("business_id", business_id.as_str())]),
                    )
                },
                HttpError::is_retryable,
            )
            .await?;
        Ok(parse_order_status(&detail))
    }
}

/// Toma solo lo útil del detalle del pedido. Si el pedido no está en la
/// cuenta, N8N responde 200 con un objeto sin `status` (por ejemplo
/// `{"la orden 1784924 no está en tu cuenta": "..."}`).
fn parse_order_status(detail: &Value) -> Option<OrderStatus> {
    let text = |value: Option<&Value>| {
        value.and_then(Value::as_str).map(str::trim).filter(|text| !text.is_empty()).map(str::to_string)
    };
    Some(OrderStatus {
        status: text(detail.get("status"))?,
        tracking_code: text(detail.get("tracking_code")),
        shipping_company: text(detail.get("shipping_company")),
        city: text(detail.pointer("/customer/city")).or_else(|| text(detail.get("city"))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_status_from_the_order_detail() {
        let detail = json!({
            "order_id": "96377657", "status": "RECLAME EN OFICINA", "tracking_code": "240065804023",
            "customer": {"name": "Edinson", "city": "CARTAGENA"}, "items": [], "total": 79900
        });
        assert_eq!(
            parse_order_status(&detail),
            Some(OrderStatus {
                status: "RECLAME EN OFICINA".into(),
                tracking_code: Some("240065804023".into()),
                shipping_company: None,
                city: Some("CARTAGENA".into()),
            })
        );
    }

    #[test]
    fn order_not_in_account_is_none() {
        let detail = json!({"la orden 1784924 no está en tu cuenta": "confirma el número con el usuario"});
        assert_eq!(parse_order_status(&detail), None);
    }
}
