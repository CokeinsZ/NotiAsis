use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::post,
};
use validator::Validate;

use crate::guides::dtos::{GuideFilters, RegisterGuideDto};
use crate::state::GuideState;
use crate::tools::responses::{build_validation_response, json_response};

/// El bot registra aquí cada guía recibida. `created` indica si hay que
/// notificar al usuario (false = guía duplicada, ya fue notificada).
async fn register_guide(
    State(state): State<GuideState>,
    Json(body): Json<RegisterGuideDto>,
) -> (StatusCode, Json<serde_json::Value>) {
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
    State(state): State<GuideState>,
    Query(filters): Query<GuideFilters>,
) -> (StatusCode, Json<serde_json::Value>) {
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

/// El bot marca la guía como notificada tras enviar las plantillas.
async fn mark_guide_notified(
    State(state): State<GuideState>,
    Path(number): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
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
        .route("/{number}/notified", post(mark_guide_notified))
        .with_state(state)
}
