use async_trait::async_trait;
use chrono::NaiveDateTime;

use crate::chats::dtos::{Chat, ChatSummary};

#[async_trait]
pub trait ChatRepositoryTrait: Send + Sync {
    async fn get_chats_by_business(&self, business_id: i32) -> Result<Vec<ChatSummary>, String>;
    async fn get_chat(&self, business_id: i32, user_id: &str) -> Result<Option<Chat>, String>;
    /// Crea el chat si no existe. No toca los datos del último mensaje.
    async fn upsert_chat(&self, business_id: i32, user_id: &str) -> Result<(), String>;
    /// Crea el chat si no existe y actualiza el último mensaje del usuario.
    async fn update_last_user_message(&self, business_id: i32, user_id: &str, message: &str, timestamp: NaiveDateTime) -> Result<(), String>;
    /// Business del chat más reciente de un usuario (para mensajes
    /// entrantes que no especifican business).
    async fn find_latest_chat_business(&self, user_id: &str) -> Result<Option<i32>, String>;
}

pub struct PostgresChatRepository {
    db_pool: sqlx::PgPool,
}

impl PostgresChatRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { db_pool: pool }
    }
}

#[async_trait]
impl ChatRepositoryTrait for PostgresChatRepository {
    async fn get_chats_by_business(&self, business_id: i32) -> Result<Vec<ChatSummary>, String> {
        let query = r#"
            SELECT
                c.business_id,
                c.user_id,
                u.full_name AS user_full_name,
                c.last_user_message,
                c.last_user_message_timestamp,
                (
                    SELECT MAX(m.created_at)
                    FROM messages m
                    WHERE m.business_id = c.business_id AND m.user_id = c.user_id
                ) AS last_activity
            FROM chats c
            JOIN users u ON u.phone_number = c.user_id
            WHERE c.business_id = $1
            ORDER BY last_activity DESC NULLS LAST
        "#;

        sqlx::query_as::<_, ChatSummary>(query)
            .bind(business_id)
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_chat(&self, business_id: i32, user_id: &str) -> Result<Option<Chat>, String> {
        let query = r#"
            SELECT business_id, user_id, last_user_message_timestamp, last_user_message
            FROM chats WHERE business_id = $1 AND user_id = $2
        "#;

        sqlx::query_as::<_, Chat>(query)
            .bind(business_id)
            .bind(user_id)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn upsert_chat(&self, business_id: i32, user_id: &str) -> Result<(), String> {
        let query = r#"
            INSERT INTO chats (business_id, user_id)
            VALUES ($1, $2)
            ON CONFLICT (business_id, user_id) DO NOTHING
        "#;

        sqlx::query(query)
            .bind(business_id)
            .bind(user_id)
            .execute(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    async fn update_last_user_message(&self, business_id: i32, user_id: &str, message: &str, timestamp: NaiveDateTime) -> Result<(), String> {
        let query = r#"
            INSERT INTO chats (business_id, user_id, last_user_message, last_user_message_timestamp)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (business_id, user_id) DO UPDATE
                SET last_user_message_timestamp = EXCLUDED.last_user_message_timestamp,
                    last_user_message = EXCLUDED.last_user_message
        "#;

        sqlx::query(query)
            .bind(business_id)
            .bind(user_id)
            .bind(message)
            .bind(timestamp)
            .execute(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    async fn find_latest_chat_business(&self, user_id: &str) -> Result<Option<i32>, String> {
        let query = r#"
            SELECT business_id FROM chats
            WHERE user_id = $1
            ORDER BY last_user_message_timestamp DESC NULLS LAST
            LIMIT 1
        "#;

        let row: Option<(i32,)> = sqlx::query_as(query)
            .bind(user_id)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(row.map(|r| r.0))
    }
}

