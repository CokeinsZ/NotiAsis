use serde::{Deserialize, Serialize};
use validator::{Validate};

use crate::invoices::dtos::{Invoice};
use crate::tools::custom_validators::{validate_numeric};

#[derive(Debug, Deserialize, Serialize)]
pub enum payment_method {
    Card,
    PSE,
    Nequi,
    Cash
}

#[derive(Debug, Deserialize, Serialize)]
pub enum payment_state {
    Pending,
    Completed,
    Failed,
    Refunded
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Payment {
    pub id: String,
    pub amount: f64,
    pub method: payment_method,
    pub full_payload: String,
    pub created_at: String,
    pub updated_at: String,
    pub state: payment_state,
    pub invoice_id: String
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PopulatedPayment {
    pub id: String,
    pub amount: f64,
    pub method: payment_method,
    pub full_payload: String,
    pub created_at: String,
    pub updated_at: String,
    pub state: payment_state,
    pub invoice_id: String,
    pub invoice: Invoice
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreatePaymentDto {
    #[validate(custom(function = "validate_numeric"))]
    pub invoice_id: String
}