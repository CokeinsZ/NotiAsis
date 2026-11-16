use async_trait::async_trait;
use chrono::NaiveDateTime;
use sqlx::PgPool;

use super::guide_rows::{GuideRow, guide_columns};
use crate::agents::customer_service::domain::{GuideDetails, NotificationEvent, NotificationKind};
use crate::agents::customer_service::ports::{GuideReader, RepositoryError};
use crate::domain::ChatKey;

/// La guía solo si es del cliente y del business del chat: el modelo
/// elige el número, pero nunca puede leer guías de otra persona.
const DETAILS: &str = concat!(
    "SELECT ",
    guide_columns!(),
    r#"
    FROM guides g
    JOIN businesses b ON b.id = g.business_id
    JOIN countries co ON co.id = b.country_id
    WHERE g.business_id = $1 AND g.user_id = $2 AND g.number = $3
    "#
);

const TIMELINE: &str = r#"
    SELECT
        n.kind,
        n.notification_step,
        ((n.created_at AT TIME ZONE 'UTC') AT TIME ZONE co.timezone) AS local_time
    FROM guide_notifications n
    JOIN guides g ON g.number = n.guide_number
    JOIN businesses b ON b.id = g.business_id
    JOIN countries co ON co.id = b.country_id
    WHERE g.business_id = $1 AND g.user_id = $2 AND g.number = $3
    ORDER BY n.created_at
"#;

#[derive(sqlx::FromRow)]
struct TimelineRow {
    kind: String,
    notification_step: i32,
    local_time: NaiveDateTime,
}

impl From<TimelineRow> for NotificationEvent {
    fn from(row: TimelineRow) -> Self {
        let kind = if row.kind == "tracking" { NotificationKind::Tracking } else { NotificationKind::Office };
        Self { kind, step: row.notification_step, at: row.local_time }
    }
}

pub struct PgGuideReader {
    pool: PgPool,
}

impl PgGuideReader {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GuideReader for PgGuideReader {
    async fn details(&self, chat: &ChatKey, guide_number: &str) -> Result<Option<GuideDetails>, RepositoryError> {
        let guide = sqlx::query_as::<_, GuideRow>(DETAILS)
            .bind(chat.business_id)
            .bind(&chat.user_phone)
            .bind(guide_number)
            .fetch_optional(&self.pool);
        let timeline = sqlx::query_as::<_, TimelineRow>(TIMELINE)
            .bind(chat.business_id)
            .bind(&chat.user_phone)
            .bind(guide_number)
            .fetch_all(&self.pool);
        let (guide, timeline) = tokio::try_join!(guide, timeline)?;

        Ok(guide.map(|row| {
            let (summary, order_id) = row.into_summary();
            GuideDetails { summary, order_id, timeline: timeline.into_iter().map(NotificationEvent::from).collect() }
        }))
    }
}
