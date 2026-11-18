use async_trait::async_trait;

use crate::ai_activity::dtos::{AiEscalation, GuideCustomerUpdate};

#[async_trait]
pub trait AiActivityRepositoryTrait: Send + Sync {
    /// Escalaciones del chat, de la más antigua a la más reciente.
    async fn get_escalations(&self, business_id: i32, user_id: &str) -> Result<Vec<AiEscalation>, String>;
    /// Novedades de guías registradas en el chat, de la más antigua a la
    /// más reciente.
    async fn get_guide_updates(&self, business_id: i32, user_id: &str) -> Result<Vec<GuideCustomerUpdate>, String>;
}

pub struct PostgresAiActivityRepository {
    db_pool: sqlx::PgPool,
}

impl PostgresAiActivityRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { db_pool: pool }
    }
}

#[async_trait]
impl AiActivityRepositoryTrait for PostgresAiActivityRepository {
    async fn get_escalations(&self, business_id: i32, user_id: &str) -> Result<Vec<AiEscalation>, String> {
        let query = r#"
            SELECT id, category, reason, guide_number, created_at
            FROM ai_escalations
            WHERE business_id = $1 AND user_id = $2
            ORDER BY created_at ASC, id ASC
        "#;

        sqlx::query_as::<_, AiEscalation>(query)
            .bind(business_id)
            .bind(user_id)
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_guide_updates(&self, business_id: i32, user_id: &str) -> Result<Vec<GuideCustomerUpdate>, String> {
        let query = r#"
            SELECT id, guide_number, kind, promised_date, note, created_at
            FROM guide_customer_updates
            WHERE business_id = $1 AND user_id = $2
            ORDER BY created_at ASC, id ASC
        "#;

        sqlx::query_as::<_, GuideCustomerUpdate>(query)
            .bind(business_id)
            .bind(user_id)
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }
}
