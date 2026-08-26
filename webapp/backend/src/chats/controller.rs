use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use validator::Validate;

use axum::routing::patch;

use crate::auth::middleware::{AuthenticatedClaims, authorize_business};
use crate::chats::dtos::{ChatFilters, SetImportanceDto};
use crate::messages::dtos::SendMessageDto;
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
}
