use async_trait::async_trait;

use crate::businesses::dtos::{
    Business, BusinessAssociate, BusinessUsersSheet, Country, CreateAssociateDto, CreateBusinessDto,
};

/// Errores de negocio al registrar un asociado (el controlador los
/// traduce a 409).
pub const USERNAME_TAKEN: &str = "Username already exists";
pub const PHONE_TAKEN: &str = "Phone number already registered to an associate";

#[async_trait]
pub trait BusinessRepositoryTrait: Send + Sync {
    async fn save_business(&self, dto: &CreateBusinessDto) -> Result<Business, String>;
    async fn get_businesses(&self) -> Result<Vec<Business>, String>;
    async fn get_business_by_id(&self, id: i32) -> Result<Option<Business>, String>;
    async fn get_country_by_id(&self, id: i32) -> Result<Option<Country>, String>;
    async fn get_sheet_by_business(&self, business_id: i32) -> Result<Option<BusinessUsersSheet>, String>;
    async fn save_associate(&self, business_id: i32, dto: &CreateAssociateDto, password_hash: &str) -> Result<BusinessAssociate, String>;
    async fn get_associates_by_business(&self, business_id: i32) -> Result<Vec<BusinessAssociate>, String>;
    /// Todos los asociados de empresas activas (lo consulta el bot al iniciar).
    async fn get_all_associates(&self) -> Result<Vec<BusinessAssociate>, String>;
    async fn get_all_associate_phones(&self) -> Result<Vec<String>, String>;
    async fn username_exists(&self, username: &str) -> Result<bool, String>;
    async fn associate_phone_exists(&self, phone_number: &str) -> Result<bool, String>;
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
            INSERT INTO businesses (name, country_id) VALUES ($1, $2)
            RETURNING id, name, state, country_id, created_at, updated_at
        "#;

        let row: Business = sqlx::query_as(query)
            .bind(&dto.name)
            .bind(dto.country_id)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(row)
    }

    async fn get_businesses(&self) -> Result<Vec<Business>, String> {
        let query = r#"
            SELECT id, name, state, country_id, created_at, updated_at
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
            SELECT id, name, state, country_id, created_at, updated_at
            FROM businesses WHERE id = $1 AND state != 'Deleted'
        "#;

        sqlx::query_as::<_, Business>(query)
            .bind(id)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_country_by_id(&self, id: i32) -> Result<Option<Country>, String> {
        let query = r#"
            SELECT id, code, name, phone_prefix, phone_length
            FROM countries WHERE id = $1
        "#;

        sqlx::query_as::<_, Country>(query)
            .bind(id)
            .fetch_optional(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_sheet_by_business(&self, business_id: i32) -> Result<Option<BusinessUsersSheet>, String> {
        let query = r#"
            SELECT id, business_id, document_id, office_id, delivered_id
            FROM business_users_sheet WHERE business_id = $1
        "#;

        sqlx::query_as::<_, BusinessUsersSheet>(query)
            .bind(business_id)
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
            .map_err(|e| {
                // Dos registros simultáneos pasan la validación previa del
                // servicio; los índices únicos deciden.
                let constraint = e.as_database_error().and_then(|db| db.constraint());
                match constraint {
                    Some("business_associates_phone_number_key") => PHONE_TAKEN.to_string(),
                    Some("business_associates_username_key") => USERNAME_TAKEN.to_string(),
                    _ => e.to_string(),
                }
            })?;

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

    async fn get_all_associates(&self) -> Result<Vec<BusinessAssociate>, String> {
        let query = r#"
            SELECT ba.id, ba.business_id, ba.phone_number, ba.username
            FROM business_associates ba
            JOIN businesses b ON b.id = ba.business_id
            WHERE b.state = 'Active'
            ORDER BY ba.id
        "#;

        sqlx::query_as::<_, BusinessAssociate>(query)
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

    async fn associate_phone_exists(&self, phone_number: &str) -> Result<bool, String> {
        let query = r#"
            SELECT EXISTS(SELECT 1 FROM business_associates WHERE phone_number = $1)
        "#;

        let result: (bool,) = sqlx::query_as(query)
            .bind(phone_number)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(result.0)
    }
}
