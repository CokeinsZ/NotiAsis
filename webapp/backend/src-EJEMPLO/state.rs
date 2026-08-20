use std::sync::Arc;

use crate::users::service::{UserServiceTrait};
use crate::payments::service::{PaymentServiceTrait};

#[derive(Clone)]
pub struct AppState {
}

#[derive(Clone)]
pub struct UserState {
    pub user_service: Arc<dyn UserServiceTrait>,
    pub global_state: Arc<AppState>,
}

#[derive(Clone)]
pub struct PaymentState {
    pub payment_service: Arc<dyn PaymentServiceTrait>,
    pub global_state: Arc<AppState>,
}