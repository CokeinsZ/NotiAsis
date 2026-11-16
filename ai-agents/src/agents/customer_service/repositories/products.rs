use async_trait::async_trait;
use sqlx::PgPool;

use super::product_rows::{ProductRow, product_columns};
use crate::agents::customer_service::domain::ProductSheet;
use crate::agents::customer_service::ports::{ProductCatalog, RepositoryError};

const CATALOG: &str = concat!(
    "SELECT ",
    product_columns!(),
    r#"
    FROM products p
    WHERE p.business_id = $1 AND p.active
    ORDER BY p.name
    "#
);

pub struct PgProductCatalog {
    pool: PgPool,
}

impl PgProductCatalog {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProductCatalog for PgProductCatalog {
    async fn products(&self, business_id: i32) -> Result<Vec<ProductSheet>, RepositoryError> {
        let rows = sqlx::query_as::<_, ProductRow>(CATALOG).bind(business_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(ProductSheet::from).collect())
    }
}
