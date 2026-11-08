/// Normaliza un número de teléfono al formato sin '+' (ej. 573001234567).
/// Todos los números se guardan y comparan en este formato para evitar
/// duplicados por diferencias de prefijo entre Meta, el LLM y los clientes.
pub fn normalize_phone(phone: &str) -> String {
    phone.trim().trim_start_matches('+').to_string()
}

/// Número con indicativo de un país: el número nacional ("3001234567"
/// en Colombia) recibe el indicativo y uno que ya lo trae queda igual.
/// None si no es un número de ese país.
pub fn to_international(phone: &str, prefix: &str, national_length: usize) -> Option<String> {
    let digits = normalize_phone(phone);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if digits.len() == national_length {
        return Some(format!("{prefix}{digits}"));
    }
    if digits.len() == prefix.len() + national_length && digits.starts_with(prefix) {
        return Some(digits);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_plus_and_spaces() {
        assert_eq!(normalize_phone("+573003579384"), "573003579384");
        assert_eq!(normalize_phone("573003579384"), "573003579384");
        assert_eq!(normalize_phone(" 573003579384 "), "573003579384");
    }

    #[test]
    fn adds_country_prefix_to_national_numbers() {
        assert_eq!(to_international("3003579384", "57", 10).as_deref(), Some("573003579384"));
        assert_eq!(to_international("+573003579384", "57", 10).as_deref(), Some("573003579384"));
        assert_eq!(to_international("51319585", "502", 8).as_deref(), Some("50251319585"));
        assert_eq!(to_international("50251319585", "502", 8).as_deref(), Some("50251319585"));
    }

    #[test]
    fn rejects_numbers_of_another_country() {
        assert_eq!(to_international("51319585", "57", 10), None);
        assert_eq!(to_international("573003579384", "502", 8), None);
        assert_eq!(to_international("300 357 9384", "57", 10), None);
    }
}
