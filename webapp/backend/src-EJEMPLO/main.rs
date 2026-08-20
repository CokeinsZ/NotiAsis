use axum::{Router};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

mod users;
mod tools;
mod invoices;
mod payments;
mod state;

use users::controller::user_routes;
use payments::controller::payment_routes;

use state::{AppState, UserState, PaymentState};


#[tokio::main]
async fn main() {
    let app_state_pointer = Arc::new(AppState { });

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://admin:secretpassword@localhost/mydb".into())
    ;

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database")
    ;
 
    let user_repository = Arc::new(users::repository::PostgresUserRepository::new(pool.clone()));
    
    let user_state = UserState {
        user_service: Arc::new(users::service::UserService::new(user_repository)),
        global_state: app_state_pointer.clone(),
    };
    let payment_state = PaymentState {
        payment_service: Arc::new(payments::service::PaymentService {}),
        global_state: app_state_pointer.clone(),
    };

    let app = Router::new()
        .nest("/users", user_routes(user_state))
        .nest("/payments", payment_routes(payment_state))
    ;

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap()
    ;
    println!("Listening on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}