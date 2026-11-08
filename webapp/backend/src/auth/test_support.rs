//! Utilidades para los tests de router: tokens falsos y peticiones.

use std::sync::Arc;

use axum::{Router, body::Body, http::{Request, StatusCode}, middleware};
use tower::ServiceExt;

use crate::auth::dtos::{ApiKeyLoginDto, ChangePasswordDto, Claims, LoginDto, TokenResponse};
use crate::auth::service::AuthServiceTrait;

/// Tokens: "bot" (api_key, acceso global) y "asociado-7" (asociado del business 7).
pub struct FakeAuthService;

#[async_trait::async_trait]
impl AuthServiceTrait for FakeAuthService {
    async fn login(&self, _: LoginDto) -> Result<TokenResponse, String> {
        unimplemented!()
    }

    async fn login_with_api_key(&self, _: ApiKeyLoginDto) -> Result<TokenResponse, String> {
        unimplemented!()
    }

    async fn change_password(&self, _: i32, _: &str, _: ChangePasswordDto) -> Result<(), String> {
        unimplemented!()
    }

    fn validate_token(&self, token: &str) -> Result<Claims, String> {
        let (kind, business_id, phone_number) = match token {
            "bot" => ("api_key", None, None),
            "asociado-7" => ("associate", Some(7), Some("573003579384".to_string())),
            _ => return Err("invalid token".into()),
        };
        Ok(Claims { sub: token.into(), kind: kind.into(), business_id, phone_number, iat: 0, exp: usize::MAX })
    }

    fn renew_token(&self, _: &Claims) -> Result<TokenResponse, String> {
        unimplemented!()
    }
}

/// Las rutas detrás del middleware de autenticación, como en main.rs.
pub fn with_auth(routes: Router) -> Router {
    let auth_service: Arc<dyn AuthServiceTrait> = Arc::new(FakeAuthService);
    routes.layer(middleware::from_fn_with_state(auth_service, crate::auth::middleware::require_auth))
}

pub async fn status_of(routes: &Router, method: &str, uri: &str, token: &str, body: serde_json::Value) -> StatusCode {
    routes
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .method(method)
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}
