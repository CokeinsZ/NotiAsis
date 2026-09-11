use async_trait::async_trait;
use chrono::NaiveDateTime;

use crate::guides::dtos::{DailyNotificationStat, Guide};

#[async_trait]
pub trait GuideRepositoryTrait: Send + Sync {
    /// Inserta la guía solo si el número no existe. Retorna None cuando
    /// la guía ya estaba registrada.
    async fn insert_guide_if_new(&self, number: &str, user_id: &str, business_id: i32) -> Result<Option<Guide>, String>;
    async fn get_guide_by_number(&self, number: &str) -> Result<Option<Guide>, String>;
    async fn get_guides(&self, user_phone: Option<&str>) -> Result<Vec<Guide>, String>;
    /// Marca cuándo se notificó la guía por última vez.
    async fn mark_notified(&self, number: &str, timestamp: NaiveDateTime) -> Result<bool, String>;
    /// Notificaciones por día y tipo para el dashboard del business.
    async fn get_notification_stats(
        &self,
        business_id: i32,
        days: Option<i32>,
        start_date: Option<chrono::NaiveDate>,
        end_date: Option<chrono::NaiveDate>,
    ) -> Result<Vec<DailyNotificationStat>, String>;
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
    async fn insert_guide_if_new(&self, number: &str, user_id: &str, business_id: i32) -> Result<Option<Guide>, String> {
        let query = r#"
            INSERT INTO guides (number, user_id, business_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (number) DO NOTHING
            RETURNING number, user_id, business_id, last_notification_timestamp, notification_count
        "#;

        sqlx::query_as::<_, Guide>(query)
            .bind(number)
            .bind(user_id)
            .bind(business_id)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_guide_by_number(&self, number: &str) -> Result<Option<Guide>, String> {
        let query = r#"
            SELECT number, user_id, business_id, last_notification_timestamp, notification_count
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
            "SELECT number, user_id, business_id, last_notification_timestamp, notification_count FROM guides WHERE 1=1"
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
            WITH updated AS (
                UPDATE guides
                SET last_notification_timestamp = $2::timestamp,
                    notification_count = notification_count + 1
                WHERE number = $1
                RETURNING number, business_id, user_id, notification_count, last_notification_timestamp
            )
            INSERT INTO guide_notifications (guide_number, business_id, user_id, notification_step, created_at)
            SELECT number, business_id, user_id, notification_count, last_notification_timestamp
            FROM updated
        "#;

        let result = sqlx::query(query)
            .bind(number)
            .bind(timestamp)
            .execute(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_notification_stats(
        &self,
        business_id: i32,
        days: Option<i32>,
        start_date: Option<chrono::NaiveDate>,
        end_date: Option<chrono::NaiveDate>,
    ) -> Result<Vec<DailyNotificationStat>, String> {
        let mut query = sqlx::QueryBuilder::new(
            r#"SELECT
                created_at::date AS day,
                notification_step AS notification_count,
                COUNT(*) AS total
            FROM guide_notifications
            WHERE business_id = "#,
        );
        query.push_bind(business_id);

        if let Some(start) = start_date {
            query.push(" AND created_at::date >= ");
            query.push_bind(start);
        } else if let Some(d) = days {
            query.push(" AND created_at::date >= CURRENT_DATE - ");
            query.push_bind(d);
        } else {
            query.push(" AND created_at::date >= CURRENT_DATE - 30");
        }

        if let Some(end) = end_date {
            query.push(" AND created_at::date <= ");
            query.push_bind(end);
        }

        query.push(" GROUP BY day, notification_step ORDER BY day, notification_step");

        query.build_query_as::<DailyNotificationStat>()
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }
}
