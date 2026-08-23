use async_trait::async_trait;

use crate::users::dtos::User;

#[async_trait]
pub trait UserRepositoryTrait: Send + Sync {
    /// Crea el usuario si no existe; si ya existe y llega un nombre
    /// no vacío distinto, lo actualiza.
    async fn upsert_user(&self, phone_number: &str, full_name: &str) -> Result<User, String>;
    async fn get_user_by_phone(&self, phone_number: &str) -> Result<Option<User>, String>;
    async fn get_users(&self) -> Result<Vec<User>, String>;
}

pub struct PostgresUserRepository {
    db_pool: sqlx::PgPool,
}

impl PostgresUserRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { db_pool: pool }
    }
}

#[async_trait]
impl UserRepositoryTrait for PostgresUserRepository {
    async fn upsert_user(&self, phone_number: &str, full_name: &str) -> Result<User, String> {
        let query = r#"
            INSERT INTO users (phone_number, full_name)
            VALUES ($1, $2)
            ON CONFLICT (phone_number) DO UPDATE
                SET full_name = CASE
                    WHEN EXCLUDED.full_name <> '' THEN EXCLUDED.full_name
                    ELSE users.full_name
                END
            RETURNING phone_number, full_name
        "#;

        let row: User = sqlx::query_as(query)
            .bind(phone_number)
            .bind(full_name)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(row)
    }

    async fn get_user_by_phone(&self, phone_number: &str) -> Result<Option<User>, String> {
        let query = r#"
            SELECT phone_number, full_name FROM users WHERE phone_number = $1
        "#;

        sqlx::query_as::<_, User>(query)
            .bind(phone_number)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_users(&self) -> Result<Vec<User>, String> {
        let query = r#"
            SELECT phone_number, full_name FROM users ORDER BY full_name
        "#;

        sqlx::query_as::<_, User>(query)
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }
}
