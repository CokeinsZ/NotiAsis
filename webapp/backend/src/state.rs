use std::sync::Arc;

use crate::auth::service::AuthServiceTrait;
use crate::businesses::service::BusinessServiceTrait;
use crate::chats::service::ChatServiceTrait;
use crate::guides::service::GuideServiceTrait;
use crate::messages::service::MessageServiceTrait;
use crate::users::service::UserServiceTrait;

#[derive(Clone)]
pub struct AppState {
}

#[derive(Clone)]
pub struct AuthState {
    pub auth_service: Arc<dyn AuthServiceTrait>,
    pub global_state: Arc<AppState>,
}

#[derive(Clone)]
pub struct BusinessState {
    pub business_service: Arc<dyn BusinessServiceTrait>,
    /// Para las estadísticas del dashboard (notificaciones de guías).
    pub guide_service: Arc<dyn GuideServiceTrait>,
    pub global_state: Arc<AppState>,
}

/// Estado para las rutas administrativas de asociados (cambio de contraseña).
#[derive(Clone)]
pub struct AssociateAdminState {
    pub auth_service: Arc<dyn AuthServiceTrait>,
    pub global_state: Arc<AppState>,
}

#[derive(Clone)]
pub struct UserState {
    pub user_service: Arc<dyn UserServiceTrait>,
    pub global_state: Arc<AppState>,
}

#[derive(Clone)]
pub struct ChatState {
    pub chat_service: Arc<dyn ChatServiceTrait>,
    pub message_service: Arc<dyn MessageServiceTrait>,
    pub auth_service: Arc<dyn AuthServiceTrait>,
    pub global_state: Arc<AppState>,
}

#[derive(Clone)]
pub struct MessageState {
    pub message_service: Arc<dyn MessageServiceTrait>,
    pub global_state: Arc<AppState>,
}

#[derive(Clone)]
pub struct GuideState {
    pub guide_service: Arc<dyn GuideServiceTrait>,
    pub global_state: Arc<AppState>,
}
