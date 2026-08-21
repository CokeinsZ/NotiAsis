use axum::Router;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

mod businesses;
mod chats;
mod guides;
mod messages;
mod state;
mod tools;
mod users;

use businesses::controller::{associate_routes, business_routes};
use chats::controller::chat_routes;
use guides::controller::guide_routes;
use messages::controller::message_routes;
use messages::meta_client::MetaClientTrait;
use state::{AppState, BusinessState, ChatState, GuideState, MessageState, UserState};
use users::controller::user_routes;

fn build_app(pool: sqlx::PgPool, meta_client: Arc<dyn MetaClientTrait>) -> Router {
    let app_state_pointer = Arc::new(AppState { });

    // Repositorios (comparten el pool de conexiones)
    let business_repository = Arc::new(businesses::repository::PostgresBusinessRepository::new(pool.clone()));
    let user_repository = Arc::new(users::repository::PostgresUserRepository::new(pool.clone()));
    let chat_repository = Arc::new(chats::repository::PostgresChatRepository::new(pool.clone()));
    let message_repository = Arc::new(messages::repository::PostgresMessageRepository::new(pool.clone()));
    let guide_repository = Arc::new(guides::repository::PostgresGuideRepository::new(pool.clone()));

    // Servicios
    let business_service = Arc::new(businesses::service::BusinessService::new(business_repository));
    let user_service = Arc::new(users::service::UserService::new(user_repository.clone()));
    let chat_service = Arc::new(chats::service::ChatService::new(chat_repository.clone()));
    let message_service = Arc::new(messages::service::MessageService::new(
        message_repository,
        chat_repository,
        user_repository.clone(),
        meta_client,
    ));
    let guide_service = Arc::new(guides::service::GuideService::new(guide_repository, user_repository));

    // Estados por módulo
    let business_state = BusinessState {
        business_service,
        global_state: app_state_pointer.clone(),
    };
    let user_state = UserState {
        user_service,
        global_state: app_state_pointer.clone(),
    };
    let chat_state = ChatState {
        chat_service,
        message_service: message_service.clone(),
        global_state: app_state_pointer.clone(),
    };
    let message_state = MessageState {
        message_service,
        global_state: app_state_pointer.clone(),
    };
    let guide_state = GuideState {
        guide_service,
        global_state: app_state_pointer.clone(),
    };

    Router::new()
        .nest("/businesses", business_routes(business_state.clone()))
        .nest("/associates", associate_routes(business_state))
        .nest("/users", user_routes(user_state))
        .nest("/chats", chat_routes(chat_state))
        .nest("/messages", message_routes(message_state))
        .nest("/guides", guide_routes(guide_state))
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://neondb_owner:npg_bR0ixfBtkD1a@ep-silent-bird-achd4b7s-pooler.sa-east-1.aws.neon.tech/neondb?sslmode=require&channel_binding=require".into())
    ;

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database")
    ;

    // Cliente de la WhatsApp Cloud API para los mensajes libres de la webapp
    let meta_client: Arc<dyn MetaClientTrait> = Arc::new(messages::meta_client::MetaClient::new(
        std::env::var("WHATSAPP_TOKEN").expect("WHATSAPP_TOKEN must be set"),
        std::env::var("WHATSAPP_PHONE_ID").expect("WHATSAPP_PHONE_ID must be set"),
    ));

    let app = build_app(pool, meta_client);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap()
    ;
    println!("Listening on http://localhost:{port}");
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    /// Construye la app real sin conectar a la base de datos
    /// (el pool lazy solo conecta al ejecutar una query).
    fn create_app_without_db() -> Router {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://admin:secretpassword@localhost/notiasis")
            .unwrap();

        let meta_client: Arc<dyn MetaClientTrait> = Arc::new(
            messages::meta_client::MetaClient::new("dummy".into(), "dummy".into())
        );

        build_app(pool, meta_client)
    }

    #[tokio::test]
    async fn app_builds_all_routes_without_panicking() {
        // Si dos rutas colisionaran, axum haría panic aquí.
        let _app = create_app_without_db();
    }

    #[tokio::test]
    async fn invalid_incoming_payload_returns_400_without_db() {
        let response = create_app_without_db()
            .oneshot(
                Request::builder()
                    .uri("/messages/incoming")
                    .method("POST")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"user_phone": "123", "meta_message_id": "wamid.x", "media_type": "text"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Falla la validación del DTO antes de tocar la base de datos.
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body_json["message"], "Invalid data");
    }

    #[tokio::test]
    async fn chats_query_validation_runs_before_db() {
        let response = create_app_without_db()
            .oneshot(
                Request::builder()
                    .uri("/chats?business_id=0")
                    .method("GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
