use async_trait::async_trait;

pub const GRAPH_API_URL: &str = "https://graph.facebook.com/v19.0";

/// Cliente de la WhatsApp Cloud API de Meta para los envíos que hace el
/// backend (mensajes libres desde la webapp) y la descarga de multimedia.
#[async_trait]
pub trait MetaClientTrait: Send + Sync {
    /// Envía un mensaje de texto libre. Retorna el meta_message_id
    /// asignado por Meta.
    async fn send_text_message(&self, to_number: &str, text: &str) -> Result<String, String>;

    /// Descarga un archivo multimedia de Meta en memoria.
    /// Retorna (content_type, bytes). Nada se escribe a disco: el archivo
    /// solo pasa por memoria volátil camino al navegador.
    async fn fetch_media(&self, media_id: &str) -> Result<(String, Vec<u8>), String>;
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

    async fn fetch_media(&self, media_id: &str) -> Result<(String, Vec<u8>), String> {
        // 1. Obtener la URL temporal de descarga
        let url = format!("{}/{}", self.graph_api_url, media_id);
        let response = self.http
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Error getting media URL from Meta: {body}"));
        }

        let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        let media_url = body["url"]
            .as_str()
            .ok_or_else(|| format!("Meta response without media url: {body}"))?
            .to_string();

        // 2. Descargar el contenido (en memoria, sin tocar disco)
        let response = self.http
            .get(&media_url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Error downloading media from Meta: {body}"));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        Ok((content_type, bytes.to_vec()))
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
