use async_trait::async_trait;
use sqlx::PgPool;

use crate::agents::customer_service::domain::GuideUpdate;
use crate::agents::customer_service::ports::{GuideUpdateWriter, RepositoryError};
use crate::domain::ChatKey;

/// Inserta solo si la guía es del cliente y del business del chat.
const RECORD: &str = r#"
    INSERT INTO guide_customer_updates (guide_number, business_id, user_id, kind, promised_date, note)
    SELECT g.number, g.business_id, g.user_id, $4, $5, $6
    FROM guides g
    WHERE g.business_id = $1 AND g.user_id = $2 AND g.number = $3
"#;

pub struct PgGuideUpdateWriter {
    pool: PgPool,
}

impl PgGuideUpdateWriter {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GuideUpdateWriter for PgGuideUpdateWriter {
    async fn record(&self, chat: &ChatKey, update: &GuideUpdate) -> Result<bool, RepositoryError> {
        let result = sqlx::query(RECORD)
            .bind(chat.business_id)
            .bind(&chat.user_phone)
            .bind(&update.guide_number)
            .bind(update.kind.as_str())
            .bind(update.promised_date)
            .bind(&update.note)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
