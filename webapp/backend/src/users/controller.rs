use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};

use crate::state::UserState;
use crate::tools::responses::json_response;

async fn get_users(
    State(state): State<UserState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.user_service.get_users().await {
        Ok(users) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Users retrieved",
                "users": users
            }),
        ),
        Err(e) => json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": e }),
        ),
    }
}

async fn get_user(
    State(state): State<UserState>,
    Path(phone_number): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.user_service.get_user(&phone_number).await {
        Ok(user) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": format!("User {phone_number} retrieved"),
                "user": user
            }),
        ),
        Err(e) => {
            let status = if e == "User not found" { StatusCode::NOT_FOUND } else { StatusCode::BAD_REQUEST };
            json_response(status, serde_json::json!({ "message": e }))
        }
    }
}

pub fn user_routes(state: UserState) -> Router {
    Router::new()
        .route("/", get(get_users))
        .route("/{phone_number}", get(get_user))
        .with_state(state)
}
