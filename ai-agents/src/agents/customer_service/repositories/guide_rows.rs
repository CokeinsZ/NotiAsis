//! Columnas y fila de una guía, compartidas por el contexto del cliente y
//! el detalle de una guía.

use chrono::NaiveDate;

use crate::agents::customer_service::domain::{GuideStage, GuideSummary, ProductLine};

/// Columnas de una guía. Requiere `guides g` y `countries co` (la zona
/// horaria del business) en el FROM.
macro_rules! guide_columns {
    () => {
        r#"
        g.number,
        g.order_id,
        g.customer_name,
        g.delivery_address,
        g.city,
        g.shipping_company,
        g.order_total::float8 AS order_total,
        g.currency,
        g.notification_count,
        g.tracking_level,
        (SELECT array_agg(p.product_name::text ORDER BY p.id)
           FROM guide_products p WHERE p.guide_number = g.number) AS product_names,
        (SELECT array_agg(p.quantity ORDER BY p.id)
           FROM guide_products p WHERE p.guide_number = g.number) AS product_quantities,
        (((SELECT min(n.created_at) FROM guide_notifications n
            WHERE n.guide_number = g.number AND n.kind = 'office')
          AT TIME ZONE 'UTC') AT TIME ZONE co.timezone)::date AS at_office_since,
        ((coalesce(g.last_notification_timestamp, g.tracking_notified_at)
          AT TIME ZONE 'UTC') AT TIME ZONE co.timezone)::date AS last_notified_on
        "#
    };
}
pub(crate) use guide_columns;

#[derive(Debug, sqlx::FromRow)]
pub struct GuideRow {
    pub number: String,
    pub order_id: Option<String>,
    pub customer_name: Option<String>,
    pub delivery_address: Option<String>,
    pub city: Option<String>,
    pub shipping_company: Option<String>,
    pub order_total: Option<f64>,
    pub currency: Option<String>,
    pub notification_count: i32,
    pub tracking_level: i16,
    pub product_names: Option<Vec<String>>,
    pub product_quantities: Option<Vec<i32>>,
    pub at_office_since: Option<NaiveDate>,
    pub last_notified_on: Option<NaiveDate>,
}

impl GuideRow {
    /// La guía y el id de su pedido en Dropi.
    pub fn into_summary(self) -> (GuideSummary, Option<String>) {
        let quantities = self.product_quantities.unwrap_or_default();
        let products = self
            .product_names
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(index, name)| ProductLine { name, quantity: quantities.get(index).copied().unwrap_or(1) })
            .collect();
        let summary = GuideSummary {
            number: self.number,
            customer_name: self.customer_name,
            products,
            delivery_address: self.delivery_address,
            city: self.city,
            shipping_company: self.shipping_company,
            order_total: self.order_total,
            currency: self.currency,
            stage: GuideStage::from_counters(self.notification_count, self.tracking_level),
            office_notifications: self.notification_count,
            at_office_since: self.at_office_since,
            last_notified_on: self.last_notified_on,
        };
        (summary, self.order_id)
    }
}
