use axum::{Router, middleware};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

mod auth;
mod businesses;
mod chats;
mod guides;
mod messages;
mod state;
mod tools;
mod users;

use auth::controller::{associate_admin_routes, auth_routes};
use auth::service::AuthServiceTrait;
use businesses::controller::{associate_routes, business_routes};
use chats::controller::chat_routes;
use guides::controller::guide_routes;
use messages::controller::message_routes;
use messages::meta_client::MetaClientTrait;
use state::{AppState, AssociateAdminState, AuthState, BusinessState, ChatState, GuideState, MessageState, UserState};
use users::controller::user_routes;

fn build_app(pool: sqlx::PgPool, meta_client: Arc<dyn MetaClientTrait>, jwt_secret: String) -> Router {
    let app_state_pointer = Arc::new(AppState { });

    // Repositorios (comparten el pool de conexiones)
    let auth_repository = Arc::new(auth::repository::PostgresAuthRepository::new(pool.clone()));
    let business_repository = Arc::new(businesses::repository::PostgresBusinessRepository::new(pool.clone()));
    let user_repository = Arc::new(users::repository::PostgresUserRepository::new(pool.clone()));
    let chat_repository = Arc::new(chats::repository::PostgresChatRepository::new(pool.clone()));
    let message_repository = Arc::new(messages::repository::PostgresMessageRepository::new(pool.clone()));
    let guide_repository = Arc::new(guides::repository::PostgresGuideRepository::new(pool.clone()));

    // Servicios
    let auth_service: Arc<dyn AuthServiceTrait> =
        Arc::new(auth::service::AuthService::new(auth_repository, jwt_secret));
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
    let auth_state = AuthState {
        auth_service: auth_service.clone(),
        global_state: app_state_pointer.clone(),
    };
    let associate_admin_state = AssociateAdminState {
        auth_service: auth_service.clone(),
        global_state: app_state_pointer.clone(),
    };
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
        auth_service: auth_service.clone(),
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

    // Rutas de asociados: consulta (BusinessState) + administración (AssociateAdminState)
    let associates_router = associate_routes(business_state.clone())
        .merge(associate_admin_routes(associate_admin_state));

    // Todos los módulos excepto /auth requieren un JWT válido.
    let protected = Router::new()
        .nest("/businesses", business_routes(business_state.clone()))
        .nest("/associates", associates_router)
        .nest("/users", user_routes(user_state))
        .nest("/chats", chat_routes(chat_state))
        .nest("/messages", message_routes(message_state))
        .nest("/guides", guide_routes(guide_state))
        .layer(middleware::from_fn_with_state(
            auth_service,
            auth::middleware::require_auth,
        ));

    Router::new()
        .nest("/auth", auth_routes(auth_state))
        .merge(protected)
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://admin:secretpassword@localhost/notiasis".into())
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

    let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");

    let app = build_app(pool, meta_client, jwt_secret);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
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

        build_app(pool, meta_client, "test-secret".into())
    }

    #[tokio::test]
    async fn app_builds_all_routes_without_panicking() {
        // Si dos rutas colisionaran, axum haría panic aquí.
        let _app = create_app_without_db();
    }

    #[tokio::test]
    async fn protected_routes_require_token() {
        // Sin token: 401 sin tocar la base de datos.
        for uri in [
            "/chats?business_id=1",
            "/businesses",
            "/users",
            "/associates",
            "/guides",
            "/messages/incoming",
        ] {
            let response = create_app_without_db()
                .oneshot(Request::builder().uri(uri).method("GET").body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "uri: {uri}");
        }
    }

    #[tokio::test]
    async fn auth_routes_do_not_require_token() {
        // Login con credenciales inválidas debe llegar al servicio y
        // responder 401/400 (no quedar bloqueado por el middleware).
        let response = create_app_without_db()
            .oneshot(
                Request::builder()
                    .uri("/auth/login")
                    .method("POST")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"username": "", "password": ""}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body_json["message"], "Invalid data");
    }
}
