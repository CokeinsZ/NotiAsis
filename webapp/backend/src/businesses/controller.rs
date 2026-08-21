use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use validator::Validate;

use crate::businesses::dtos::{CreateAssociateDto, CreateBusinessDto};
use crate::state::BusinessState;
use crate::tools::responses::{build_validation_response, json_response};

async fn create_business(
    State(state): State<BusinessState>,
    Json(body): Json<CreateBusinessDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }

    match state.business_service.create_business(body).await {
        Ok(business) => json_response(
            StatusCode::CREATED,
            serde_json::json!({
                "message": "Business created",
                "business": business
            }),
        ),
        Err(e) => json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": e }),
        ),
    }
}

async fn get_businesses(
    State(state): State<BusinessState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.business_service.get_businesses().await {
        Ok(businesses) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Businesses retrieved",
                "businesses": businesses
            }),
        ),
        Err(e) => json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": e }),
        ),
    }
}

async fn get_business(
    State(state): State<BusinessState>,
    Path(id): Path<i32>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.business_service.get_business(id).await {
        Ok(business) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": format!("Business {id} retrieved"),
                "business": business
            }),
        ),
        Err(e) => {
            let status = if e == "Business not found" { StatusCode::NOT_FOUND } else { StatusCode::BAD_REQUEST };
            json_response(status, serde_json::json!({ "message": e }))
        }
    }
}

async fn create_associate(
    State(state): State<BusinessState>,
    Path(business_id): Path<i32>,
    Json(body): Json<CreateAssociateDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }

    match state.business_service.create_associate(business_id, body).await {
        Ok(associate) => json_response(
            StatusCode::CREATED,
            serde_json::json!({
                "message": "Associate created",
                "associate": associate
            }),
        ),
        Err(e) => {
            let status = if e == "Business not found" { StatusCode::NOT_FOUND } else { StatusCode::BAD_REQUEST };
            json_response(status, serde_json::json!({ "message": e }))
        }
    }
}

async fn get_associates(
    State(state): State<BusinessState>,
    Path(business_id): Path<i32>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.business_service.get_associates(business_id).await {
        Ok(associates) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Associates retrieved",
                "associates": associates
            }),
        ),
        Err(e) => {
            let status = if e == "Business not found" { StatusCode::NOT_FOUND } else { StatusCode::BAD_REQUEST };
            json_response(status, serde_json::json!({ "message": e }))
        }
    }
}

/// Números con permiso de enviar guías. Lo consulta el bot de Python
/// al iniciar para armar su lista en memoria.
async fn get_associate_phones(
    State(state): State<BusinessState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.business_service.get_associate_phones().await {
        Ok(phones) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Associate phones retrieved",
                "phones": phones
            }),
        ),
        Err(e) => json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": e }),
        ),
    }
}

pub fn business_routes(state: BusinessState) -> Router {
    Router::new()
        .route("/", post(create_business).get(get_businesses))
        .route("/{id}", get(get_business))
        .route("/{id}/associates", post(create_associate).get(get_associates))
        .with_state(state)
}

pub fn associate_routes(state: BusinessState) -> Router {
    Router::new()
        .route("/phones", get(get_associate_phones))
        .with_state(state)
}
