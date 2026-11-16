use async_trait::async_trait;
use sqlx::PgPool;

use crate::agents::customer_service::domain::PendingVoiceNote;
use crate::agents::customer_service::ports::{RepositoryError, VoiceNoteStore};
use crate::domain::ChatKey;

/// Notas de voz del cliente de las últimas 24 h (las que el agente puede
/// estar respondiendo) que aún no tienen transcripción.
const PENDING: &str = r#"
    SELECT id AS message_id, media_id
    FROM messages
    WHERE business_id = $1 AND user_id = $2
      AND from_user AND media_type = 'audio'
      AND media_id IS NOT NULL AND transcription IS NULL
      AND created_at > (now() AT TIME ZONE 'UTC') - interval '24 hours'
    ORDER BY id DESC
    LIMIT $3
"#;

const SAVE: &str = "UPDATE messages SET transcription = $2 WHERE id = $1 AND transcription IS NULL";

#[derive(sqlx::FromRow)]
struct PendingRow {
    message_id: i32,
    media_id: String,
}

pub struct PgVoiceNoteStore {
    pool: PgPool,
}

impl PgVoiceNoteStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl VoiceNoteStore for PgVoiceNoteStore {
    async fn pending(&self, chat: &ChatKey, limit: i64) -> Result<Vec<PendingVoiceNote>, RepositoryError> {
        let rows: Vec<PendingRow> = sqlx::query_as(PENDING)
            .bind(chat.business_id)
            .bind(&chat.user_phone)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| PendingVoiceNote { message_id: row.message_id, media_id: row.media_id })
            .collect())
    }

    async fn save_transcription(&self, message_id: i32, text: &str) -> Result<(), RepositoryError> {
        sqlx::query(SAVE).bind(message_id).bind(text).execute(&self.pool).await?;
        Ok(())
    }
}
