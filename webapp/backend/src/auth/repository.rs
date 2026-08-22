use async_trait::async_trait;

/// Fila de business_associates con su hash (solo para autenticación).
#[derive(Clone)]
pub struct AssociateCredentials {
    pub username: String,
    pub phone_number: String,
    pub business_id: i32,
    pub password_hash: String,
}

#[async_trait]
pub trait AuthRepositoryTrait: Send + Sync {
    async fn find_associate_by_username(&self, username: &str) -> Result<Option<AssociateCredentials>, String>;
    async fn find_associate_by_id(&self, id: i32) -> Result<Option<AssociateCredentials>, String>;
    async fn update_associate_password(&self, id: i32, password_hash: &str) -> Result<bool, String>;
    async fn find_api_key_id(&self, key: &str) -> Result<Option<i32>, String>;
}

pub struct PostgresAuthRepository {
    db_pool: sqlx::PgPool,
}

impl PostgresAuthRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { db_pool: pool }
    }
}

#[async_trait]
impl AuthRepositoryTrait for PostgresAuthRepository {
    async fn find_associate_by_username(&self, username: &str) -> Result<Option<AssociateCredentials>, String> {
        let query = r#"
            SELECT ba.username, ba.phone_number, ba.business_id, ba.password_hash
            FROM business_associates ba
            JOIN businesses b ON b.id = ba.business_id
            WHERE ba.username = $1 AND b.state = 'Active'
        "#;

        let row: Option<(String, String, i32, String)> = sqlx::query_as(query)
            .bind(username)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(row.map(|(username, phone_number, business_id, password_hash)| {
            AssociateCredentials { username, phone_number, business_id, password_hash }
        }))
    }

    async fn find_associate_by_id(&self, id: i32) -> Result<Option<AssociateCredentials>, String> {
        let query = r#"
            SELECT username, phone_number, business_id, password_hash
            FROM business_associates WHERE id = $1
        "#;

        let row: Option<(String, String, i32, String)> = sqlx::query_as(query)
            .bind(id)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(row.map(|(username, phone_number, business_id, password_hash)| {
            AssociateCredentials { username, phone_number, business_id, password_hash }
        }))
    }

    async fn update_associate_password(&self, id: i32, password_hash: &str) -> Result<bool, String> {
        let query = r#"
            UPDATE business_associates SET password_hash = $2 WHERE id = $1
        "#;

        let result = sqlx::query(query)
            .bind(id)
            .bind(password_hash)
            .execute(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(result.rows_affected() > 0)
    }

    async fn find_api_key_id(&self, key: &str) -> Result<Option<i32>, String> {
        let query = r#"
            SELECT id FROM api_keys WHERE key = $1
        "#;

        let row: Option<(i32,)> = sqlx::query_as(query)
            .bind(key)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(row.map(|r| r.0))
    }
}
