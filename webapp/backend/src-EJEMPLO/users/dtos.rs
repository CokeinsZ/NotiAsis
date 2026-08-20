use serde::{Deserialize, Serialize};
use validator::{Validate};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::tools::custom_validators::{validate_non_blank, validate_password};

static GOVERNMENT_ID_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:\d{10}|\d{10}-\d{2})$").unwrap()
});

static PHONE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:\d{7}|\d{10}|\+\d{12})$").unwrap()
});

#[derive(Debug, Deserialize, Serialize)]
#[derive(sqlx::Type)]
#[sqlx(type_name = "varchar", rename_all = "PascalCase")]
pub enum user_entity_type {
    Natural,
    Juridical
}

#[derive(Debug, Deserialize, Serialize)]
#[derive(sqlx::Type)]
#[sqlx(type_name = "varchar", rename_all = "PascalCase")]
pub enum user_state {
    Active,
    Inactive,
    Blocked,
    Deleted
}

#[derive(Debug, Deserialize, Serialize)]
#[derive(sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub full_name: String,
    pub entity_type: user_entity_type,
    pub government_id: String,
    pub email: String,
    pub phone: String,
    pub address: String,
    pub created_at: String,
    pub updated_at: String,
    pub state: user_state
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateUserDto {
    #[validate(custom(function = "validate_non_blank"), length(min = 1, max = 255, message = "El nombre debe tener entre 1 y 255 caracteres"))]
    pub full_name: String,

    pub entity_type: user_entity_type, 

    #[validate(regex(path = *GOVERNMENT_ID_REGEX, message = "El documento debe ser 10 dígitos o 10 dígitos seguido de guion y 2 dígitos"))]
    pub government_id: String,
    
    #[validate(email(message = "Email inválido"))]
    pub email: String,
    
    #[validate(regex(path = *PHONE_REGEX, message = "El teléfono debe ser 7, 10 o '+57' y 10 dígitos"))]
    pub phone: String,

    #[validate(custom(function = "validate_password"))]
    pub password: String,

    #[validate(custom(function = "validate_non_blank"), length(min = 1, message = "La dirección debe tener mas de 1 caracter"))]
    pub address: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateUserDto {
    #[validate(length(min = 1, max = 255, message = "El nombre debe tener entre 1 y 255 caracteres"))]
    pub full_name: String,

    pub entity_type: user_entity_type, 

    #[validate(regex(path = *GOVERNMENT_ID_REGEX, message = "El documento debe ser 10 dígitos o 10 dígitos seguido de guion y 2 dígitos"))]
    pub government_id: String,
    
    #[validate(email(message = "Email inválido"))]
    pub email: String,
    
    #[validate(regex(path = *PHONE_REGEX, message = "El teléfono debe ser 7, 10 o '+57' y 10 dígitos"))]
    pub phone: String,

    #[validate(length(min = 1, message = "La dirección debe tener mas de 1 caracter"))]
    pub address: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct ChangeUserPasswordDto {
    #[validate(length(min = 5, max = 100, message = "La contraseña debe tener entre 5 y 100 caracteres"))]
    pub password: String,

    #[validate(length(equal = 6, message = "El código debe tener 6 caracteres"))]
    pub code: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
#[derive(sqlx::FromRow)]
pub struct UserFilters {
    #[validate(length(min = 1, max = 255, message = "El nombre debe tener entre 1 y 255 caracteres"))]
    pub name: Option<String>,

    #[validate(length(min = 10, max = 13, message = "El documento de identificación debe tener entre 10 y 13 caracteres"))]
    pub government_id: Option<String>,
    
    #[validate(regex(path = *PHONE_REGEX, message = "El teléfono debe ser 7, 10 o '+57' y 10 dígitos"))]
    pub phone: Option<String>
}