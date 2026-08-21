use validator::ValidationError;

pub fn validate_non_blank(value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        let mut error = ValidationError::new("blank");
        error.message = Some("No puede estar vacío o solo con espacios".into());
        return Err(error);
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_blank_ok() {
        assert!(validate_non_blank("hola").is_ok());
    }

    #[test]
    fn non_blank_fails_with_spaces() {
        assert!(validate_non_blank("   ").is_err());
    }

    #[test]
    fn password_rules() {
        assert!(validate_password("Passw0rd").is_ok());
        assert!(validate_password("password").is_err()); // sin mayúscula ni número
        assert!(validate_password("PASS1234").is_err()); // sin minúscula
        assert!(validate_password("Pa1").is_err());      // muy corta
    }
}
