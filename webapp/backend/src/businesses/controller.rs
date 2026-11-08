use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use validator::Validate;

use crate::auth::middleware::{AuthenticatedClaims, authorize_business, require_bot};
use crate::businesses::dtos::{CreateAssociateDto, CreateBusinessDto};
use crate::businesses::repository::{PHONE_TAKEN, USERNAME_TAKEN};
use crate::state::BusinessState;
use crate::tools::responses::{build_validation_response, json_response};

async fn create_business(
    claims: AuthenticatedClaims,
    State(state): State<BusinessState>,
    Json(body): Json<CreateBusinessDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
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
    claims: AuthenticatedClaims,
    State(state): State<BusinessState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
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
    claims: AuthenticatedClaims,
    State(state): State<BusinessState>,
    Path(id): Path<i32>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = authorize_business(&claims.0, id) {
        return response;
    }
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
    claims: AuthenticatedClaims,
    State(state): State<BusinessState>,
    Path(business_id): Path<i32>,
    Json(body): Json<CreateAssociateDto>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
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
            let status = match e.as_str() {
                "Business not found" => StatusCode::NOT_FOUND,
                PHONE_TAKEN | USERNAME_TAKEN => StatusCode::CONFLICT,
                _ => StatusCode::BAD_REQUEST,
            };
            json_response(status, serde_json::json!({ "message": e }))
        }
    }
}

async fn get_associates(
    claims: AuthenticatedClaims,
    State(state): State<BusinessState>,
    Path(business_id): Path<i32>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
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

/// Todos los asociados de empresas activas (con su business_id).
/// Lo consulta el bot de Python al iniciar para armar su lista en memoria.
async fn get_all_associates(
    claims: AuthenticatedClaims,
    State(state): State<BusinessState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
    match state.business_service.get_all_associates().await {
        Ok(associates) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Associates retrieved",
                "associates": associates
            }),
        ),
        Err(e) => json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": e }),
        ),
    }
}

/// Estadísticas de notificaciones por día y tipo para el dashboard.
/// Solo accesible con un JWT autorizado para ese business.
async fn get_notification_stats(
    claims: AuthenticatedClaims,
    State(state): State<BusinessState>,
    Path(business_id): Path<i32>,
    Query(filters): Query<NotificationStatsFiltersQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = authorize_business(&claims.0, business_id) {
        return response;
    }

    match state.guide_service.get_notification_stats(business_id, filters.days, filters.start_date, filters.end_date).await {
        Ok(stats) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Notification stats retrieved",
                "stats": stats
            }),
        ),
        Err(e) => json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({ "message": e }),
        ),
    }
}

/// Query params del endpoint de estadísticas (el business_id va en el path).
#[derive(Debug, serde::Deserialize)]
pub struct NotificationStatsFiltersQuery {
    pub days: Option<i32>,
    pub start_date: Option<chrono::NaiveDate>,
    pub end_date: Option<chrono::NaiveDate>,
}

/// Configuración del Google Sheet de usuarios a notificar de un business.
/// La consulta el bot para el endpoint de notificación masiva.
async fn get_business_sheet(
    claims: AuthenticatedClaims,
    State(state): State<BusinessState>,
    Path(business_id): Path<i32>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
    match state.business_service.get_business_sheet(business_id).await {
        Ok(sheet) => json_response(
            StatusCode::OK,
            serde_json::json!({
                "message": "Sheet config retrieved",
                "sheet": sheet
            }),
        ),
        Err(e) => {
            let status = if e == "Sheet config not found for business" { StatusCode::NOT_FOUND } else { StatusCode::BAD_REQUEST };
            json_response(status, serde_json::json!({ "message": e }))
        }
    }
}

/// Números con permiso de enviar guías. Lo consulta el bot de Python
/// al iniciar para armar su lista en memoria.
async fn get_associate_phones(
    claims: AuthenticatedClaims,
    State(state): State<BusinessState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(response) = require_bot(&claims.0) {
        return response;
    }
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
        .route("/{id}/sheet", get(get_business_sheet))
        .route("/{id}/stats/notifications", get(get_notification_stats))
        .route("/{id}/associates", post(create_associate).get(get_associates))
        .with_state(state)
}

pub fn associate_routes(state: BusinessState) -> Router {
    Router::new()
        .route("/", get(get_all_associates))
        .route("/phones", get(get_associate_phones))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::http::StatusCode;
    use serde_json::json;

    use super::*;
    use crate::auth::test_support::{status_of, with_auth};
    use crate::businesses::dtos::{
        Business, BusinessAssociate, BusinessUsersSheet, BusinessWithCountry, Country, EntityState,
    };
    use crate::businesses::service::BusinessServiceTrait;
    use crate::guides::dtos::{DailyNotificationStat, Guide, GuideRegistration, GuideWithDetails, RegisterGuideDto};
    use crate::guides::service::GuideServiceTrait;
    use crate::state::AppState;

    const TAKEN_PHONE: &str = "573000000000";

    fn business(id: i32) -> Business {
        let now = chrono::Utc::now().naive_utc();
        Business { id, name: format!("B{id}"), state: EntityState::Active, country_id: 1, created_at: now, updated_at: now }
    }

    struct FakeBusinessService;

    #[async_trait]
    impl BusinessServiceTrait for FakeBusinessService {
        async fn create_business(&self, _: CreateBusinessDto) -> Result<Business, String> {
            Ok(business(1))
        }

        async fn get_businesses(&self) -> Result<Vec<Business>, String> {
            Ok(Vec::new())
        }

        async fn get_business(&self, id: i32) -> Result<BusinessWithCountry, String> {
            let country = Country { id: 1, code: "CO".into(), name: "Colombia".into(), phone_prefix: "57".into(), phone_length: 10 };
            Ok(BusinessWithCountry { business: business(id), country })
        }

        async fn get_business_sheet(&self, _: i32) -> Result<BusinessUsersSheet, String> {
            Err("Sheet config not found for business".into())
        }

        async fn create_associate(&self, business_id: i32, dto: CreateAssociateDto) -> Result<BusinessAssociate, String> {
            if dto.phone_number == TAKEN_PHONE {
                return Err(PHONE_TAKEN.into());
            }
            Ok(BusinessAssociate { id: 1, business_id, phone_number: dto.phone_number, username: dto.username })
        }

        async fn get_associates(&self, _: i32) -> Result<Vec<BusinessAssociate>, String> {
            Ok(Vec::new())
        }

        async fn get_all_associates(&self) -> Result<Vec<BusinessAssociate>, String> {
            Ok(Vec::new())
        }

        async fn get_associate_phones(&self) -> Result<Vec<String>, String> {
            Ok(Vec::new())
        }
    }

    struct FakeGuideService;

    #[async_trait]
    impl GuideServiceTrait for FakeGuideService {
        async fn register_guide(&self, _: RegisterGuideDto) -> Result<GuideRegistration, String> {
            unimplemented!()
        }

        async fn get_guides(&self, _: Option<String>) -> Result<Vec<Guide>, String> {
            unimplemented!()
        }

        async fn lookup_guides(&self, _: Vec<String>) -> Result<Vec<GuideWithDetails>, String> {
            unimplemented!()
        }

        async fn get_guide(&self, _: &str) -> Result<Guide, String> {
            unimplemented!()
        }

        async fn mark_notified(&self, _: &str) -> Result<(), String> {
            unimplemented!()
        }

        async fn get_notification_stats(
            &self,
            _: i32,
            _: Option<i32>,
            _: Option<chrono::NaiveDate>,
            _: Option<chrono::NaiveDate>,
        ) -> Result<Vec<DailyNotificationStat>, String> {
            Ok(Vec::new())
        }
    }

    fn routes() -> Router {
        let state = BusinessState {
            business_service: Arc::new(FakeBusinessService),
            guide_service: Arc::new(FakeGuideService),
            global_state: Arc::new(AppState {}),
        };
        with_auth(
            Router::new()
                .nest("/businesses", business_routes(state.clone()))
                .nest("/associates", associate_routes(state)),
        )
    }

    fn new_associate(phone: &str) -> serde_json::Value {
        json!({ "phone_number": phone, "username": "nuevo.asociado", "password": "Passw0rd" })
    }

    #[tokio::test]
    async fn associate_cannot_use_admin_routes_or_other_businesses() {
        let routes = routes();
        let forbidden = [
            ("POST", "/businesses", json!({ "name": "Otra", "country_id": 1 })),
            ("GET", "/businesses", json!({})),
            ("GET", "/businesses/9", json!({})),
            ("GET", "/businesses/9/stats/notifications", json!({})),
            ("GET", "/businesses/7/sheet", json!({})),
            // Ni siquiera en su propio business: crear cuentas es del administrador.
            ("POST", "/businesses/7/associates", new_associate("3001112233")),
            ("POST", "/businesses/9/associates", new_associate("3001112233")),
            ("GET", "/businesses/7/associates", json!({})),
            ("GET", "/associates", json!({})),
            ("GET", "/associates/phones", json!({})),
        ];
        for (method, uri, body) in forbidden {
            assert_eq!(status_of(&routes, method, uri, "asociado-7", body).await, StatusCode::FORBIDDEN, "{method} {uri}");
        }
    }

    #[tokio::test]
    async fn associate_reads_own_business() {
        let routes = routes();

        assert_eq!(status_of(&routes, "GET", "/businesses/7", "asociado-7", json!({})).await, StatusCode::OK);
        assert_eq!(
            status_of(&routes, "GET", "/businesses/7/stats/notifications", "asociado-7", json!({})).await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn bot_creates_associates_and_duplicate_phone_is_a_conflict() {
        let routes = routes();

        assert_eq!(status_of(&routes, "POST", "/businesses/9/associates", "bot", new_associate("3001112233")).await,
                   StatusCode::CREATED);
        assert_eq!(status_of(&routes, "POST", "/businesses/9/associates", "bot", new_associate(TAKEN_PHONE)).await,
                   StatusCode::CONFLICT);
    }
}
