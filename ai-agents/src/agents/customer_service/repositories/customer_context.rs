use async_trait::async_trait;
use chrono::NaiveDateTime;
use sqlx::PgPool;

use super::guide_rows::{GuideRow, guide_columns};
use crate::agents::customer_service::domain::{AgentPersona, BusinessProfile, CustomerContext};
use crate::agents::customer_service::ports::{CustomerContextReader, RepositoryError};
use crate::domain::ChatKey;

/// Nombre del agente si el business no configuró uno.
const DEFAULT_AGENT_NAME: &str = "Verónica";

const HEADER: &str = r#"
    SELECT
        b.name AS business_name,
        co.name AS country_name,
        co.timezone,
        (now() AT TIME ZONE co.timezone) AS local_now,
        u.full_name AS customer_name,
        a.agent_name,
        a.policies
    FROM chats c
    JOIN businesses b ON b.id = c.business_id
    JOIN countries co ON co.id = b.country_id
    JOIN users u ON u.phone_number = c.user_id
    LEFT JOIN business_ai_agents a ON a.business_id = c.business_id
    WHERE c.business_id = $1 AND c.user_id = $2
"#;

const GUIDES: &str = concat!(
    "SELECT ",
    guide_columns!(),
    r#"
    FROM guides g
    JOIN businesses b ON b.id = g.business_id
    JOIN countries co ON co.id = b.country_id
    WHERE g.business_id = $1 AND g.user_id = $2
    ORDER BY coalesce(g.last_notification_timestamp, g.tracking_notified_at) DESC NULLS LAST, g.number
    LIMIT $3
    "#
);

#[derive(sqlx::FromRow)]
struct HeaderRow {
    business_name: String,
    country_name: String,
    timezone: String,
    local_now: NaiveDateTime,
    customer_name: String,
    agent_name: Option<String>,
    policies: Option<String>,
}

pub struct PgCustomerContextReader {
    pool: PgPool,
}

impl PgCustomerContextReader {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CustomerContextReader for PgCustomerContextReader {
    async fn load(&self, chat: &ChatKey, guide_limit: i64) -> Result<Option<CustomerContext>, RepositoryError> {
        let header = sqlx::query_as::<_, HeaderRow>(HEADER)
            .bind(chat.business_id)
            .bind(&chat.user_phone)
            .fetch_optional(&self.pool);
        let guides = sqlx::query_as::<_, GuideRow>(GUIDES)
            .bind(chat.business_id)
            .bind(&chat.user_phone)
            .bind(guide_limit)
            .fetch_all(&self.pool);
        let (header, guides) = tokio::try_join!(header, guides)?;

        Ok(header.map(|header| CustomerContext {
            chat: chat.clone(),
            business: BusinessProfile {
                name: header.business_name,
                country: header.country_name,
                timezone: header.timezone,
                local_now: header.local_now,
            },
            persona: AgentPersona {
                name: header.agent_name.unwrap_or_else(|| DEFAULT_AGENT_NAME.to_string()),
                policies: header.policies.unwrap_or_default(),
            },
            customer_name: header.customer_name,
            guides: guides.into_iter().map(|row| row.into_summary().0).collect(),
        }))
    }
}
