use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub enum invoice_state {
    Pending,
    Completed,
    Failed,
    Refunded
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Invoice {
    pub id: String,
    pub user_id: String,
    pub total_amount: f64,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
    pub state: invoice_state,
}