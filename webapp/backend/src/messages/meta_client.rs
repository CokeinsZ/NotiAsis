use async_trait::async_trait;

pub const GRAPH_API_URL: &str = "https://graph.facebook.com/v19.0";

/// Cliente de la WhatsApp Cloud API de Meta para los envíos que hace el
/// backend (mensajes libres desde la webapp).
#[async_trait]
pub trait MetaClientTrait: Send + Sync {
    /// Envía un mensaje de texto libre. Retorna el meta_message_id
    /// asignado por Meta.
    async fn send_text_message(&self, to_number: &str, text: &str) -> Result<String, String>;
}

pub struct MetaClient {
    access_token: String,
    phone_number_id: String,
    graph_api_url: String,
    http: reqwest::Client,
}

impl MetaClient {
    pub fn new(access_token: String, phone_number_id: String) -> Self {
        Self {
            access_token,
            phone_number_id,
            graph_api_url: GRAPH_API_URL.to_string(),
            http: reqwest::Client::new(),
        }
    }
}

/// Payload del endpoint POST /{phone_number_id}/messages para texto libre.
pub fn build_text_message_payload(to_number: &str, text: &str) -> serde_json::Value {
    serde_json::json!({
        "messaging_product": "whatsapp",
        "recipient_type": "individual",
        "to": to_number.trim_start_matches('+'),
        "type": "text",
        "text": {
            "body": text
        }
    })
}

#[async_trait]
impl MetaClientTrait for MetaClient {
    async fn send_text_message(&self, to_number: &str, text: &str) -> Result<String, String> {
        let url = format!("{}/{}/messages", self.graph_api_url, self.phone_number_id);

        let response = self.http
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&build_text_message_payload(to_number, text))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Error sending message to Meta: {body}"));
        }

        let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;

        body["messages"][0]["id"]
            .as_str()
            .map(|id| id.to_string())
            .ok_or_else(|| format!("Meta response without message id: {body}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_payload_matches_meta_api_format() {
        let payload = build_text_message_payload("+573003579384", "Hola!");

        assert_eq!(payload["messaging_product"], "whatsapp");
        assert_eq!(payload["recipient_type"], "individual");
        assert_eq!(payload["to"], "573003579384"); // sin el '+'
        assert_eq!(payload["type"], "text");
        assert_eq!(payload["text"]["body"], "Hola!");
    }
}
