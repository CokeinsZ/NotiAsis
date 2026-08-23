/// Normaliza un número de teléfono al formato sin '+' (ej. 573001234567).
/// Todos los números se guardan y comparan en este formato para evitar
/// duplicados por diferencias de prefijo entre Meta, el LLM y los clientes.
pub fn normalize_phone(phone: &str) -> String {
    phone.trim().trim_start_matches('+').to_string()
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
}
