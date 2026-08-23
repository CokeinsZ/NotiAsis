use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[derive(sqlx::FromRow)]
pub struct User {
    pub phone_number: String,
    pub full_name: String,
}
