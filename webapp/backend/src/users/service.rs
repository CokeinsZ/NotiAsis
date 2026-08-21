use std::sync::Arc;

use async_trait::async_trait;

use crate::users::dtos::User;
use crate::users::repository::UserRepositoryTrait;

#[async_trait]
pub trait UserServiceTrait: Send + Sync {
    async fn get_users(&self) -> Result<Vec<User>, String>;
    async fn get_user(&self, phone_number: &str) -> Result<User, String>;
}

pub struct UserService {
    repository: Arc<dyn UserRepositoryTrait>,
}

impl UserService {
    pub fn new(repository: Arc<dyn UserRepositoryTrait>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl UserServiceTrait for UserService {
    async fn get_users(&self) -> Result<Vec<User>, String> {
        self.repository.get_users().await
    }

    async fn get_user(&self, phone_number: &str) -> Result<User, String> {
        match self.repository.get_user_by_phone(phone_number).await? {
            Some(user) => Ok(user),
            None => Err("User not found".to_string()),
        }
    }
}
