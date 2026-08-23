use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{patch, post},
};
use validator::Validate;

use crate::auth::dtos::{ApiKeyLoginDto, ChangePasswordDto, LoginDto};
use crate::auth::middleware::AuthenticatedClaims;
use crate::state::{AssociateAdminState, AuthState};
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

/// Cambio de contraseña de un asociado. Requiere JWT del propio asociado
/// (el phone_number del token debe coincidir con el de la cuenta).
async fn change_password(
    claims: AuthenticatedClaims,
    State(state): State<AssociateAdminState>,
    Path(associate_id): Path<i32>,
    Json(body): Json<ChangePasswordDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }

    // Solo los tokens de asociado pueden cambiar contraseñas.
    let Some(requester_phone) = claims.0.phone_number.clone() else {
        return json_response(
            StatusCode::FORBIDDEN,
            serde_json::json!({ "message": "Only associates can change passwords" }),
        );
    };

    match state.auth_service.change_password(associate_id, &requester_phone, body).await {
        Ok(_) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": format!("Password of associate {associate_id} updated")
            }),
        ),
        Err(e) => {
            let status = match e.as_str() {
                "Associate not found" => StatusCode::NOT_FOUND,
                "You can only change your own password" => StatusCode::FORBIDDEN,
                _ => StatusCode::BAD_REQUEST,
            };
            json_response(status, serde_json::json!({ "message": e }))
        }
    }
}

/// Rutas administrativas de asociados (protegidas por el middleware global).
pub fn associate_admin_routes(state: AssociateAdminState) -> Router {
    Router::new()
        .route("/{id}/password", patch(change_password))
        .with_state(state)
}
