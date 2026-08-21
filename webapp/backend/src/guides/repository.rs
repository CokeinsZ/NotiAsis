use async_trait::async_trait;
use chrono::NaiveDateTime;

use crate::guides::dtos::Guide;

#[async_trait]
pub trait GuideRepositoryTrait: Send + Sync {
    /// Inserta la guía solo si el número no existe. Retorna None cuando
    /// la guía ya estaba registrada.
    async fn insert_guide_if_new(&self, number: &str, user_id: &str) -> Result<Option<Guide>, String>;
    async fn get_guide_by_number(&self, number: &str) -> Result<Option<Guide>, String>;
    async fn get_guides(&self, user_phone: Option<&str>) -> Result<Vec<Guide>, String>;
    /// Marca cuándo se notificó la guía por última vez.
    async fn mark_notified(&self, number: &str, timestamp: NaiveDateTime) -> Result<bool, String>;
}

pub struct PostgresGuideRepository {
    db_pool: sqlx::PgPool,
}

impl PostgresGuideRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { db_pool: pool }
    }
}

#[async_trait]
impl GuideRepositoryTrait for PostgresGuideRepository {
    async fn insert_guide_if_new(&self, number: &str, user_id: &str) -> Result<Option<Guide>, String> {
        let query = r#"
            INSERT INTO guides (number, user_id)
            VALUES ($1, $2)
            ON CONFLICT (number) DO NOTHING
            RETURNING number, user_id, last_notification_timestamp
        "#;

        sqlx::query_as::<_, Guide>(query)
            .bind(number)
            .bind(user_id)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_guide_by_number(&self, number: &str) -> Result<Option<Guide>, String> {
        let query = r#"
            SELECT number, user_id, last_notification_timestamp
            FROM guides WHERE number = $1
        "#;

        sqlx::query_as::<_, Guide>(query)
            .bind(number)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_guides(&self, user_phone: Option<&str>) -> Result<Vec<Guide>, String> {
        let mut query = sqlx::QueryBuilder::new(
            "SELECT number, user_id, last_notification_timestamp FROM guides WHERE 1=1"
        );

        if let Some(phone) = user_phone {
            query.push(" AND user_id = ");
            query.push_bind(phone.to_string());
        }

        query.push(" ORDER BY last_notification_timestamp DESC NULLS LAST");

        query.build_query_as::<Guide>()
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn mark_notified(&self, number: &str, timestamp: NaiveDateTime) -> Result<bool, String> {
        let query = r#"
            UPDATE guides SET last_notification_timestamp = $2 WHERE number = $1
        "#;

        let result = sqlx::query(query)
            .bind(number)
            .bind(timestamp)
            .execute(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(result.rows_affected() > 0)
    }
}
