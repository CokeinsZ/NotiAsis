use axum::{
    Router, extract::{Json, Path, Query, State}, http::StatusCode, routing::{get, post}
};
use validator::Validate;

use crate::state::{UserState};
use crate::users::dtos::*;
use crate::payments::dtos::{Payment, payment_method, payment_state};
use crate::invoices::dtos::{Invoice, invoice_state};
use crate::tools::responses::{json_response, build_validation_response};

async fn create_user(
        State(state): State<UserState>,
        Json(user): Json<CreateUserDto>
    ) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = user.validate() {
        return build_validation_response(errors);
    }

    match state.user_service.create_user(user).await {
        Ok(new_user) => {
            json_response(
                StatusCode::CREATED,
                serde_json::json!({
                    "message": "User created",
                    "user": new_user
                }),
            )
        },
        Err(e) => {
            json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "message": e
                }),
            )
        }
    }
}

async fn get_user(
    State(state): State<UserState>,
    Path(id): Path<String>, 
    Query(filters): Query<UserFilters>
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = filters.validate() {
        return build_validation_response(errors);
    }

    if id == "-1" {
        // Find by filters
        match state.user_service.get_users_by_filters(&filters).await {
            Ok(users) => {
                json_response(
                    StatusCode::OK,
                    serde_json::json!({
                        "message": "Users retrieved",
                        "filters": filters,
                        "users": users
                    }),
                )
            },
            Err(e) => {
                json_response(
                    StatusCode::BAD_REQUEST,
                    serde_json::json!({
                        "message": e
                    }),
                )
            }
        }
    } else {
        // Find by ID
        match state.user_service.get_user(&id).await {
            Ok(user) => {
                json_response(
                    StatusCode::OK,
                    serde_json::json!({
                        "message": format!("User {id} retrieved"),
                        "users": user
                    }),
                )
            },
            Err(e) => {
                let status = if e == "User not found" { StatusCode::NOT_FOUND } else { StatusCode::BAD_REQUEST };
                json_response(
                    status,
                    serde_json::json!({
                        "message": e
                    }),
                )
            }
        }
    }
}

async fn update_user(
    State(state): State<UserState>,
    Path(id): Path<String>, 
    Json(body): Json<UpdateUserDto>
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(errors) = body.validate() {
        return build_validation_response(errors);
    }

    match state.user_service.update_user(&id, body).await {
        Ok(_) => {
            json_response(
                StatusCode::OK,
                serde_json::json!({
                    "message": format!("User {id} updated")
                }),
            )
        },
        Err(e) => {
            let status = if e == "User not found" { StatusCode::NOT_FOUND } else { StatusCode::BAD_REQUEST };
            json_response(
                status,
                serde_json::json!({
                    "message": e
                }),
            )
        }
    }
}

async fn delete_user(
    State(state): State<UserState>,
    Path(id): Path<String>
) -> (StatusCode, Json<serde_json::Value>) {
    match state.user_service.delete_user(&id).await {
        Ok(_) => {
            json_response(
                StatusCode::OK,
                serde_json::json!({
                    "message": format!("User {id} deleted")
                }),
            )
        },
        Err(e) => {
            let status = if e == "User not found" { StatusCode::NOT_FOUND } else { StatusCode::BAD_REQUEST };
            json_response(
                status,
                serde_json::json!({
                    "message": e
                }),
            )
        }
    }
}

async fn get_user_payments(Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    let payment = Payment {
        id: "payment_1".to_string(),
        amount: 100.0,
        method: payment_method::Card,
        full_payload: "{}".to_string(),
        created_at: "2023-01-01T00:00:00Z".to_string(),
        updated_at: "2023-01-01T00:00:00Z".to_string(),
        state: payment_state::Completed,
        invoice_id: "invoice_1".to_string()
    };
    
    json_response(
        StatusCode::NOT_IMPLEMENTED,
        serde_json::json!({
            "message": format!("User {id} payments"),
            "payments": [payment]
        }),
    )
}

async fn get_user_invoices(Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    let invoice = Invoice {
        id: "invoice_1".to_string(),
        user_id: id.clone(),
        total_amount: 100.0,
        description: "Sample invoice".to_string(),
        created_at: "2023-01-01T00:00:00Z".to_string(),
        updated_at: "2023-01-01T00:00:00Z".to_string(),
        state: invoice_state::Completed
    };
    
    json_response(
        StatusCode::NOT_IMPLEMENTED,
        serde_json::json!({
            "message": format!("User {id} invoices"),
            "invoices": [invoice]
        }),
    )
}

pub fn user_routes(state: UserState) -> Router {
    Router::new()
        .route("/", post(create_user))
        .route("/{id}", get(get_user).put(update_user).delete(delete_user))
        .route("/{id}/payments", get(get_user_payments))
        .route("/{id}/invoices", get(get_user_invoices))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;
    use std::sync::Arc;

    use crate::{state::AppState, users::service::UserService };

    async fn create_routes() -> Router {
        let app_state = AppState { };

        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://admin:secretpassword@localhost/mydb".into())
        ;
 
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to database")
        ;
    
        let user_repository = Arc::new(crate::users::repository::PostgresUserRepository::new(pool.clone()));

        let user_state = UserState {
            user_service: Arc::new(UserService::new(user_repository)),
            global_state: Arc::new(app_state),
        };
        user_routes(user_state)
    }

    #[tokio::test]
    async fn create_user() {
        let routes = create_routes().await;
        let unique_email = format!("john.doe{}@example.com", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());

        let user_data = CreateUserDto {
            full_name: "John Doe".to_string(),
            entity_type: user_entity_type::Natural,
            government_id: "1234567890-12".to_string(),
            email: unique_email,
            phone: "+571234567890".to_string(),
            password: "Password123".to_string(),
            address: "123 Main St".to_string(),
        };

        let response = routes
            .oneshot(
                Request::builder()
                    .uri("/")
                    .method("POST")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&user_data).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        
        let status = response.status();
        let headers = response.headers().clone();

        assert_eq!(status, StatusCode::CREATED);

        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_text = String::from_utf8(body_bytes.to_vec()).unwrap();

        println!("\n\nTest Create User Response:");
        println!("Headers: {:?}", headers);
        println!("Body: {}", body_text);

    }

    #[tokio::test]
    async fn create_invalid_user() {
        let invalid_cases = vec![
            (
                "Empty full name",
                CreateUserDto {
                    full_name: "".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "1234567890".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "1234567890".to_string(),
                    password: "Password123".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Short government ID",
                CreateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "123".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "1234567890".to_string(),
                    password: "Password123".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Invalid government ID",
                CreateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "1a23-45".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "1234567890".to_string(),
                    password: "Password123".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Long government ID",
                CreateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "123456789012".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "1234567890".to_string(),
                    password: "Password123".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Invalid email",
                CreateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "1234567890".to_string(),
                    email: "invalid-email".to_string(),
                    phone: "1234567890".to_string(),
                    password: "Password123".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Short phone",
                CreateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "1234567890".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "123".to_string(),
                    password: "Password123".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Invalid phone",
                CreateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "1234567890".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "cel:123456".to_string(),
                    password: "Password123".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Invalid password",
                CreateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "1234567890".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "1234567890".to_string(),
                    password: "pas".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Empty address",
                CreateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "1234567890".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "1234567890".to_string(),
                    password: "Password123".to_string(),
                    address: "".to_string(),
                },
            ),
        ];

        for (case_name, invalid_user_data) in invalid_cases {
            let routes = create_routes().await;

            let response = routes
                .oneshot(
                    Request::builder()
                        .uri("/")
                        .method("POST")
                        .header("Content-Type", "application/json")
                        .body(Body::from(serde_json::to_string(&invalid_user_data).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();

            let status = response.status();
            let headers = response.headers().clone();

            assert_eq!(status, StatusCode::BAD_REQUEST, "Failed: {case_name}");

            let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let body_text = String::from_utf8(body_bytes.to_vec()).unwrap();

            println!("\n\nTest Create Invalid User Response [{case_name}]");
            println!("Headers: {:?}", headers);
            println!("Body: {}", body_text);
        }
    }

    #[tokio::test]
    async fn get_user_by_id() {
        let routes = create_routes().await;

        let user_id = "123";
        let response = routes
            .oneshot(
                Request::builder()
                    .uri(&format!("/{}", user_id))
                    .method("GET")
                    .header("Content-Type", "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        
        let status = response.status();
        let headers = response.headers().clone();

        assert_eq!(status, StatusCode::NOT_FOUND);

        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_text = String::from_utf8(body_bytes.to_vec()).unwrap();

        println!("\n\n Test Get User Response:");
        println!("Headers: {:?}", headers);
        println!("Body: {}", body_text);

    }

    #[tokio::test]
    async fn get_user_by_filters() {
        let routes = create_routes().await;

        let user_id = "-1";
        let user_name = "John Doe";
        let government_id = "1234567890";
        let phone = "1234567890";
        
        let response = routes
            .oneshot(
                Request::builder()
                    .uri(&format!("/{user_id}?government_id={government_id}&phone={phone}"))
                    .method("GET")
                    .header("Content-Type", "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        
        let status = response.status();
        let headers = response.headers().clone();

        assert_eq!(status, StatusCode::OK);

        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_text = String::from_utf8(body_bytes.to_vec()).unwrap();
        let body_json: serde_json::Value = serde_json::from_str(&body_text).unwrap();

        let users = body_json["users"].as_array().expect("users must be an array");
        for user in users {
            let matches_name = user["full_name"] == user_name;
            let matches_government_id = user["government_id"] == government_id;
            let matches_phone = user["phone"] == phone;

            assert!(matches_name || matches_government_id || matches_phone,
                "User did not match at least one filter: {}", user
            );
        }
        
        println!("\n\n Test Get User Response:");
        println!("Headers: {:?}", headers);
        println!("Body: {}", body_text);

    }

    #[tokio::test]
    async fn update_user() {
        let routes = create_routes().await;

        let user_id = "123";
        let user_data = UpdateUserDto {
            full_name: "John Doe".to_string(),
            entity_type: user_entity_type::Natural,
            government_id: "1234567890-12".to_string(),
            email: "john.doe@example.com".to_string(),
            phone: "+571234567890".to_string(),
            address: "123 Main St".to_string(),
        };

        let response = routes
            .oneshot(
                Request::builder()
                    .uri(&format!("/{user_id}"))
                    .method("PUT")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&user_data).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        
        let status = response.status();
        let headers = response.headers().clone();

        assert_eq!(status, StatusCode::NOT_FOUND);

        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_text = String::from_utf8(body_bytes.to_vec()).unwrap();

        println!("\n\n Test Update User Response:");
        println!("Headers: {:?}", headers);
        println!("Body: {}", body_text);

    }

    #[tokio::test]
    async fn update_invalid_user() {
        let user_id = "123";
        let invalid_cases = vec![
            (
                "Empty full name",
                UpdateUserDto {
                    full_name: "".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "1234567890".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "1234567890".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Short government ID",
                UpdateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "123".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "1234567890".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Invalid government ID",
                UpdateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "1a23-45".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "1234567890".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Long government ID",
                UpdateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "123456789012".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "1234567890".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Invalid email",
                UpdateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "1234567890".to_string(),
                    email: "invalid-email".to_string(),
                    phone: "1234567890".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Short phone",
                UpdateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "1234567890".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "123".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Invalid phone",
                UpdateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "1234567890".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "cel:123456".to_string(),
                    address: "123 Main St".to_string(),
                },
            ),
            (
                "Empty address",
                UpdateUserDto {
                    full_name: "John Doe".to_string(),
                    entity_type: user_entity_type::Natural,
                    government_id: "1234567890".to_string(),
                    email: "john.doe@example.com".to_string(),
                    phone: "1234567890".to_string(),
                    address: "".to_string(),
                },
            ),
        ];

        for (case_name, invalid_user_data) in invalid_cases {
            let routes = create_routes().await;

            let response = routes
                .oneshot(
                    Request::builder()
                        .uri(&format!("/{user_id}"))
                        .method("PUT")
                        .header("Content-Type", "application/json")
                        .body(Body::from(serde_json::to_string(&invalid_user_data).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();

            let status = response.status();
            let headers = response.headers().clone();

            assert_eq!(status, StatusCode::BAD_REQUEST, "Failed: {case_name}");

            let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let body_text = String::from_utf8(body_bytes.to_vec()).unwrap();

            println!("\n\nTest Update Invalid User Response [{case_name}]");
            println!("Headers: {:?}", headers);
            println!("Body: {}", body_text);
        }
    }

    #[tokio::test]
    async fn delete_user() {
        let routes = create_routes().await;
        let user_id = "123";

        let response = routes
            .oneshot(
                Request::builder()
                    .uri(&format!("/{user_id}"))
                    .method("DELETE")
                    .header("Content-Type", "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let status = response.status();
        let headers = response.headers().clone();

        assert_eq!(status, StatusCode::NOT_FOUND);

        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_text = String::from_utf8(body_bytes.to_vec()).unwrap();

        println!("\n\n Test Delete User Response:");
        println!("Headers: {:?}", headers);
        println!("Body: {}", body_text);
    }

    #[tokio::test]
    async fn get_user_payments() {
        let routes = create_routes().await;
        let user_id = "123";

        let response = routes
            .oneshot(
                Request::builder()
                    .uri(&format!("/{user_id}/payments"))
                    .method("GET")
                    .header("Content-Type", "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let status = response.status();
        let headers = response.headers().clone();

        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);

        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_text = String::from_utf8(body_bytes.to_vec()).unwrap();

        println!("\n\n Test Get User Payments Response:");
        println!("Headers: {:?}", headers);
        println!("Body: {}", body_text);
    }

    #[tokio::test]
    async fn get_user_invoices() {
        let routes = create_routes().await;
        let user_id = "123";

        let response = routes
            .oneshot(
                Request::builder()
                    .uri(&format!("/{user_id}/invoices"))
                    .method("GET")
                    .header("Content-Type", "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let status = response.status();
        let headers = response.headers().clone();

        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);

        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_text = String::from_utf8(body_bytes.to_vec()).unwrap();

        println!("\n\n Test Get User Invoices Response:");
        println!("Headers: {:?}", headers);
        println!("Body: {}", body_text);
    }
}