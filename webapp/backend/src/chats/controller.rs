use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::StatusCode,
    routing::get,
};
use validator::Validate;

use axum::routing::patch;

use crate::auth::middleware::{AuthenticatedClaims, authorize_business};
use crate::chats::dtos::{ChatFilters, SetImportanceDto};
use crate::messages::dtos::{MediaType, SendMessageDto};
use crate::state::ChatState;
use crate::tools::responses::{build_validation_response, json_response};

/// Bandeja de entrada: chats de una empresa ordenados por actividad.
async fn get_chats(
    claims: AuthenticatedClaims,
    State(state): State<ChatState>,
    Query(filters): Query<ChatFilters>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = filters.validate() {
        return build_validation_response(errors);
    }
    if let Err(response) = authorize_business(&claims.0, filters.business_id) {
        return response;
    }

    match state.chat_service.get_chats(filters.business_id).await {
        Ok(chats) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Chats retrieved",
                "chats": chats
            }),
        ),
        Err(e) => json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": e }),
        ),
    }
}

/// Historial de mensajes de un chat (la webapp).
async fn get_chat_messages(
    claims: AuthenticatedClaims,
    State(state): State<ChatState>,
    Path((business_id, user_phone)): Path<(i32, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = authorize_business(&claims.0, business_id) {
        return response;
    }

    match state.message_service.get_chat_messages(business_id, &user_phone).await {
        Ok(messages) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Chat messages retrieved",
                "data": messages
            }),
        ),
        Err(e) => json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": e }),
        ),
    }
}

/// Envío de mensaje libre dentro de la ventana de 24h (la webapp).
/// Cada envío exitoso renueva el token del asociado por otros 15 minutos.
async fn send_chat_message(
    claims: AuthenticatedClaims,
    State(state): State<ChatState>,
    Path((business_id, user_phone)): Path<(i32, String)>,
    Json(body): Json<SendMessageDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }
    if let Err(response) = authorize_business(&claims.0, business_id) {
        return response;
    }

    match state.message_service.send_free_message(business_id, &user_phone, body).await {
        Ok(message) => {
            // Renovar el token para que la sesión no expire mientras se conversa.
            let renewed_token = state.auth_service
                .renew_token(&claims.0)
                .map(|t| t.token)
                .ok();

            json_response(
                StatusCode::CREATED,
                serde_json::json!({
                    "message": "Message sent",
                    "data": message,
                    "renewed_token": renewed_token
                }),
            )
        }
        Err(e) => {
            let status = match e.as_str() {
                "Chat not found" => StatusCode::NOT_FOUND,
                e if e.contains("window is closed") => StatusCode::UNPROCESSABLE_ENTITY,
                _ => StatusCode::BAD_REQUEST,
            };
            json_response(status, serde_json::json!({ "message": e }))
        }
    }
}

/// Envío de mensaje multimedia libre dentro de la ventana de 24h.
/// Recibe multipart/form-data con `file`, `media_type`
/// (image|video|audio|document), `caption` opcional y `voice` opcional
/// ("true" para notas de voz grabadas: se convierten a OGG/Opus y se
/// envían con voice:true). Sube el archivo a Meta en memoria (sin tocar
/// disco salvo temporales de transcodificación) y envía el mensaje.
/// También renueva el token como el envío de texto.
async fn send_chat_media_message(
    claims: AuthenticatedClaims,
    State(state): State<ChatState>,
    Path((business_id, user_phone)): Path<(i32, String)>,
    mut multipart: Multipart,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = authorize_business(&claims.0, business_id) {
        return response;
    }

    let mut media_type_raw: Option<String> = None;
    let mut caption_raw: Option<String> = None;
    let mut voice_raw: Option<String> = None;
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;
    let mut file_mime: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "media_type" => {
                if let Ok(text) = field.text().await {
                    media_type_raw = Some(text);
                }
            }
            "caption" => {
                if let Ok(text) = field.text().await {
                    caption_raw = Some(text);
                }
            }
            "voice" => {
                if let Ok(text) = field.text().await {
                    voice_raw = Some(text);
                }
            }
            "file" => {
                file_name = field.file_name().map(|s| s.to_string());
                file_mime = field.content_type().map(|s| s.to_string());
                match field.bytes().await {
                    Ok(bytes) => file_bytes = Some(bytes.to_vec()),
                    Err(e) => {
                        return json_response(
                            StatusCode::BAD_REQUEST,
                            serde_json::json!({ "message": format!("Error reading file: {e}") }),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    let media_type: MediaType = match media_type_raw.as_deref().map(str::trim) {
        Some("image") => MediaType::Image,
        Some("video") => MediaType::Video,
        Some("audio") => MediaType::Audio,
        Some("document") => MediaType::Document,
        _ => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({ "message": "media_type debe ser image, video, audio o document" }),
            );
        }
    };

    let is_voice = matches!(voice_raw.as_deref().map(str::trim).map(str::to_lowercase).as_deref(), Some("true") | Some("1") | Some("yes"));
    if is_voice && media_type != MediaType::Audio {
        return json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": "Voice messages must be audio" }),
        );
    }

    let bytes = match file_bytes {
        Some(b) if !b.is_empty() => b,
        _ => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({ "message": "El archivo es requerido y no puede estar vacío" }),
            );
        }
    };

    // Límite duro en memoria (el mayor permitido + margen multipart).
    const HARD_LIMIT: usize = 26 * 1024 * 1024;
    if bytes.len() > HARD_LIMIT {
        return json_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            serde_json::json!({ "message": "El archivo supera los 25MB permitidos." }),
        );
    }

    let mime = file_mime.unwrap_or_else(|| "application/octet-stream".to_string());
    let filename = file_name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| default_filename(media_type, &mime));

    let caption = caption_raw.filter(|c| !c.trim().is_empty());

    match state
        .message_service
        .send_free_media_message(business_id, &user_phone, media_type, caption, filename, mime, bytes, is_voice)
        .await
    {
        Ok(message) => {
            let renewed_token = state.auth_service
                .renew_token(&claims.0)
                .map(|t| t.token)
                .ok();

            json_response(
                StatusCode::CREATED,
                serde_json::json!({
                    "message": "Media message sent",
                    "data": message,
                    "renewed_token": renewed_token
                }),
            )
        }
        Err(e) => {
            let status = match e.as_str() {
                "Chat not found" => StatusCode::NOT_FOUND,
                e if e.contains("window is closed") => StatusCode::UNPROCESSABLE_ENTITY,
                e if e.contains("ffmpeg") || e.contains("procesar el audio") || e.contains("conversión") || e.contains("Meta") => StatusCode::BAD_GATEWAY,
                e if e.contains("supera los") || e.contains("no soportado") || e.contains("vacío") || e.contains("1024") => StatusCode::BAD_REQUEST,
                _ => StatusCode::BAD_REQUEST,
            };
            json_response(status, serde_json::json!({ "message": e }))
        }
    }
}

fn default_filename(media_type: MediaType, mime: &str) -> String {
    let ext = match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "video/3gpp" => "3gp",
        "audio/mpeg" => "mp3",
        "audio/ogg" | "application/ogg" => "ogg",
        "audio/mp4" => "m4a",
        "audio/webm" | "video/webm" => "webm",
        "application/pdf" => "pdf",
        _ => match media_type {
            MediaType::Image => "jpg",
            MediaType::Video => "mp4",
            MediaType::Audio => "ogg",
            MediaType::Document => "pdf",
            MediaType::Text => "txt",
        },
    };
    match media_type {
        MediaType::Image => format!("imagen.{ext}"),
        MediaType::Video => format!("video.{ext}"),
        MediaType::Audio => format!("audio.{ext}"),
        MediaType::Document => format!("documento.{ext}"),
        MediaType::Text => format!("archivo.{ext}"),
    }
}

/// Marca o desmarca un chat como importante.
async fn set_importance(
    claims: AuthenticatedClaims,
    State(state): State<ChatState>,
    Path((business_id, user_phone)): Path<(i32, String)>,
    Json(body): Json<SetImportanceDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }
    if let Err(response) = authorize_business(&claims.0, business_id) {
        return response;
    }

    match state.chat_service.set_chat_importance(business_id, &user_phone, body.is_important).await {
        Ok(_) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": format!("Chat importance updated"),
                "is_important": body.is_important
            }),
        ),
        Err(e) => {
            let status = if e == "Chat not found" { StatusCode::NOT_FOUND } else { StatusCode::BAD_REQUEST };
            json_response(status, serde_json::json!({ "message": e }))
        }
    }
}

pub fn chat_routes(state: ChatState) -> Router {
    Router::new()
        .route("/", get(get_chats))
        .route("/{business_id}/{user_phone}/messages", get(get_chat_messages).post(send_chat_message))
        // Multipart en memoria hasta 26MB (el resto sigue con el límite
        // por defecto de 2MB de axum).
        .route(
            "/{business_id}/{user_phone}/messages/media",
            axum::routing::post(send_chat_media_message).layer(DefaultBodyLimit::disable()),
        )
        .route("/{business_id}/{user_phone}/importance", patch(set_importance))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
        middleware,
    };
    use std::sync::Arc;
    use tower::ServiceExt;

    use crate::auth::dtos::{ApiKeyLoginDto, Claims, LoginDto, TokenResponse};
    use crate::auth::service::AuthServiceTrait;
    use crate::chats::dtos::ChatSummary;
    use crate::chats::service::ChatServiceTrait;
    use crate::messages::dtos::{IncomingMessageDto, Message, MessageStatus, OutgoingMessageDto};
    use crate::messages::service::MessageServiceTrait;
    use crate::state::AppState;

    struct FakeChatService;

    #[async_trait::async_trait]
    impl ChatServiceTrait for FakeChatService {
        async fn set_chat_importance(&self, _: i32, _: &str, _: bool) -> Result<(), String> {
            Ok(())
        }

        async fn get_chats(&self, business_id: i32) -> Result<Vec<ChatSummary>, String> {
            Ok(vec![ChatSummary {
                business_id,
                user_id: "573003579384".to_string(),
                user_full_name: "Stiven".to_string(),
                last_user_message: Some("Hola".to_string()),
                last_user_message_timestamp: Some(chrono::Utc::now().naive_utc()),
                last_activity: Some(chrono::Utc::now().naive_utc()),
                is_important: false,
                last_guide_notification_at: None,
                window_open: true,
            }])
        }
    }

    struct FakeMessageService;

    #[async_trait::async_trait]
    impl MessageServiceTrait for FakeMessageService {
        async fn get_chat_messages(&self, _: i32, _: &str) -> Result<Vec<Message>, String> {
            Ok(Vec::new())
        }

        async fn send_free_message(&self, _: i32, _: &str, _: SendMessageDto) -> Result<Message, String> {
            Ok(Message {
                id: 1,
                meta_message_id: "wamid.fake".into(),
                business_id: 1,
                user_id: "573003579384".into(),
                media_id: None,
                media_type: crate::messages::dtos::MediaType::Text,
                message: Some("hola".into()),
                status: Some(MessageStatus::Sent),
                from_user: false,
                created_at: chrono::Utc::now().naive_utc(),
            })
        }

        async fn send_free_media_message(
            &self,
            business_id: i32,
            user_phone: &str,
            media_type: crate::messages::dtos::MediaType,
            caption: Option<String>,
            _filename: String,
            _mime: String,
            bytes: Vec<u8>,
            is_voice: bool,
        ) -> Result<Message, String> {
            if bytes.is_empty() {
                return Err("El archivo está vacío.".to_string());
            }
            if is_voice && media_type != crate::messages::dtos::MediaType::Audio {
                return Err("Voice messages must be audio".to_string());
            }
            Ok(Message {
                id: 2,
                meta_message_id: "wamid.fake-media".into(),
                business_id,
                user_id: user_phone.to_string(),
                media_id: Some("mid.fake".into()),
                media_type,
                message: caption,
                status: Some(MessageStatus::Sent),
                from_user: false,
                created_at: chrono::Utc::now().naive_utc(),
            })
        }

        async fn register_incoming(&self, _: IncomingMessageDto) -> Result<Message, String> {
            unimplemented!()
        }

        async fn register_outgoing(&self, _: OutgoingMessageDto) -> Result<Message, String> {
            unimplemented!()
        }

        async fn update_status(&self, _: &str, _: MessageStatus) -> Result<(), String> {
            Ok(())
        }

        async fn fetch_media(&self, _: &str) -> Result<(String, Vec<u8>), String> {
            unimplemented!()
        }
    }

    /// AuthService fake: acepta el token "valido" (business 7) y "otro" (business 9).
    struct FakeAuthService;

    #[async_trait::async_trait]
    impl AuthServiceTrait for FakeAuthService {
        async fn login(&self, _: LoginDto) -> Result<TokenResponse, String> {
            unimplemented!()
        }

        async fn login_with_api_key(&self, _: ApiKeyLoginDto) -> Result<TokenResponse, String> {
            unimplemented!()
        }

        async fn change_password(
            &self,
            _: i32,
            _: &str,
            _: crate::auth::dtos::ChangePasswordDto,
        ) -> Result<(), String> {
            unimplemented!()
        }

        fn validate_token(&self, token: &str) -> Result<Claims, String> {
            match token {
                "valido" => Ok(Claims {
                    sub: "stiven".into(),
                    kind: "associate".into(),
                    business_id: Some(7),
                    phone_number: Some("573003579384".into()),
                    iat: 0,
                    exp: usize::MAX,
                }),
                "otro" => Ok(Claims {
                    sub: "alguien".into(),
                    kind: "associate".into(),
                    business_id: Some(9),
                    phone_number: None,
                    iat: 0,
                    exp: usize::MAX,
                }),
                _ => Err("invalid token".into()),
            }
        }

        fn renew_token(&self, claims: &Claims) -> Result<TokenResponse, String> {
            Ok(TokenResponse {
                token: format!("{}-renovado", claims.sub),
                expires_in: 900,
                business_id: claims.business_id,
                phone_number: claims.phone_number.clone(),
            })
        }
    }

    fn create_routes() -> Router {
        let auth_service: Arc<dyn AuthServiceTrait> = Arc::new(FakeAuthService);
        let chat_state = ChatState {
            chat_service: Arc::new(FakeChatService),
            message_service: Arc::new(FakeMessageService),
            auth_service: auth_service.clone(),
            global_state: Arc::new(AppState { }),
        };

        chat_routes(chat_state).layer(middleware::from_fn_with_state(
            auth_service,
            crate::auth::middleware::require_auth,
        ))
    }

    fn authed_request(uri: &str, method: &str, token: Option<&str>, body: Option<String>) -> Request<Body> {
        let mut builder = Request::builder().uri(uri).method(method);
        if let Some(token) = token {
            builder = builder.header("Authorization", format!("Bearer {token}"));
        }
        builder = builder.header("Content-Type", "application/json");
        builder.body(Body::from(body.unwrap_or_default())).unwrap()
    }

    #[tokio::test]
    async fn chats_without_token_returns_401() {
        let response = create_routes()
            .oneshot(authed_request("/?business_id=7", "GET", None, None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn chats_with_invalid_token_returns_401() {
        let response = create_routes()
            .oneshot(authed_request("/?business_id=7", "GET", Some("falso"), None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn chats_with_valid_token_returns_200() {
        let response = create_routes()
            .oneshot(authed_request("/?business_id=7", "GET", Some("valido"), None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn chats_of_another_business_returns_403() {
        let response = create_routes()
            .oneshot(authed_request("/?business_id=9", "GET", Some("valido"), None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn send_message_renews_token() {
        let response = create_routes()
            .oneshot(authed_request(
                "/7/573003579384/messages",
                "POST",
                Some("valido"),
                Some(r#"{"message": "hola"}"#.to_string()),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body_json["renewed_token"], "stiven-renovado");
    }

    #[tokio::test]
    async fn send_message_to_another_business_returns_403() {
        let response = create_routes()
            .oneshot(authed_request(
                "/9/573003579384/messages",
                "POST",
                Some("valido"),
                Some(r#"{"message": "hola"}"#.to_string()),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    fn multipart_body(boundary: &str, parts: Vec<(&str, Option<(&str, &str, Vec<u8>)>, Option<String>)>) -> (String, Vec<u8>) {
        // parts: (field_name, Some((filename, mime, bytes)) | None, text_value)
        let mut body = Vec::new();
        for (name, file, text) in parts {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            if let Some((filename, mime, bytes)) = file {
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: {mime}\r\n\r\n").as_bytes(),
                );
                body.extend_from_slice(&bytes);
                body.extend_from_slice(b"\r\n");
            } else if let Some(value) = text {
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes(),
                );
            }
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        (format!("multipart/form-data; boundary={boundary}"), body)
    }

    #[tokio::test]
    async fn send_media_message_returns_201_and_renews_token() {
        let boundary = "testboundary123";
        let (content_type, body) = multipart_body(
            boundary,
            vec![
                ("media_type", None, Some("image".to_string())),
                ("caption", None, Some("mira esto".to_string())),
                ("file", Some(("foto.jpg", "image/jpeg", b"fake-image-bytes".to_vec())), None),
            ],
        );

        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/7/573003579384/messages/media")
                    .method("POST")
                    .header("Authorization", "Bearer valido")
                    .header("Content-Type", content_type)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body_json["data"]["media_type"], "image");
        assert_eq!(body_json["renewed_token"], "stiven-renovado");
    }

    #[tokio::test]
    async fn send_media_without_file_returns_400() {
        let boundary = "testboundary456";
        let (content_type, body) = multipart_body(
            boundary,
            vec![("media_type", None, Some("image".to_string()))],
        );

        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/7/573003579384/messages/media")
                    .method("POST")
                    .header("Authorization", "Bearer valido")
                    .header("Content-Type", content_type)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn send_media_with_invalid_type_returns_400() {
        let boundary = "testboundary789";
        let (content_type, body) = multipart_body(
            boundary,
            vec![
                ("media_type", None, Some("sticker".to_string())),
                ("file", Some(("foto.jpg", "image/jpeg", b"bytes".to_vec())), None),
            ],
        );

        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/7/573003579384/messages/media")
                    .method("POST")
                    .header("Authorization", "Bearer valido")
                    .header("Content-Type", content_type)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn send_media_to_another_business_returns_403() {
        let boundary = "testboundary000";
        let (content_type, body) = multipart_body(
            boundary,
            vec![
                ("media_type", None, Some("image".to_string())),
                ("file", Some(("foto.jpg", "image/jpeg", b"bytes".to_vec())), None),
            ],
        );

        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/9/573003579384/messages/media")
                    .method("POST")
                    .header("Authorization", "Bearer valido")
                    .header("Content-Type", content_type)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn default_filename_uses_extension() {
        assert_eq!(default_filename(MediaType::Image, "image/png"), "imagen.png");
        assert_eq!(default_filename(MediaType::Video, "video/mp4"), "video.mp4");
        assert_eq!(default_filename(MediaType::Audio, "audio/ogg"), "audio.ogg");
        assert_eq!(default_filename(MediaType::Document, "application/pdf"), "documento.pdf");
    }

    #[tokio::test]
    async fn send_voice_note_returns_201() {
        let boundary = "voiceboundary123";
        let (content_type, body) = multipart_body(
            boundary,
            vec![
                ("media_type", None, Some("audio".to_string())),
                ("voice", None, Some("true".to_string())),
                ("file", Some(("audio.webm", "audio/webm", b"fake-webm-bytes".to_vec())), None),
            ],
        );

        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/7/573003579384/messages/media")
                    .method("POST")
                    .header("Authorization", "Bearer valido")
                    .header("Content-Type", content_type)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body_json["data"]["media_type"], "audio");
    }

    #[tokio::test]
    async fn send_voice_flag_with_non_audio_returns_400() {
        let boundary = "voiceboundary456";
        let (content_type, body) = multipart_body(
            boundary,
            vec![
                ("media_type", None, Some("image".to_string())),
                ("voice", None, Some("true".to_string())),
                ("file", Some(("foto.jpg", "image/jpeg", b"bytes".to_vec())), None),
            ],
        );

        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/7/573003579384/messages/media")
                    .method("POST")
                    .header("Authorization", "Bearer valido")
                    .header("Content-Type", content_type)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
