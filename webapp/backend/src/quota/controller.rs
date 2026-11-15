use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use validator::Validate;

use crate::auth::middleware::{AuthenticatedClaims, require_bot};
use crate::quota::dtos::{ReleaseOutcome, ReserveQuotaDto};
use crate::quota::repository::QUOTA_BUSY;
use crate::state::QuotaState;
use crate::tools::responses::{build_validation_response, json_response};

/// Cupo actual de clientes nuevos en 24 h (no reserva nada).
async fn get_quota(
    claims: AuthenticatedClaims,
    State(state): State<QuotaState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
    match state.quota_service.status().await {
        Ok(status) => json_response(StatusCode::OK, serde_json::json!({ "message": "Quota retrieved", "quota": status })),
        Err(e) => json_response(StatusCode::BAD_REQUEST, serde_json::json!({ "message": e })),
    }
}

/// Antes de enviar, una corrida del bot pide cupo para los teléfonos a los
/// que va a escribir, en orden de prioridad. Responde cuáles se aprueban
/// (`approved_phones`: enviar solo a esos) y cuáles no (`denied_phones`).
/// Los aprobados que ocupan cupo quedan reservados en `reservation_id`
/// hasta liberarla con DELETE o hasta que venza.
async fn reserve_quota(
    claims: AuthenticatedClaims,
    State(state): State<QuotaState>,
    Json(body): Json<ReserveQuotaDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }

    match state.quota_service.reserve(body).await {
        Ok(decision) => {
            let message = if decision.approved { "Quota reserved" } else { "Not enough quota for every phone" };
            let mut response = serde_json::json!({ "message": message });
            if let (Some(object), Ok(serde_json::Value::Object(fields))) =
                (response.as_object_mut(), serde_json::to_value(&decision))
            {
                object.extend(fields);
            }
            json_response(StatusCode::OK, response)
        }
        // Otra reserva tuvo el cupo bloqueado más de lo normal: no aprobado,
        // se puede reintentar.
        Err(e) if e == QUOTA_BUSY => json_response(StatusCode::SERVICE_UNAVAILABLE, serde_json::json!({ "message": e })),
        Err(e) => json_response(StatusCode::BAD_REQUEST, serde_json::json!({ "message": e })),
    }
}

/// El bot libera la reserva al terminar el envío. Liberar dos veces no
/// hace nada.
async fn release_quota(
    claims: AuthenticatedClaims,
    State(state): State<QuotaState>,
    Path(reservation_id): Path<i32>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
    match state.quota_service.release(reservation_id).await {
        Ok(ReleaseOutcome::NotFound) => {
            json_response(StatusCode::NOT_FOUND, serde_json::json!({ "message": "Reservation not found" }))
        }
        Ok(outcome) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": format!("Reservation {reservation_id} released"),
                "released": outcome == ReleaseOutcome::Released
            }),
        ),
        Err(e) => json_response(StatusCode::BAD_REQUEST, serde_json::json!({ "message": e })),
    }
}

pub fn quota_routes(state: QuotaState) -> Router {
    Router::new()
        .route("/", get(get_quota))
        .route("/reservations", post(reserve_quota))
        .route("/reservations/{reservation_id}", delete(release_quota))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use serde_json::json;
    use tower::ServiceExt;

    use super::*;
    use crate::auth::test_support::{status_of, with_auth};
    use crate::quota::dtos::{QuotaDecision, QuotaKind, QuotaStatus};
    use crate::quota::service::QuotaServiceTrait;
    use crate::state::AppState;

    /// Aprueba hasta 2 teléfonos; "573000000099" simula el bloqueo ocupado.
    struct FakeQuotaService;

    #[async_trait]
    impl QuotaServiceTrait for FakeQuotaService {
        async fn status(&self) -> Result<QuotaStatus, String> {
            Ok(QuotaStatus {
                limit: 230,
                tracking_limit: 80,
                used: 200,
                reserved: 0,
                tracking_used: 10,
                tracking_reserved: 0,
                available_office: 30,
                available_tracking: 30,
            })
        }

        async fn reserve(&self, dto: ReserveQuotaDto) -> Result<QuotaDecision, String> {
            if dto.phones.iter().any(|phone| phone == "573000000099") {
                return Err(QUOTA_BUSY.to_string());
            }
            let approved_phones: Vec<String> = dto.phones.iter().take(2).cloned().collect();
            let denied_phones: Vec<String> = dto.phones.iter().skip(2).cloned().collect();
            Ok(QuotaDecision {
                approved: denied_phones.is_empty(),
                kind: dto.kind,
                approved_phones,
                denied_phones,
                new_clients: dto.phones.len() as i64,
                available: 2,
                limit: 230,
                tracking_limit: 80,
                used: 228,
                reserved: 0,
                tracking_used: 0,
                tracking_reserved: 0,
                reservation_id: Some(7),
                expires_at: None,
            })
        }

        async fn release(&self, reservation_id: i32) -> Result<ReleaseOutcome, String> {
            Ok(match reservation_id {
                7 => ReleaseOutcome::Released,
                8 => ReleaseOutcome::AlreadyReleased,
                _ => ReleaseOutcome::NotFound,
            })
        }
    }

    fn routes() -> Router {
        with_auth(quota_routes(QuotaState {
            quota_service: Arc::new(FakeQuotaService),
            global_state: Arc::new(AppState {}),
        }))
    }

    async fn reserve(phones: &[&str]) -> serde_json::Value {
        let response = routes()
            .oneshot(
                Request::builder()
                    .uri("/reservations")
                    .method("POST")
                    .header("Authorization", "Bearer bot")
                    .header("Content-Type", "application/json")
                    .body(Body::from(json!({ "phones": phones, "kind": "office", "business_id": 1 }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn approved_request_returns_its_phones_and_reservation() {
        let body = reserve(&["573001112233", "573001112244"]).await;
        assert_eq!(body["approved"], true);
        assert_eq!(body["approved_phones"], json!(["573001112233", "573001112244"]));
        assert_eq!(body["reservation_id"], 7);
        assert_eq!(body["kind"], "office");
    }

    #[tokio::test]
    async fn partial_request_says_which_phones_were_denied() {
        let body = reserve(&["573001112233", "573001112244", "573001112255"]).await;
        assert_eq!(body["approved"], false);
        assert_eq!(body["message"], "Not enough quota for every phone");
        assert_eq!(body["denied_phones"], json!(["573001112255"]));
    }

    #[tokio::test]
    async fn busy_quota_lock_is_503_so_the_bot_can_retry() {
        let body = json!({ "phones": ["573000000099"], "kind": "office" });
        assert_eq!(status_of(&routes(), "POST", "/reservations", "bot", body).await, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn invalid_requests_are_rejected() {
        let routes = routes();
        for body in [
            json!({ "phones": [], "kind": "office" }),
            json!({ "phones": ["123"], "kind": "office" }),
            json!({ "phones": ["573001112233"], "kind": "otro" }),
            json!({ "phones": ["573001112233"] }),
            json!({ "phones": ["573001112233"], "kind": "tracking", "ttl_minutes": 500 }),
        ] {
            let status = status_of(&routes, "POST", "/reservations", "bot", body.clone()).await;
            assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY, "{body}: {status}");
        }
    }

    #[tokio::test]
    async fn release_is_idempotent_and_404_for_unknown_reservations() {
        let routes = routes();
        assert_eq!(status_of(&routes, "DELETE", "/reservations/7", "bot", json!({})).await, StatusCode::OK);
        assert_eq!(status_of(&routes, "DELETE", "/reservations/8", "bot", json!({})).await, StatusCode::OK);
        assert_eq!(status_of(&routes, "DELETE", "/reservations/9", "bot", json!({})).await, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn only_the_bot_uses_the_quota() {
        let routes = routes();
        for (method, uri, body) in [
            ("GET", "/", json!({})),
            ("POST", "/reservations", json!({ "phones": ["573001112233"], "kind": QuotaKind::Office })),
            ("DELETE", "/reservations/7", json!({})),
        ] {
            assert_eq!(status_of(&routes, method, uri, "asociado-7", body).await, StatusCode::FORBIDDEN, "{method} {uri}");
        }
    }
}
