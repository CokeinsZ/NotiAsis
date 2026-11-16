use async_trait::async_trait;
use sqlx::PgPool;

use crate::agents::customer_service::domain::Escalation;
use crate::agents::customer_service::ports::{EscalationWriter, RepositoryError};
use crate::domain::ChatKey;

/// La guía se guarda solo si es del cliente (si no, queda NULL).
const INSERT: &str = r#"
    INSERT INTO ai_escalations (business_id, user_id, guide_number, category, reason)
    VALUES (
        $1, $2,
        (SELECT g.number FROM guides g WHERE g.number = $3 AND g.business_id = $1 AND g.user_id = $2),
        $4, $5
    )
"#;

/// Los chats importantes salen primero en la bandeja de la webapp.
const MARK_IMPORTANT: &str = "UPDATE chats SET is_important = true WHERE business_id = $1 AND user_id = $2";

pub struct PgEscalationWriter {
    pool: PgPool,
}

impl PgEscalationWriter {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EscalationWriter for PgEscalationWriter {
    async fn escalate(&self, chat: &ChatKey, escalation: &Escalation) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(INSERT)
            .bind(chat.business_id)
            .bind(&chat.user_phone)
            .bind(&escalation.guide_number)
            .bind(escalation.category.as_str())
            .bind(&escalation.reason)
            .execute(&mut *tx)
            .await?;
        sqlx::query(MARK_IMPORTANT).bind(chat.business_id).bind(&chat.user_phone).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
