use std::collections::HashMap;

use async_trait::async_trait;
use chrono::NaiveDateTime;

use crate::guides::dtos::{
    DailyNotificationStat, Guide, GuideDetails, GuideDetailsDto, GuideProduct, GuideWithDetails,
};

#[async_trait]
pub trait GuideRepositoryTrait: Send + Sync {
    /// Inserta la guía solo si el número no existe. Retorna None cuando
    /// la guía ya estaba registrada.
    async fn insert_guide_if_new(&self, number: &str, user_id: &str, business_id: i32) -> Result<Option<Guide>, String>;
    async fn get_guide_by_number(&self, number: &str) -> Result<Option<Guide>, String>;
    async fn get_guides(&self, user_phone: Option<&str>) -> Result<Vec<Guide>, String>;
    /// Guarda los datos del pedido de una guía existente. Los campos en
    /// None conservan su valor; si llegan productos, reemplazan a los
    /// anteriores. Retorna false si la guía no existe.
    async fn save_details(&self, number: &str, details: &GuideDetailsDto) -> Result<bool, String>;
    /// Las guías existentes entre `numbers`, con sus datos de pedido.
    async fn get_guides_with_details(&self, numbers: &[String]) -> Result<Vec<GuideWithDetails>, String>;
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

    async fn save_details(&self, number: &str, details: &GuideDetailsDto) -> Result<bool, String> {
        let mut tx = self.db_pool.begin().await.map_err(|e| e.to_string())?;

        let query = r#"
            UPDATE guides SET
                order_id = COALESCE($2, order_id),
                customer_name = COALESCE($3, customer_name),
                customer_document = COALESCE($4, customer_document),
                delivery_address = COALESCE($5, delivery_address),
                city = COALESCE($6, city),
                shipping_company = COALESCE($7, shipping_company),
                order_total = COALESCE($8::numeric, order_total),
                currency = COALESCE($9, currency)
            WHERE number = $1
        "#;

        let updated = sqlx::query(query)
            .bind(number)
            .bind(&details.order_id)
            .bind(&details.customer_name)
            .bind(&details.customer_document)
            .bind(&details.delivery_address)
            .bind(&details.city)
            .bind(&details.shipping_company)
            .bind(details.total)
            .bind(&details.currency)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        if updated.rows_affected() == 0 {
            // La guía no existe: el drop de `tx` hace rollback.
            return Ok(false);
        }

        if !details.products.is_empty() {
            sqlx::query("DELETE FROM guide_products WHERE guide_number = $1")
                .bind(number)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;

            let insert = r#"
                INSERT INTO guide_products (guide_number, product_id, product_name, quantity, unit_price, subtotal)
                VALUES ($1, $2, $3, $4, $5::numeric, $6::numeric)
            "#;

            for product in &details.products {
                sqlx::query(insert)
                    .bind(number)
                    .bind(product.product_id)
                    .bind(&product.product_name)
                    .bind(product.quantity)
                    .bind(product.unit_price)
                    .bind(product.subtotal)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }

        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(true)
    }

    async fn get_guides_with_details(&self, numbers: &[String]) -> Result<Vec<GuideWithDetails>, String> {
        #[derive(sqlx::FromRow)]
        struct GuideRow {
            #[sqlx(flatten)]
            guide: Guide,
            #[sqlx(flatten)]
            details: GuideDetails,
        }

        let guides_query = r#"
            SELECT number, user_id, business_id, last_notification_timestamp, notification_count,
                   order_id, customer_name, customer_document, delivery_address, city,
                   shipping_company, order_total::float8 AS total, currency
            FROM guides WHERE number = ANY($1)
        "#;

        let rows: Vec<GuideRow> = sqlx::query_as(guides_query)
            .bind(numbers)
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        let products_query = r#"
            SELECT guide_number, product_id, product_name, quantity,
                   unit_price::float8 AS unit_price, subtotal::float8 AS subtotal
            FROM guide_products WHERE guide_number = ANY($1)
            ORDER BY guide_number, id
        "#;

        let products: Vec<GuideProduct> = sqlx::query_as(products_query)
            .bind(numbers)
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        let mut products_by_guide: HashMap<String, Vec<GuideProduct>> = HashMap::new();
        for product in products {
            products_by_guide.entry(product.guide_number.clone()).or_default().push(product);
        }

        Ok(rows
            .into_iter()
            .map(|row| GuideWithDetails {
                products: products_by_guide.remove(&row.guide.number).unwrap_or_default(),
                guide: row.guide,
                details: row.details,
            })
            .collect())
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
