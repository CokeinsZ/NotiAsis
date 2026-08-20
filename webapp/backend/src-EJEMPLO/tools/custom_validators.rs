use validator::{ValidationError};

pub fn validate_non_blank(value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        let mut error = ValidationError::new("blank");
        error.message = Some("No puede estar vacío o solo con espacios".into());
        return Err(error);
    }

    Ok(())
}

pub fn validate_numeric(value: &str) -> Result<(), ValidationError> {
    if value.chars().all(char::is_numeric) {
        Ok(())
    } else {
        let mut error = ValidationError::new("invalid_numeric_value");
        error.message = Some("El valor debe ser numérico".into());
        Err(error)
    }
}

pub fn validate_password(value: &str) -> Result<(), ValidationError> {
    let has_uppercase = value.chars().any(char::is_uppercase);
    let has_lowercase = value.chars().any(char::is_lowercase);
    let has_number = value.chars().any(char::is_numeric);

    if (5..=100).contains(&value.len()) && has_uppercase && has_lowercase && has_number {
        Ok(())
    } else {
        let mut error = ValidationError::new("weak_password");
        error.message = Some("La contraseña debe tener entre 5 y 100 caracteres, incluir mayúscula, minúscula y número".into());
        Err(error)
    }
}

