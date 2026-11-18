use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};

use crate::auth::middleware::{AuthenticatedClaims, authorize_business};
use crate::state::AiActivityState;
use crate::tools::responses::json_response;

/// Escalaciones y novedades de guías que el agente de IA registró en un
/// chat (la webapp las intercala con los mensajes del historial).
async fn get_chat_ai_activity(
    claims: AuthenticatedClaims,
    State(state): State<AiActivityState>,
    Path((business_id, user_phone)): Path<(i32, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = authorize_business(&claims.0, business_id) {
        return response;
    }

    match state.ai_activity_service.get_chat_activity(business_id, &user_phone).await {
        Ok(activity) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Chat AI activity retrieved",
                "data": activity
            }),
        ),
        Err(e) => json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": e }),
        ),
    }
}

/// Se monta bajo `/chats`, junto a las rutas del módulo de chats.
pub fn ai_activity_routes(state: AiActivityState) -> Router {
    Router::new()
        .route("/{business_id}/{user_phone}/ai-activity", get(get_chat_ai_activity))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
        middleware,
    };
    use std::sync::Arc;
    use tower::ServiceExt;

    use crate::ai_activity::dtos::{AiEscalation, ChatAiActivity};
    use crate::ai_activity::service::AiActivityServiceTrait;
    use crate::auth::dtos::{ApiKeyLoginDto, Claims, LoginDto, TokenResponse};
    use crate::auth::service::AuthServiceTrait;
    use crate::state::AppState;

    struct FakeAiActivityService;

    #[async_trait::async_trait]
    impl AiActivityServiceTrait for FakeAiActivityService {
        async fn get_chat_activity(&self, _: i32, _: &str) -> Result<ChatAiActivity, String> {
            Ok(ChatAiActivity {
                escalations: vec![AiEscalation {
                    id: 1,
                    category: "complaint".into(),
                    reason: "Está molesto por la demora".into(),
                    guide_number: None,
                    created_at: chrono::NaiveDate::from_ymd_opt(2026, 11, 16)
                        .unwrap()
                        .and_hms_opt(14, 30, 0)
                        .unwrap(),
                }],
                guide_updates: Vec::new(),
            })
        }
    }

    /// Acepta el token "valido" (asociado del business 7).
    struct FakeAuthService;

    #[async_trait::async_trait]
    impl AuthServiceTrait for FakeAuthService {
        async fn login(&self, _: LoginDto) -> Result<TokenResponse, String> {
            unimplemented!()
        }

        async fn login_with_api_key(&self, _: ApiKeyLoginDto) -> Result<TokenResponse, String> {
            unimplemented!()
        }

        async fn change_password(&self, _: i32, _: &str, _: crate::auth::dtos::ChangePasswordDto) -> Result<(), String> {
            unimplemented!()
        }

        fn validate_token(&self, token: &str) -> Result<Claims, String> {
            match token {
                "valido" => Ok(Claims {
                    sub: "asesor".into(),
                    kind: "associate".into(),
                    business_id: Some(7),
                    phone_number: None,
                    iat: 0,
                    exp: usize::MAX,
                }),
                _ => Err("invalid token".into()),
            }
        }

        fn renew_token(&self, _: &Claims) -> Result<TokenResponse, String> {
            unimplemented!()
        }
    }

    fn create_routes() -> Router {
        let auth_service: Arc<dyn AuthServiceTrait> = Arc::new(FakeAuthService);
        let state = AiActivityState {
            ai_activity_service: Arc::new(FakeAiActivityService),
            global_state: Arc::new(AppState {}),
        };

        ai_activity_routes(state)
            .layer(middleware::from_fn_with_state(auth_service, crate::auth::middleware::require_auth))
    }

    fn get(uri: &str, token: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder().uri(uri).method("GET");
        if let Some(token) = token {
            builder = builder.header("Authorization", format!("Bearer {token}"));
        }
        builder.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn requires_a_token() {
        let response = create_routes().oneshot(get("/7/573003579384/ai-activity", None)).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn another_business_is_forbidden() {
        let response = create_routes().oneshot(get("/9/573003579384/ai-activity", Some("valido"))).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn returns_escalations_and_guide_updates() {
        let response = create_routes().oneshot(get("/7/573003579384/ai-activity", Some("valido"))).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["data"]["escalations"][0]["category"], "complaint");
        assert_eq!(json["data"]["escalations"][0]["created_at"], "2026-11-16T14:30:00");
        assert_eq!(json["data"]["guide_updates"], serde_json::json!([]));
    }
}
