use std::sync::Arc;
use async_trait::async_trait;

use crate::users::{dtos::{CreateUserDto, UpdateUserDto, User, UserFilters}, repository::UserRepositoryTrait};

#[async_trait]
pub trait UserServiceTrait: Send + Sync {
    async fn create_user(&self, dto: CreateUserDto) -> Result<User, String>;
    async fn get_user(&self, id: &str) -> Result<User, String>;
    async fn get_users_by_filters(&self, filters: &UserFilters) -> Result<Vec<User>, String>;
    async fn update_user(&self, id: &str, dto: UpdateUserDto) -> Result<(), String>;
    async fn delete_user(&self, id: &str) -> Result<(), String>;
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
    async fn create_user(&self, dto: CreateUserDto) -> Result<User, String> {
        if self.repository.email_exists(&dto.email).await? {
            return Err("Email already exists".to_string());
        }
        let new_user = self.repository.save_user(&dto).await?;
        Ok(new_user)
    }

    async fn get_user(&self, id: &str) -> Result<User, String> {
        match self.repository.get_user_by_id(id).await? {
            Some(user) => Ok(user),
            None => Err("User not found".to_string()),
        }
    }

    async fn get_users_by_filters(&self, filters: &UserFilters) -> Result<Vec<User>, String> {
        self.repository.get_users_by_filters(filters).await
    }

    async fn update_user(&self, id: &str, dto: UpdateUserDto) -> Result<(), String> {
        let existing_user = self.repository.get_user_by_id(id).await?;
        if existing_user.is_none() {
            return Err("User not found".to_string());
        }

        let existing_user = existing_user.unwrap();
        if existing_user.email != dto.email {
            if self.repository.email_exists(&dto.email).await? {
                return Err("Email already in use".to_string());
            }
        }

        self.repository.update_user(id, &dto).await?;
        Ok(())
    }

    async fn delete_user(&self, id: &str) -> Result<(), String> {
        let existing_user = self.repository.get_user_by_id(id).await?;
        if existing_user.is_none() {
            return Err("User not found".to_string());
        }

        self.repository.delete_user(id).await?;
        Ok(())
    }
}
