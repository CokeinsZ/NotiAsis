use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use validator::Validate;

use crate::auth::middleware::AuthenticatedClaims;
use crate::messages::dtos::{IncomingMessageDto, OutgoingMessageDto, UpdateMessageStatusDto};
use crate::state::MessageState;
use crate::tools::responses::{build_validation_response, json_response};

/// El bot de Python reporta aquí los mensajes entrantes del webhook de Meta.
async fn register_incoming(
    State(state): State<MessageState>,
    Json(body): Json<IncomingMessageDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }

    match state.message_service.register_incoming(body).await {
        Ok(message) => json_response(
            StatusCode::CREATED,
            serde_json::json!({
                "message": "Incoming message registered",
                "data": message
            }),
        ),
        Err(e) => {
            let status = if e == "Chat not found for user" { StatusCode::NOT_FOUND } else { StatusCode::BAD_REQUEST };
            json_response(status, serde_json::json!({ "message": e }))
        }
    }
}

/// El bot de Python reporta aquí los mensajes salientes (plantillas) para
/// que queden en el historial del chat.
async fn register_outgoing(
    State(state): State<MessageState>,
    Json(body): Json<OutgoingMessageDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }

    match state.message_service.register_outgoing(body).await {
        Ok(message) => json_response(
            StatusCode::CREATED,
            serde_json::json!({
                "message": "Outgoing message registered",
                "data": message
            }),
        ),
        Err(e) => json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": e }),
        ),
    }
}

/// Meta reporta los cambios de estado (sent/delivered/read) de los
/// mensajes enviados; el bot los reenvía aquí.
async fn update_status(
    State(state): State<MessageState>,
    Path(meta_message_id): Path<String>,
    Json(body): Json<UpdateMessageStatusDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }

    match state.message_service.update_status(&meta_message_id, body.status).await {
        Ok(_) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": format!("Message {meta_message_id} status updated")
            }),
        ),
        Err(e) => json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": e }),
        ),
    }
}

/// Descarga multimedia de Meta y la reenvía al navegador en memoria
/// (Content-Disposition: inline para que se visualice/reproduzca en la
/// página en vez de descargarse). Nada se guarda en el servidor.
async fn get_media(
    _claims: AuthenticatedClaims,
    State(state): State<MessageState>,
    Path(media_id): Path<String>,
) -> Response<Body> {
    match state.message_service.fetch_media(&media_id).await {
        Ok((content_type, bytes)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_DISPOSITION, "inline")
            .body(Body::from(bytes))
            .unwrap(),
        Err(e) => json_response(
            StatusCode::BAD_GATEWAY,
            serde_json::json!({ "message": e }),
        )
        .into_response(),
    }
}

pub fn message_routes(state: MessageState) -> Router {
    Router::new()
        .route("/incoming", post(register_incoming))
        .route("/outgoing", post(register_outgoing))
        .route("/media/{media_id}", get(get_media))
        .route("/{meta_message_id}/status", patch(update_status))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use std::sync::Arc;
    use tower::ServiceExt;

    use axum::middleware;

    use crate::auth::dtos::{ApiKeyLoginDto, ChangePasswordDto, Claims, LoginDto, TokenResponse};
    use crate::auth::service::AuthServiceTrait;
    use crate::messages::dtos::{MediaType, Message, MessageStatus, SendMessageDto};
    use crate::messages::service::MessageServiceTrait;
    use crate::state::AppState;

    struct FakeAuthService;

    #[async_trait::async_trait]
    impl AuthServiceTrait for FakeAuthService {
        async fn login(&self, _: LoginDto) -> Result<TokenResponse, String> {
            unimplemented!()
        }

        async fn login_with_api_key(&self, _: ApiKeyLoginDto) -> Result<TokenResponse, String> {
            unimplemented!()
        }

        async fn change_password(&self, _: i32, _: &str, _: ChangePasswordDto) -> Result<(), String> {
            unimplemented!()
        }

        fn validate_token(&self, token: &str) -> Result<Claims, String> {
            match token {
                "valido" => Ok(Claims {
                    sub: "stiven".into(),
                    kind: "associate".into(),
                    business_id: Some(1),
                    phone_number: Some("573003579384".into()),
                    iat: 0,
                    exp: usize::MAX,
                }),
                _ => Err("invalid token".into()),
            }
        }

        fn renew_token(&self, _: &Claims) -> Result<TokenResponse, String> {
            unimplemented!()
        }
    }

    struct FakeMessageService;

    #[async_trait::async_trait]
    impl MessageServiceTrait for FakeMessageService {
        async fn get_chat_messages(&self, _: i32, _: &str) -> Result<Vec<Message>, String> {
            Ok(Vec::new())
        }

        async fn send_free_message(&self, _: i32, _: &str, _: SendMessageDto) -> Result<Message, String> {
            Err("not implemented in fake".to_string())
        }

        async fn register_incoming(&self, dto: IncomingMessageDto) -> Result<Message, String> {
            Ok(Message {
                id: 1,
                meta_message_id: dto.meta_message_id,
                business_id: dto.business_id.unwrap_or(0),
                user_id: dto.user_phone,
                media_id: dto.media_id,
                media_type: dto.media_type,
                message: dto.message,
                status: None,
                from_user: true,
                created_at: chrono::Utc::now().naive_utc(),
            })
        }

        async fn register_outgoing(&self, dto: OutgoingMessageDto) -> Result<Message, String> {
            Ok(Message {
                id: 2,
                meta_message_id: dto.meta_message_id,
                business_id: dto.business_id,
                user_id: dto.user_phone,
                media_id: dto.media_id,
                media_type: dto.media_type,
                message: dto.message,
                status: Some(MessageStatus::Sent),
                from_user: false,
                created_at: chrono::Utc::now().naive_utc(),
            })
        }

        async fn update_status(&self, _: &str, _: MessageStatus) -> Result<(), String> {
            Ok(())
        }

        async fn fetch_media(&self, media_id: &str) -> Result<(String, Vec<u8>), String> {
            if media_id == "existe" {
                Ok(("application/pdf".to_string(), b"%PDF-fake".to_vec()))
            } else {
                Err("media not found".to_string())
            }
        }
    }

    fn create_routes() -> Router {
        let auth_service: Arc<dyn AuthServiceTrait> = Arc::new(FakeAuthService);
        message_routes(MessageState {
            message_service: Arc::new(FakeMessageService),
            global_state: Arc::new(AppState { }),
        })
        .layer(middleware::from_fn_with_state(
            auth_service,
            crate::auth::middleware::require_auth,
        ))
    }

    #[tokio::test]
    async fn register_incoming_message() {
        let payload = serde_json::json!({
            "user_phone": "573003579384",
            "user_name": "Stiven",
            "business_id": 1,
            "meta_message_id": "wamid.HBgMNTczMDAzNTc5Mzg0FQIAEhgUM0FEOURDMzYzRTFDMTdGNEI4REIA",
            "media_type": "text",
            "message": "Hola",
            "timestamp": 1787200338
        });

        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/incoming")
                    .header("Authorization", "Bearer valido")
                    .method("POST")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body_json["data"]["from_user"], true);
        assert_eq!(body_json["data"]["message"], "Hola");
    }

    #[tokio::test]
    async fn register_incoming_with_invalid_phone_returns_400() {
        let payload = serde_json::json!({
            "user_phone": "123",
            "meta_message_id": "wamid.123",
            "media_type": "text"
        });

        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/incoming")
                    .header("Authorization", "Bearer valido")
                    .method("POST")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn update_message_status() {
        let payload = serde_json::json!({ "status": "read" });

        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/wamid.abc/status")
                    .header("Authorization", "Bearer valido")
                    .method("PATCH")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn get_media_streams_bytes_with_content_type() {
        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/media/existe")
                    .method("GET")
                    .header("Authorization", "Bearer valido")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "application/pdf");
        assert_eq!(response.headers()["content-disposition"], "inline");

        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body_bytes.as_ref(), b"%PDF-fake");
    }

    #[tokio::test]
    async fn get_media_failure_returns_502() {
        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/media/noexiste")
                    .method("GET")
                    .header("Authorization", "Bearer valido")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn get_media_without_token_returns_401() {
        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/media/existe")
                    .method("GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
