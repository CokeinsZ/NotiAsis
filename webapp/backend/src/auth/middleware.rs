use std::sync::Arc;

use axum::{
    Json,
    body::Body,
    extract::{FromRequestParts, Request, State},
    http::{StatusCode, request::Parts},
    middleware::Next,
    response::Response,
};

use crate::auth::dtos::Claims;
use crate::auth::service::AuthServiceTrait;
use crate::tools::responses::json_response;

fn unauthorized(message: &str) -> (StatusCode, Json<serde_json::Value>) {
    json_response(
        StatusCode::UNAUTHORIZED,
        serde_json::json!({ "message": message }),
    )
}

/// Middleware que exige un JWT válido en `Authorization: Bearer <token>`
/// y deja los claims disponibles en las extensiones del request.
pub async fn require_auth(
    State(auth_service): State<Arc<dyn AuthServiceTrait>>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let header = request
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| unauthorized("Missing Authorization header"))?;

    let token = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| unauthorized("Invalid Authorization header format"))?;

    let claims = auth_service
        .validate_token(token)
        .map_err(|_| unauthorized("Invalid or expired token"))?;

    request.extensions_mut().insert(claims);
    Ok(next.run(request).await)
}

/// Extractor para los handlers: recupera los claims que dejó el middleware.
pub struct AuthenticatedClaims(pub Claims);

impl<S> FromRequestParts<S> for AuthenticatedClaims
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Claims>()
            .cloned()
            .map(AuthenticatedClaims)
            .ok_or_else(|| unauthorized("Missing authentication claims"))
    }
}

/// Verifica que los claims autoricen el acceso al business solicitado.
pub fn authorize_business(
    claims: &Claims,
    business_id: i32,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if claims.can_access_business(business_id) {
        return Ok(());
    }
    Err(json_response(
        StatusCode::FORBIDDEN,
        serde_json::json!({ "message": "You don't have access to this business" }),
    ))
}
