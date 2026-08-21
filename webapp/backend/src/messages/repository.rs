use async_trait::async_trait;
use chrono::NaiveDateTime;

use crate::messages::dtos::{MediaType, Message, MessageStatus};

/// Datos necesarios para persistir un mensaje nuevo.
pub struct NewMessage {
    pub meta_message_id: String,
    pub business_id: i32,
    pub user_id: String,
    pub media_id: Option<String>,
    pub media_type: MediaType,
    pub message: Option<String>,
    pub status: Option<MessageStatus>,
    pub from_user: bool,
    /// Si es None, la base de datos usa CURRENT_TIMESTAMP.
    pub created_at: Option<NaiveDateTime>,
}

#[async_trait]
pub trait MessageRepositoryTrait: Send + Sync {
    /// Guarda el mensaje. Si el meta_message_id ya existe (Meta puede
    /// reenviar webhooks), retorna el mensaje ya guardado.
    async fn save_message(&self, message: &NewMessage) -> Result<Message, String>;
    async fn get_messages_by_chat(&self, business_id: i32, user_id: &str) -> Result<Vec<Message>, String>;
    /// Actualiza el estado de un mensaje saliente. Nunca retrocede un
    /// mensaje ya marcado como 'read'. Retorna si se actualizó algo.
    async fn update_status_by_meta_id(&self, meta_message_id: &str, status: MessageStatus) -> Result<bool, String>;
}

pub struct PostgresMessageRepository {
    db_pool: sqlx::PgPool,
}

impl PostgresMessageRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { db_pool: pool }
    }
}

#[async_trait]
impl MessageRepositoryTrait for PostgresMessageRepository {
    async fn save_message(&self, message: &NewMessage) -> Result<Message, String> {
        let query = r#"
            INSERT INTO messages (
                meta_message_id, business_id, user_id,
                media_id, media_type, message, status, from_user, created_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, COALESCE($9::timestamp, CURRENT_TIMESTAMP)
            )
            ON CONFLICT (meta_message_id) DO NOTHING
            RETURNING id, meta_message_id, business_id, user_id,
                      media_id, media_type, message, status, from_user, created_at
        "#;

        let inserted: Option<Message> = sqlx::query_as(query)
            .bind(&message.meta_message_id)
            .bind(message.business_id)
            .bind(&message.user_id)
            .bind(&message.media_id)
            .bind(message.media_type)
            .bind(&message.message)
            .bind(message.status)
            .bind(message.from_user)
            .bind(message.created_at)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        if let Some(row) = inserted {
            return Ok(row);
        }

        // Webhook duplicado: el mensaje ya existía.
        let existing = sqlx::query_as::<_, Message>(
            r#"SELECT id, meta_message_id, business_id, user_id,
                      media_id, media_type, message, status, from_user, created_at
               FROM messages WHERE meta_message_id = $1"#
        )
            .bind(&message.meta_message_id)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(existing)
    }

    async fn get_messages_by_chat(&self, business_id: i32, user_id: &str) -> Result<Vec<Message>, String> {
        let query = r#"
            SELECT id, meta_message_id, business_id, user_id,
                   media_id, media_type, message, status, from_user, created_at
            FROM messages
            WHERE business_id = $1 AND user_id = $2
            ORDER BY created_at ASC, id ASC
        "#;

        sqlx::query_as::<_, Message>(query)
            .bind(business_id)
            .bind(user_id)
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn update_status_by_meta_id(&self, meta_message_id: &str, status: MessageStatus) -> Result<bool, String> {
        let query = r#"
            UPDATE messages SET status = $1
            WHERE meta_message_id = $2 AND status != 'read'
        "#;

        let result = sqlx::query(query)
            .bind(status)
            .bind(meta_message_id)
            .execute(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(result.rows_affected() > 0)
    }
}
