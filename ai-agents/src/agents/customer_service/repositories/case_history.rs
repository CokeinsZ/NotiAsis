//! Escalamientos y novedades anteriores del chat (parte del contexto del
//! cliente). Horas en la zona del business.

use chrono::{NaiveDate, NaiveDateTime};
use serde::de::DeserializeOwned;
use sqlx::PgPool;

use crate::agents::customer_service::domain::{
    CaseHistory, Escalation, EscalationCategory, GuideUpdate, PastEscalation, PastGuideUpdate,
};
use crate::domain::ChatKey;

/// Los `$3` más recientes; `answered` = un asesor escribió después.
const ESCALATIONS: &str = r#"
    SELECT
        ((e.created_at AT TIME ZONE 'UTC') AT TIME ZONE co.timezone) AS local_time,
        e.category,
        e.reason,
        e.guide_number,
        EXISTS (
            SELECT 1 FROM messages m
            WHERE m.business_id = e.business_id AND m.user_id = e.user_id
              AND m.sender = 'associate' AND m.created_at >= e.created_at
        ) AS answered
    FROM ai_escalations e
    JOIN businesses b ON b.id = e.business_id
    JOIN countries co ON co.id = b.country_id
    WHERE e.business_id = $1 AND e.user_id = $2
    ORDER BY e.created_at DESC
    LIMIT $3
"#;

const UPDATES: &str = r#"
    SELECT
        ((u.created_at AT TIME ZONE 'UTC') AT TIME ZONE co.timezone) AS local_time,
        u.guide_number,
        u.kind,
        u.promised_date,
        u.note
    FROM guide_customer_updates u
    JOIN businesses b ON b.id = u.business_id
    JOIN countries co ON co.id = b.country_id
    WHERE u.business_id = $1 AND u.user_id = $2
    ORDER BY u.created_at DESC
    LIMIT $3
"#;

#[derive(sqlx::FromRow)]
struct EscalationRow {
    local_time: NaiveDateTime,
    category: String,
    reason: String,
    guide_number: Option<String>,
    answered: bool,
}

#[derive(sqlx::FromRow)]
struct UpdateRow {
    local_time: NaiveDateTime,
    guide_number: String,
    kind: String,
    promised_date: Option<NaiveDate>,
    note: Option<String>,
}

/// Hasta `limit` escalamientos y `limit` novedades, del más antiguo al más
/// reciente.
pub async fn load_case_history(pool: &PgPool, chat: &ChatKey, limit: i64) -> Result<CaseHistory, sqlx::Error> {
    let escalations = sqlx::query_as::<_, EscalationRow>(ESCALATIONS)
        .bind(chat.business_id)
        .bind(&chat.user_phone)
        .bind(limit)
        .fetch_all(pool);
    let updates = sqlx::query_as::<_, UpdateRow>(UPDATES)
        .bind(chat.business_id)
        .bind(&chat.user_phone)
        .bind(limit)
        .fetch_all(pool);
    let (escalations, updates) = tokio::try_join!(escalations, updates)?;

    Ok(CaseHistory {
        escalations: escalations
            .into_iter()
            .rev()
            .map(|row| PastEscalation {
                at: row.local_time,
                escalation: Escalation {
                    category: from_db(&row.category).unwrap_or(EscalationCategory::Other),
                    reason: row.reason,
                    guide_number: row.guide_number,
                },
                answered: row.answered,
            })
            .collect(),
        updates: updates
            .into_iter()
            .rev()
            .filter_map(|row| {
                Some(PastGuideUpdate {
                    at: row.local_time,
                    update: GuideUpdate {
                        guide_number: row.guide_number,
                        kind: from_db(&row.kind)?,
                        promised_date: row.promised_date,
                        note: row.note,
                    },
                })
            })
            .collect(),
    })
}

/// Los enums se guardan con su nombre en snake_case, el mismo de serde.
fn from_db<T: DeserializeOwned>(value: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::from(value)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::customer_service::domain::GuideUpdateKind;

    #[test]
    fn database_values_parse_into_the_enums() {
        assert_eq!(from_db::<GuideUpdateKind>("will_pickup"), Some(GuideUpdateKind::WillPickup));
        assert_eq!(from_db::<EscalationCategory>("shipping_issue"), Some(EscalationCategory::ShippingIssue));
        assert_eq!(from_db::<EscalationCategory>("algo_nuevo"), None);
    }
}
