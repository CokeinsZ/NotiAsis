use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use validator::Validate;

use crate::chats::dtos::ChatFilters;
use crate::messages::dtos::SendMessageDto;
use crate::state::ChatState;
use crate::tools::responses::{build_validation_response, json_response};

/// Bandeja de entrada: chats de una empresa ordenados por actividad.
async fn get_chats(
    State(state): State<ChatState>,
    Query(filters): Query<ChatFilters>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = filters.validate() {
        return build_validation_response(errors);
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
    State(state): State<ChatState>,
    Path((business_id, user_phone)): Path<(i32, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
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
async fn send_chat_message(
    State(state): State<ChatState>,
    Path((business_id, user_phone)): Path<(i32, String)>,
    Json(body): Json<SendMessageDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }

    match state.message_service.send_free_message(business_id, &user_phone, body).await {
        Ok(message) => json_response(
            StatusCode::CREATED,
            serde_json::json!({
                "message": "Message sent",
                "data": message
            }),
        ),
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

pub fn chat_routes(state: ChatState) -> Router {
    Router::new()
        .route("/", get(get_chats))
        .route("/{business_id}/{user_phone}/messages", get(get_chat_messages).post(send_chat_message))
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

    use crate::chats::dtos::ChatSummary;
    use crate::chats::service::ChatServiceTrait;
    use crate::messages::dtos::{Message, MessageStatus, OutgoingMessageDto, IncomingMessageDto, SendMessageDto};
    use crate::messages::service::MessageServiceTrait;
    use crate::state::AppState;

    struct FakeChatService;

    #[async_trait::async_trait]
    impl ChatServiceTrait for FakeChatService {
        async fn get_chats(&self, business_id: i32) -> Result<Vec<ChatSummary>, String> {
            Ok(vec![ChatSummary {
                business_id,
                user_id: "573003579384".to_string(),
                user_full_name: "Stiven".to_string(),
                last_user_message: Some("Hola".to_string()),
                last_user_message_timestamp: Some(chrono::Utc::now().naive_utc()),
                last_activity: Some(chrono::Utc::now().naive_utc()),
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
            Err("Customer service window is closed (24h). Use a template message instead.".to_string())
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
    }

    fn create_routes() -> Router {
        chat_routes(ChatState {
            chat_service: Arc::new(FakeChatService),
            message_service: Arc::new(FakeMessageService),
            global_state: Arc::new(AppState { }),
        })
    }

    #[tokio::test]
    async fn get_chats_returns_inbox() {
        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/?business_id=1")
                    .method("GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body_json["chats"][0]["window_open"], true);
        assert_eq!(body_json["chats"][0]["user_full_name"], "Stiven");
    }

    #[tokio::test]
    async fn get_chats_without_business_id_returns_422_or_400() {
        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .method("GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.status() == StatusCode::BAD_REQUEST || response.status() == StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn send_message_with_closed_window_returns_422() {
        let payload = serde_json::json!({ "message": "Hola" });

        let response = create_routes()
            .oneshot(
                Request::builder()
                    .uri("/1/573003579384/messages")
                    .method("POST")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
