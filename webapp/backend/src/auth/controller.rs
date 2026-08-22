use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::post,
};
use validator::Validate;

use crate::auth::dtos::{ApiKeyLoginDto, LoginDto};
use crate::state::AuthState;
use crate::tools::responses::{build_validation_response, json_response};

/// Login de business associates (webapp): JWT de 15 minutos.
async fn login(
    State(state): State<AuthState>,
    Json(body): Json<LoginDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }

    match state.auth_service.login(body).await {
        Ok(token) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Login successful",
                "token": token.token,
                "expires_in": token.expires_in,
                "business_id": token.business_id,
                "phone_number": token.phone_number
            }),
        ),
        Err(e) => json_response(
            StatusCode::UNAUTHORIZED,
            serde_json::json!({ "message": e }),
        ),
    }
}

/// Login con api_key (bot): JWT de 24 horas.
async fn login_with_api_key(
    State(state): State<AuthState>,
    Json(body): Json<ApiKeyLoginDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }

    match state.auth_service.login_with_api_key(body).await {
        Ok(token) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Login successful",
                "token": token.token,
                "expires_in": token.expires_in
            }),
        ),
        Err(e) => json_response(
            StatusCode::UNAUTHORIZED,
            serde_json::json!({ "message": e }),
        ),
    }
}

pub fn auth_routes(state: AuthState) -> Router {
    Router::new()
        .route("/login", post(login))
        .route("/api-key", post(login_with_api_key))
        .with_state(state)
}
