use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{patch, post},
};
use validator::Validate;

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

pub fn message_routes(state: MessageState) -> Router {
    Router::new()
        .route("/incoming", post(register_incoming))
        .route("/outgoing", post(register_outgoing))
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

    use crate::messages::dtos::{MediaType, Message, MessageStatus, SendMessageDto};
    use crate::messages::service::MessageServiceTrait;
    use crate::state::AppState;

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
    }

    fn create_routes() -> Router {
        message_routes(MessageState {
            message_service: Arc::new(FakeMessageService),
            global_state: Arc::new(AppState { }),
        })
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
                    .method("PATCH")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
