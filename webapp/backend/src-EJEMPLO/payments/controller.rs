use axum::{
    Router, extract::{Json, Path, Query, State}, http::StatusCode, routing::{get, post}
};
use validator::Validate;

use crate::state::{PaymentState};
use crate::payments::dtos::*;
use crate::tools::responses::{json_response, build_validation_response};

async fn create_payment(
        State(state): State<PaymentState>,
        Json(payment): Json<CreatePaymentDto>
    ) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = payment.validate() {
        return build_validation_response(errors);
    }

    match state.payment_service.create_payment(payment).await {
        Ok(message) => {
            json_response(
                StatusCode::NOT_IMPLEMENTED,
                serde_json::json!({
                    "message": message
                }),
            )
        },
        Err(e) => {
            json_response(
                StatusCode::NOT_IMPLEMENTED,
                serde_json::json!({
                    "message": e
                }),
            )
        }
    }

    
}

pub fn payment_routes(state: PaymentState) -> Router {
    Router::new()
        .route("/", post(create_payment))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    use std::sync::Arc;

    use crate::{state::AppState, payments::service::PaymentService };

    fn create_routes() -> Router {
        let app_state = AppState { };
        let payment_state = PaymentState {
            payment_service: Arc::new(PaymentService {}),
            global_state: Arc::new(app_state),
        };
        payment_routes(payment_state)
    }

    #[tokio::test]
    async fn create_payment() {
        let routes = create_routes();

        let payment = CreatePaymentDto {
            invoice_id: "123".to_string()
        };

        let response = routes
            .oneshot(
                Request::builder()
                    .uri("/")
                    .method("POST")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&payment).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        
        let status = response.status();
        let headers = response.headers().clone();

        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);

        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_text = String::from_utf8(body_bytes.to_vec()).unwrap();

        println!("\n\nTest Create Payment Response:");
        println!("Headers: {:?}", headers);
        println!("Body: {}", body_text);

    }
}