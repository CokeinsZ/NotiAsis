use async_trait::async_trait;
use crate::users::dtos::*;

#[async_trait]
pub trait UserRepositoryTrait: Send + Sync {
    async fn save_user(&self, user_data: &CreateUserDto) -> Result<User, String>;
    async fn get_user_by_id(&self, id: &str) -> Result<Option<User>, String>;
    async fn get_users_by_filters(&self, filters: &UserFilters) -> Result<Vec<User>, String>;
    async fn update_user(&self, id: &str, user_data: &UpdateUserDto) -> Result<(), String>;
    async fn delete_user(&self, id: &str) -> Result<(), String>;
    async fn email_exists(&self, email: &str) -> Result<bool, String>;
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
    async fn save_user(&self, user_data: &CreateUserDto) -> Result<User, String> {
        let query = r#"
            INSERT INTO users (
                full_name, entity_type, government_id, email, phone, password, address
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7
            ) RETURNING *
        "#;
        
        let entity_type_str = match user_data.entity_type {
            user_entity_type::Natural => "Natural",
            user_entity_type::Juridical => "Juridical",
        };

        let row: User = sqlx::query_as(query)
            .bind(&user_data.full_name)
            .bind(entity_type_str)
            .bind(&user_data.government_id)
            .bind(&user_data.email)
            .bind(&user_data.phone)
            .bind(&user_data.password) // Hash password ideally
            .bind(&user_data.address)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(row)
    }

    async fn get_user_by_id(&self, id: &str) -> Result<Option<User>, String> {
        let query = r#"
            SELECT id, full_name, entity_type, government_id, email, phone, address, created_at, updated_at, state
            FROM users WHERE id = $1
        "#;

        let user = sqlx::query_as::<_, User>(query)
            .bind(id)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(user)
    }

    async fn get_users_by_filters(&self, filters: &UserFilters) -> Result<Vec<User>, String> {
        let mut query = sqlx::QueryBuilder::new(
            "SELECT id, full_name, entity_type, government_id, email, phone, address, created_at, updated_at, state FROM users WHERE 1=1"
        );

        if let Some(name) = &filters.name {
            query.push(" AND full_name ILIKE ");
            query.push_bind(format!("%{}%", name));
        }

        if let Some(gov_id) = &filters.government_id {
            query.push(" AND government_id = ");
            query.push_bind(gov_id.to_string());
        }

        if let Some(phone) = &filters.phone {
            query.push(" AND phone = ");
            query.push_bind(phone.to_string());
        }

        let users = query.build_query_as::<User>()
            .fetch_all(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;
        
        Ok(users)
    }

    async fn update_user(&self, id: &str, user_data: &UpdateUserDto) -> Result<(), String> {
        let query = r#"
            UPDATE users SET
                full_name = $1,
                entity_type = $2,
                government_id = $3,
                email = $4,
                phone = $5,
                address = $6,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $7
        "#;

        let entity_type_str = match user_data.entity_type {
            user_entity_type::Natural => "Natural",
            user_entity_type::Juridical => "Juridical",
        };

        sqlx::query(query)
            .bind(&user_data.full_name)
            .bind(entity_type_str)
            .bind(&user_data.government_id)
            .bind(&user_data.email)
            .bind(&user_data.phone)
            .bind(&user_data.address)
            .bind(id)
            .execute(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    async fn delete_user(&self, id: &str) -> Result<(), String> {
        let query = r#"
            UPDATE users SET state = 'Deleted', updated_at = CURRENT_TIMESTAMP WHERE id = $1
        "#;

        sqlx::query(query)
            .bind(id)
            .execute(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    async fn email_exists(&self, email: &str) -> Result<bool, String> {
        let query = r#"
            SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)
        "#;

        let result: (bool,) = sqlx::query_as(query)
            .bind(email)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(result.0)
    }
}
