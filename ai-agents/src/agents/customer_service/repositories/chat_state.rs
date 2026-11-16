use async_trait::async_trait;
use chrono::NaiveDateTime;
use sqlx::PgPool;

use crate::agents::customer_service::domain::{AgentSwitch, ChatSnapshot, IncomingRef};
use crate::agents::customer_service::ports::{ChatStateReader, RepositoryError};
use crate::domain::ChatKey;

const SNAPSHOT: &str = r#"
    SELECT
        (now() AT TIME ZONE 'UTC') AS now_utc,
        li.id AS latest_incoming_id,
        li.created_at AS latest_incoming_at,
        (SELECT max(m.id) FROM messages m
          WHERE m.business_id = c.business_id AND m.user_id = c.user_id AND m.sender = 'ai') AS last_ai_reply_id,
        (SELECT max(m.created_at) FROM messages m
          WHERE m.business_id = c.business_id AND m.user_id = c.user_id AND m.sender = 'associate')
          AS last_associate_message_at,
        (SELECT max(e.created_at) FROM ai_escalations e
          WHERE e.business_id = c.business_id AND e.user_id = c.user_id) AS last_escalation_at,
        (SELECT count(*) FROM messages m
          WHERE m.business_id = c.business_id AND m.user_id = c.user_id AND m.sender = 'ai'
            AND m.created_at > (now() AT TIME ZONE 'UTC') - interval '1 hour') AS ai_replies_last_hour,
        a.enabled AS agent_enabled,
        a.human_takeover_hours::int8 AS human_takeover_hours
    FROM chats c
    LEFT JOIN business_ai_agents a ON a.business_id = c.business_id
    LEFT JOIN LATERAL (
        SELECT m.id, m.created_at FROM messages m
        WHERE m.business_id = c.business_id AND m.user_id = c.user_id AND m.from_user
        ORDER BY m.id DESC
        LIMIT 1
    ) li ON true
    WHERE c.business_id = $1 AND c.user_id = $2
"#;

#[derive(sqlx::FromRow)]
struct SnapshotRow {
    now_utc: NaiveDateTime,
    latest_incoming_id: Option<i32>,
    latest_incoming_at: Option<NaiveDateTime>,
    last_ai_reply_id: Option<i32>,
    last_associate_message_at: Option<NaiveDateTime>,
    last_escalation_at: Option<NaiveDateTime>,
    ai_replies_last_hour: i64,
    agent_enabled: Option<bool>,
    human_takeover_hours: Option<i64>,
}

impl From<SnapshotRow> for ChatSnapshot {
    fn from(row: SnapshotRow) -> Self {
        Self {
            now: row.now_utc,
            latest_incoming: row.latest_incoming_id.zip(row.latest_incoming_at).map(|(id, at)| IncomingRef { id, at }),
            last_ai_reply_id: row.last_ai_reply_id,
            last_associate_message_at: row.last_associate_message_at,
            last_escalation_at: row.last_escalation_at,
            ai_replies_last_hour: row.ai_replies_last_hour,
            agent: row
                .agent_enabled
                .map(|enabled| AgentSwitch { enabled, human_takeover_hours: row.human_takeover_hours.unwrap_or(0) }),
        }
    }
}

pub struct PgChatStateReader {
    pool: PgPool,
}

impl PgChatStateReader {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ChatStateReader for PgChatStateReader {
    async fn snapshot(&self, chat: &ChatKey) -> Result<Option<ChatSnapshot>, RepositoryError> {
        let row: Option<SnapshotRow> =
            sqlx::query_as(SNAPSHOT).bind(chat.business_id).bind(&chat.user_phone).fetch_optional(&self.pool).await?;
        Ok(row.map(ChatSnapshot::from))
    }
}
