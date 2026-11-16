use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

use super::dispatcher::TurnDispatcher;
use super::domain::IncomingMessageEvent;
use crate::domain::ChatKey;
use crate::shared::text::normalize_phone;

/// Aviso del bot: un cliente escribió y el backend ya guardó el mensaje.
#[derive(Debug, Deserialize)]
pub struct IncomingMessagePayload {
    /// `messages.id` que devolvió `POST /messages/incoming`.
    pub message_id: i32,
    pub business_id: i32,
    pub user_phone: String,
}

impl IncomingMessagePayload {
    fn into_event(self) -> Result<IncomingMessageEvent, &'static str> {
        if self.message_id <= 0 || self.business_id <= 0 {
            return Err("message_id y business_id deben ser positivos");
        }
        let phone = normalize_phone(&self.user_phone).ok_or("user_phone debe tener entre 8 y 15 dígitos")?;
        Ok(IncomingMessageEvent { message_id: self.message_id, chat: ChatKey::new(self.business_id, phone) })
    }
}

/// Responde 202 de inmediato: el turno corre en segundo plano.
async fn receive_event(
    State(dispatcher): State<Arc<dyn TurnDispatcher>>,
    Json(payload): Json<IncomingMessagePayload>,
) -> (StatusCode, Json<Value>) {
    match payload.into_event() {
        Ok(event) => {
            dispatcher.dispatch(event);
            (StatusCode::ACCEPTED, Json(json!({ "status": "accepted" })))
        }
        Err(message) => (StatusCode::BAD_REQUEST, Json(json!({ "message": message }))),
    }
}

pub fn routes(dispatcher: Arc<dyn TurnDispatcher>) -> Router {
    Router::new().route("/events", post(receive_event)).with_state(dispatcher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use std::sync::Mutex;
    use tower::ServiceExt;

    #[derive(Default)]
    struct RecordingDispatcher {
        events: Mutex<Vec<IncomingMessageEvent>>,
    }

    impl TurnDispatcher for RecordingDispatcher {
        fn dispatch(&self, event: IncomingMessageEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    async fn post_event(dispatcher: Arc<RecordingDispatcher>, body: Value) -> StatusCode {
        routes(dispatcher)
            .oneshot(
                Request::post("/events")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn accepts_and_dispatches_valid_events() {
        let dispatcher = Arc::new(RecordingDispatcher::default());
        let status = post_event(
            dispatcher.clone(),
            json!({"message_id": 10371, "business_id": 1, "user_phone": "+573223713205"}),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(
            dispatcher.events.lock().unwrap().as_slice(),
            [IncomingMessageEvent { message_id: 10371, chat: ChatKey::new(1, "573223713205") }]
        );
    }

    #[tokio::test]
    async fn rejects_invalid_events_without_dispatching() {
        let dispatcher = Arc::new(RecordingDispatcher::default());
        let status =
            post_event(dispatcher.clone(), json!({"message_id": 1, "business_id": 1, "user_phone": "123"})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let status = post_event(dispatcher.clone(), json!({"business_id": 1})).await;
        assert!(status.is_client_error());
        assert!(dispatcher.events.lock().unwrap().is_empty());
    }
}
