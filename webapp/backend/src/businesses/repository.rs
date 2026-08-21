use async_trait::async_trait;

use crate::businesses::dtos::{Business, BusinessAssociate, CreateAssociateDto, CreateBusinessDto};

#[async_trait]
pub trait BusinessRepositoryTrait: Send + Sync {
    async fn save_business(&self, dto: &CreateBusinessDto) -> Result<Business, String>;
    async fn get_businesses(&self) -> Result<Vec<Business>, String>;
    async fn get_business_by_id(&self, id: i32) -> Result<Option<Business>, String>;
    async fn save_associate(&self, business_id: i32, dto: &CreateAssociateDto, password_hash: &str) -> Result<BusinessAssociate, String>;
    async fn get_associates_by_business(&self, business_id: i32) -> Result<Vec<BusinessAssociate>, String>;
    async fn get_all_associate_phones(&self) -> Result<Vec<String>, String>;
    async fn username_exists(&self, username: &str) -> Result<bool, String>;
}

pub struct PostgresBusinessRepository {
    db_pool: sqlx::PgPool,
}

impl PostgresBusinessRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { db_pool: pool }
    }
}

#[async_trait]
impl BusinessRepositoryTrait for PostgresBusinessRepository {
    async fn save_business(&self, dto: &CreateBusinessDto) -> Result<Business, String> {
        let query = r#"
            INSERT INTO businesses (name) VALUES ($1)
            RETURNING id, name, state, created_at, updated_at
        "#;

        let row: Business = sqlx::query_as(query)
            .bind(&dto.name)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(row)
    }

    async fn get_businesses(&self) -> Result<Vec<Business>, String> {
        let query = r#"
            SELECT id, name, state, created_at, updated_at
            FROM businesses WHERE state != 'Deleted'
            ORDER BY created_at DESC
        "#;

        sqlx::query_as::<_, Business>(query)
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_business_by_id(&self, id: i32) -> Result<Option<Business>, String> {
        let query = r#"
            SELECT id, name, state, created_at, updated_at
            FROM businesses WHERE id = $1 AND state != 'Deleted'
        "#;

        sqlx::query_as::<_, Business>(query)
            .bind(id)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn save_associate(&self, business_id: i32, dto: &CreateAssociateDto, password_hash: &str) -> Result<BusinessAssociate, String> {
        let query = r#"
            INSERT INTO business_associates (business_id, phone_number, username, password_hash)
            VALUES ($1, $2, $3, $4)
            RETURNING id, business_id, phone_number, username
        "#;

        let row: BusinessAssociate = sqlx::query_as(query)
            .bind(business_id)
            .bind(&dto.phone_number)
            .bind(&dto.username)
            .bind(password_hash)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(row)
    }

    async fn get_associates_by_business(&self, business_id: i32) -> Result<Vec<BusinessAssociate>, String> {
        let query = r#"
            SELECT id, business_id, phone_number, username
            FROM business_associates WHERE business_id = $1
            ORDER BY id
        "#;

        sqlx::query_as::<_, BusinessAssociate>(query)
            .bind(business_id)
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_all_associate_phones(&self) -> Result<Vec<String>, String> {
        let query = r#"
            SELECT ba.phone_number
            FROM business_associates ba
            JOIN businesses b ON b.id = ba.business_id
            WHERE b.state = 'Active'
        "#;

        let rows: Vec<(String,)> = sqlx::query_as(query)
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(rows.into_iter().map(|row| row.0).collect())
    }

    async fn username_exists(&self, username: &str) -> Result<bool, String> {
        let query = r#"
            SELECT EXISTS(SELECT 1 FROM business_associates WHERE username = $1)
        "#;

        let result: (bool,) = sqlx::query_as(query)
            .bind(username)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(result.0)
    }
}
