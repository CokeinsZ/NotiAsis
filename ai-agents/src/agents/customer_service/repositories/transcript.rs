use async_trait::async_trait;
use chrono::NaiveDateTime;
use sqlx::PgPool;

use crate::agents::customer_service::domain::{Author, MediaKind, QuotedMessage, TranscriptEntry};
use crate::agents::customer_service::ports::{RepositoryError, TranscriptReader};
use crate::domain::ChatKey;

/// Los mensajes más recientes primero (se invierten al leerlos). Por
/// `created_at`: en los entrantes es la hora de Meta, el orden real. Cada
/// uno con el mensaje que citó, si es del mismo chat.
const RECENT: &str = r#"
    SELECT
        ((m.created_at AT TIME ZONE 'UTC') AT TIME ZONE co.timezone) AS local_time,
        m.from_user,
        m.sender,
        m.media_type::text AS media_type,
        m.status::text AS status,
        m.guide_number,
        m.message,
        m.transcription,
        ((q.created_at AT TIME ZONE 'UTC') AT TIME ZONE co.timezone) AS quoted_time,
        q.from_user AS quoted_from_user,
        q.sender AS quoted_sender,
        q.media_type::text AS quoted_media_type,
        q.message AS quoted_message,
        q.transcription AS quoted_transcription
    FROM messages m
    JOIN businesses b ON b.id = m.business_id
    JOIN countries co ON co.id = b.country_id
    LEFT JOIN messages q
        ON q.id = m.reply_to_message_id AND q.business_id = m.business_id AND q.user_id = m.user_id
    WHERE m.business_id = $1 AND m.user_id = $2
    ORDER BY m.created_at DESC, m.id DESC
    LIMIT $3
"#;

#[derive(sqlx::FromRow)]
struct TranscriptRow {
    local_time: NaiveDateTime,
    from_user: bool,
    sender: Option<String>,
    media_type: String,
    status: Option<String>,
    guide_number: Option<String>,
    message: Option<String>,
    transcription: Option<String>,
    quoted_time: Option<NaiveDateTime>,
    quoted_from_user: Option<bool>,
    quoted_sender: Option<String>,
    quoted_media_type: Option<String>,
    quoted_message: Option<String>,
    quoted_transcription: Option<String>,
}

impl From<TranscriptRow> for TranscriptEntry {
    fn from(row: TranscriptRow) -> Self {
        let quoted = match (row.quoted_time, row.quoted_from_user, row.quoted_media_type) {
            (Some(at), Some(from_user), Some(media_type)) => Some(QuotedMessage {
                at,
                author: Author::from_columns(from_user, row.quoted_sender.as_deref()),
                media: MediaKind::from_column(&media_type),
                text: row.quoted_message,
                transcription: row.quoted_transcription,
            }),
            _ => None,
        };
        Self {
            at: row.local_time,
            author: Author::from_columns(row.from_user, row.sender.as_deref()),
            media: MediaKind::from_column(&row.media_type),
            text: row.message,
            transcription: row.transcription,
            guide_number: row.guide_number,
            failed: row.status.as_deref() == Some("failed"),
            quoted,
        }
    }
}

pub struct PgTranscriptReader {
    pool: PgPool,
}

impl PgTranscriptReader {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TranscriptReader for PgTranscriptReader {
    async fn recent(&self, chat: &ChatKey, limit: i64) -> Result<Vec<TranscriptEntry>, RepositoryError> {
        let rows: Vec<TranscriptRow> = sqlx::query_as(RECENT)
            .bind(chat.business_id)
            .bind(&chat.user_phone)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().rev().map(TranscriptEntry::from).collect())
    }
}
