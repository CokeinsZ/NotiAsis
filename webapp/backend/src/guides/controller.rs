use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use validator::Validate;

use crate::auth::middleware::{AuthenticatedClaims, require_bot};
use crate::guides::dtos::{GuideFilters, GuideLookupDto, RegisterGuideDto};
use crate::state::GuideState;
use crate::tools::responses::{build_validation_response, json_response};

/// El bot registra aquí cada guía recibida. `created` indica si hay que
/// notificar al usuario (false = guía duplicada, ya fue notificada).
/// Si trae `details` (datos del pedido), se guardan aunque la guía ya
/// existiera.
async fn register_guide(
    claims: AuthenticatedClaims,
    State(state): State<GuideState>,
    Json(body): Json<RegisterGuideDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }

    match state.guide_service.register_guide(body).await {
        Ok(registration) => {
            let status = if registration.created { StatusCode::CREATED } else { StatusCode::OK };
            json_response(
                status,
                serde_json::json!({
                    "message": if registration.created { "Guide registered" } else { "Guide already registered" },
                    "guide": registration.guide,
                    "created": registration.created
                }),
            )
        }
        Err(e) => json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": e }),
        ),
    }
}

async fn get_guides(
    claims: AuthenticatedClaims,
    State(state): State<GuideState>,
    Query(filters): Query<GuideFilters>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
    if let Err(errors) = filters.validate() {
        return build_validation_response(errors);
    }

    match state.guide_service.get_guides(filters.user_phone).await {
        Ok(guides) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Guides retrieved",
                "guides": guides
            }),
        ),
        Err(e) => json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": e }),
        ),
    }
}

/// Consulta una guía puntual por su número.
async fn get_guide(
    claims: AuthenticatedClaims,
    State(state): State<GuideState>,
    Path(number): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
    match state.guide_service.get_guide(&number).await {
        Ok(guide) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Guide retrieved",
                "guide": guide
            }),
        ),
        Err(e) => {
            let status = if e == "Guide not found" { StatusCode::NOT_FOUND } else { StatusCode::BAD_REQUEST };
            json_response(status, serde_json::json!({ "message": e }))
        }
    }
}

/// El bot consulta en lote qué guías ya existen, cuántas veces se
/// notificaron y los datos de pedido guardados (cliente y productos).
/// Solo para el token del bot: expone datos personales de los clientes.
async fn lookup_guides(
    claims: AuthenticatedClaims,
    State(state): State<GuideState>,
    Json(body): Json<GuideLookupDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }

    match state.guide_service.lookup_guides(body.numbers).await {
        Ok(guides) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Guides retrieved",
                "guides": guides
            }),
        ),
        Err(e) => json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": e }),
        ),
    }
}

/// El bot marca la guía como notificada tras enviar las plantillas.
async fn mark_guide_notified(
    claims: AuthenticatedClaims,
    State(state): State<GuideState>,
    Path(number): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
    match state.guide_service.mark_notified(&number).await {
        Ok(_) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": format!("Guide {number} marked as notified")
            }),
        ),
        Err(e) => {
            let status = if e == "Guide not found" { StatusCode::NOT_FOUND } else { StatusCode::BAD_REQUEST };
            json_response(status, serde_json::json!({ "message": e }))
        }
    }
}

pub fn guide_routes(state: GuideState) -> Router {
    Router::new()
        .route("/", post(register_guide).get(get_guides))
        .route("/lookup", post(lookup_guides))
        .route("/{number}", get(get_guide))
        .route("/{number}/notified", post(mark_guide_notified))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use crate::auth::test_support::{status_of, with_auth};
    use crate::guides::dtos::{DailyNotificationStat, Guide, GuideRegistration, GuideWithDetails};
    use crate::guides::service::GuideServiceTrait;
    use crate::state::AppState;

    /// Los handlers deben responder 403 antes de llegar al servicio.
    struct UnreachableGuideService;

    #[async_trait]
    impl GuideServiceTrait for UnreachableGuideService {
        async fn register_guide(&self, _: RegisterGuideDto) -> Result<GuideRegistration, String> {
            unreachable!()
        }

        async fn get_guides(&self, _: Option<String>) -> Result<Vec<Guide>, String> {
            unreachable!()
        }

        async fn lookup_guides(&self, _: Vec<String>) -> Result<Vec<GuideWithDetails>, String> {
            unreachable!()
        }

        async fn get_guide(&self, _: &str) -> Result<Guide, String> {
            unreachable!()
        }

        async fn mark_notified(&self, _: &str) -> Result<(), String> {
            unreachable!()
        }

        async fn get_notification_stats(
            &self,
            _: i32,
            _: Option<i32>,
            _: Option<chrono::NaiveDate>,
            _: Option<chrono::NaiveDate>,
        ) -> Result<Vec<DailyNotificationStat>, String> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn associates_cannot_read_or_change_guides() {
        let routes = with_auth(guide_routes(GuideState {
            guide_service: Arc::new(UnreachableGuideService),
            global_state: Arc::new(AppState {}),
        }));
        let guide = json!({ "number": "G1", "user_phone": "573001112233", "user_name": "X", "business_id": 9 });

        for (method, uri, body) in [
            ("POST", "/", guide),
            ("GET", "/", json!({})),
            ("GET", "/G1", json!({})),
            ("POST", "/G1/notified", json!({})),
            ("POST", "/lookup", json!({ "numbers": ["G1"] })),
        ] {
            assert_eq!(status_of(&routes, method, uri, "asociado-7", body).await, StatusCode::FORBIDDEN, "{method} {uri}");
        }
    }
}
