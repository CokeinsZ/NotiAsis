use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};

use crate::auth::middleware::{AuthenticatedClaims, require_bot};
use crate::state::UserState;
use crate::tools::responses::json_response;

/// Clientes de todos los business: solo el bot.
async fn get_users(
    claims: AuthenticatedClaims,
    State(state): State<UserState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
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
    claims: AuthenticatedClaims,
    State(state): State<UserState>,
    Path(phone_number): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use crate::auth::test_support::{status_of, with_auth};
    use crate::state::AppState;
    use crate::users::dtos::User;
    use crate::users::service::UserServiceTrait;

    struct FakeUserService;

    #[async_trait]
    impl UserServiceTrait for FakeUserService {
        async fn get_users(&self) -> Result<Vec<User>, String> {
            Ok(Vec::new())
        }

        async fn get_user(&self, phone_number: &str) -> Result<User, String> {
            Ok(User { phone_number: phone_number.into(), full_name: "Cliente".into() })
        }
    }

    #[tokio::test]
    async fn only_the_bot_lists_customers() {
        let routes = with_auth(user_routes(UserState {
            user_service: Arc::new(FakeUserService),
            global_state: Arc::new(AppState {}),
        }));

        assert_eq!(status_of(&routes, "GET", "/", "asociado-7", json!({})).await, StatusCode::FORBIDDEN);
        assert_eq!(status_of(&routes, "GET", "/573001112233", "asociado-7", json!({})).await, StatusCode::FORBIDDEN);
        assert_eq!(status_of(&routes, "GET", "/", "bot", json!({})).await, StatusCode::OK);
    }
}
