use async_trait::async_trait;

pub const GRAPH_API_URL: &str = "https://graph.facebook.com/v19.0";

/// Cliente de la WhatsApp Cloud API de Meta para los envíos que hace el
/// backend (mensajes libres desde la webapp) y la descarga de multimedia.
#[async_trait]
pub trait MetaClientTrait: Send + Sync {
    /// Envía un mensaje de texto libre. Retorna el meta_message_id
    /// asignado por Meta.
    async fn send_text_message(&self, to_number: &str, text: &str) -> Result<String, String>;

    /// Sube un archivo a Meta y retorna su media_id para usarlo en un
    /// mensaje multimedia.
    async fn upload_media(&self, bytes: Vec<u8>, filename: &str, mime: &str) -> Result<String, String>;

    /// Envía un mensaje multimedia libre referenciando un media_id ya
    /// subido. Retorna el meta_message_id asignado por Meta.
    async fn send_media_message(
        &self,
        to_number: &str,
        media_id: &str,
        kind: crate::messages::dtos::MediaType,
        caption: Option<&str>,
        filename: Option<&str>,
    ) -> Result<String, String>;

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

/// Payload para mensajes multimedia libres (imagen/video/audio/documento).
/// `kind` debe ser Image, Video, Audio o Document (nunca Text).
pub fn build_media_message_payload(
    to_number: &str,
    media_id: &str,
    kind: crate::messages::dtos::MediaType,
    caption: Option<&str>,
    filename: Option<&str>,
) -> serde_json::Value {
    use crate::messages::dtos::MediaType;
    let to = to_number.trim_start_matches('+');
    let base = serde_json::json!({
        "messaging_product": "whatsapp",
        "recipient_type": "individual",
        "to": to,
    });

    match kind {
        MediaType::Image => {
            let mut image = serde_json::json!({ "id": media_id });
            if let Some(c) = caption.filter(|c| !c.is_empty()) {
                image["caption"] = serde_json::Value::String(c.to_string());
            }
            let mut payload = base;
            payload["type"] = serde_json::Value::String("image".into());
            payload["image"] = image;
            payload
        }
        MediaType::Video => {
            let mut video = serde_json::json!({ "id": media_id });
            if let Some(c) = caption.filter(|c| !c.is_empty()) {
                video["caption"] = serde_json::Value::String(c.to_string());
            }
            let mut payload = base;
            payload["type"] = serde_json::Value::String("video".into());
            payload["video"] = video;
            payload
        }
        MediaType::Audio => {
            let mut payload = base;
            payload["type"] = serde_json::Value::String("audio".into());
            payload["audio"] = serde_json::json!({ "id": media_id });
            payload
        }
        MediaType::Document => {
            let mut document = serde_json::json!({ "id": media_id });
            if let Some(c) = caption.filter(|c| !c.is_empty()) {
                document["caption"] = serde_json::Value::String(c.to_string());
            }
            if let Some(f) = filename.filter(|f| !f.is_empty()) {
                document["filename"] = serde_json::Value::String(f.to_string());
            }
            let mut payload = base;
            payload["type"] = serde_json::Value::String("document".into());
            payload["document"] = document;
            payload
        }
        MediaType::Text => build_text_message_payload(to_number, caption.unwrap_or_default()),
    }
}

fn extract_meta_message_id(body: &serde_json::Value) -> Result<String, String> {
    body["messages"][0]["id"]
        .as_str()
        .map(|id| id.to_string())
        .ok_or_else(|| format!("Meta response without message id: {body}"))
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
        extract_meta_message_id(&body)
    }

    async fn upload_media(&self, bytes: Vec<u8>, filename: &str, mime: &str) -> Result<String, String> {
        let url = format!("{}/{}/media", self.graph_api_url, self.phone_number_id);

        let file_part = reqwest::multipart::Part::bytes(bytes)
            .file_name(filename.to_string())
            .mime_str(mime)
            .map_err(|e| e.to_string())?;

        let form = reqwest::multipart::Form::new()
            .text("messaging_product", "whatsapp")
            .part("file", file_part)
            .text("type", mime.to_string());

        let response = self.http
            .post(&url)
            .bearer_auth(&self.access_token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Error uploading media to Meta: {body}"));
        }

        let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        body["id"]
            .as_str()
            .map(|id| id.to_string())
            .ok_or_else(|| format!("Meta response without media id: {body}"))
    }

    async fn send_media_message(
        &self,
        to_number: &str,
        media_id: &str,
        kind: crate::messages::dtos::MediaType,
        caption: Option<&str>,
        filename: Option<&str>,
    ) -> Result<String, String> {
        let url = format!("{}/{}/messages", self.graph_api_url, self.phone_number_id);

        let response = self.http
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&build_media_message_payload(to_number, media_id, kind, caption, filename))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Error sending media message to Meta: {body}"));
        }

        let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        extract_meta_message_id(&body)
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
    use crate::messages::dtos::MediaType;

    #[test]
    fn text_payload_matches_meta_api_format() {
        let payload = build_text_message_payload("+573003579384", "Hola!");

        assert_eq!(payload["messaging_product"], "whatsapp");
        assert_eq!(payload["recipient_type"], "individual");
        assert_eq!(payload["to"], "573003579384"); // sin el '+'
        assert_eq!(payload["type"], "text");
        assert_eq!(payload["text"]["body"], "Hola!");
    }

    #[test]
    fn image_payload_includes_caption() {
        let payload = build_media_message_payload("573003579384", "mid.123", MediaType::Image, Some("mira"), None);
        assert_eq!(payload["type"], "image");
        assert_eq!(payload["image"]["id"], "mid.123");
        assert_eq!(payload["image"]["caption"], "mira");
    }

    #[test]
    fn video_payload_includes_caption() {
        let payload = build_media_message_payload("+573003579384", "mid.456", MediaType::Video, Some("video"), None);
        assert_eq!(payload["to"], "573003579384");
        assert_eq!(payload["type"], "video");
        assert_eq!(payload["video"]["id"], "mid.456");
        assert_eq!(payload["video"]["caption"], "video");
    }

    #[test]
    fn audio_payload_has_no_caption() {
        let payload = build_media_message_payload("573003579384", "mid.789", MediaType::Audio, Some("ignorado"), None);
        assert_eq!(payload["type"], "audio");
        assert_eq!(payload["audio"]["id"], "mid.789");
        assert!(payload.get("caption").is_none());
    }

    #[test]
    fn document_payload_includes_filename_and_caption() {
        let payload = build_media_message_payload(
            "573003579384",
            "mid.doc",
            MediaType::Document,
            Some("factura"),
            Some("factura.pdf"),
        );
        assert_eq!(payload["type"], "document");
        assert_eq!(payload["document"]["id"], "mid.doc");
        assert_eq!(payload["document"]["caption"], "factura");
        assert_eq!(payload["document"]["filename"], "factura.pdf");
    }
}
