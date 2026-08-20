use axum::{
    Json,
    http::StatusCode, 
};

pub fn json_response(status: StatusCode, body: serde_json::Value) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(body))
}

pub fn build_validation_response(errors: validator::ValidationErrors) -> (StatusCode, Json<serde_json::Value>) {
    let mut error_messages = Vec::new();

    for (field, field_errors) in errors.field_errors() {
        for error in field_errors {
            let message = error.message.as_deref().unwrap_or("invalid value");
            error_messages.push(format!("{field}: {message}"));
        }
    }

    json_response(
        StatusCode::BAD_REQUEST,
        serde_json::json!({
            "message": "Invalid user data",
            "errors": error_messages
        }),
    )
}
