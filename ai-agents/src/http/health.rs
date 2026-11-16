use axum::Json;
use serde_json::{Value, json};

/// `GET /health`: el proceso está arriba (no consulta dependencias).
pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
