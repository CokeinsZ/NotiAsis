/// Corta `text` a máximo `max_chars` caracteres sin partir un carácter
/// UTF-8 (los mensajes traen tildes y emojis).
pub fn truncate_chars(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((byte_index, _)) => text[..byte_index].to_string(),
        None => text.to_string(),
    }
}

/// Teléfono enmascarado para los logs: conserva el indicativo y los
/// últimos 4 dígitos (`573003579384` → `5730****9384`).
pub fn mask_phone(phone: &str) -> String {
    let digits: Vec<char> = phone.chars().collect();
    if digits.len() <= 8 {
        return "*".repeat(digits.len());
    }
    let (head, rest) = digits.split_at(4);
    let (middle, tail) = rest.split_at(rest.len() - 4);
    format!("{}{}{}", head.iter().collect::<String>(), "*".repeat(middle.len()), tail.iter().collect::<String>())
}

/// Teléfono normalizado como lo guarda el backend: solo dígitos, sin '+'.
/// `None` si no tiene entre 8 y 15 dígitos.
pub fn normalize_phone(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_start_matches('+');
    let valid = (8..=15).contains(&trimmed.len()) && trimmed.chars().all(|c| c.is_ascii_digit());
    valid.then(|| trimmed.to_string())
}

/// Palabras que no distinguen un nombre de otro.
const CONNECTORS: &[&str] =
    &["a", "al", "and", "con", "de", "del", "el", "en", "la", "las", "los", "of", "para", "por", "the", "x", "y"];

/// Palabras para comparar nombres: minúsculas, sin tildes, sin signos ni
/// conectores. `"Pimienta de Cayena (Cayenne Pepper)"` →
/// `["pimienta", "cayena", "cayenne", "pepper"]`.
pub fn search_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .chars()
        .map(without_accent)
        .collect::<String>()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty() && !CONNECTORS.contains(word))
        .map(str::to_string)
        .collect()
}

fn without_accent(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_words_drop_accents_signs_and_connectors() {
        assert_eq!(search_words("Pimienta de Cayena (Cayenne Pepper)"), ["pimienta", "cayena", "cayenne", "pepper"]);
        assert_eq!(search_words("Testosterone+ ADVANCED 15 en 1"), ["testosterone", "advanced", "15", "1"]);
        assert_eq!(search_words("Combo perfumes árabes"), ["combo", "perfumes", "arabes"]);
    }

    #[test]
    fn truncate_respects_multibyte_characters() {
        assert_eq!(truncate_chars("canción 😀 final", 9), "canción 😀");
        assert_eq!(truncate_chars("corto", 50), "corto");
    }

    #[test]
    fn mask_phone_keeps_prefix_and_last_digits() {
        assert_eq!(mask_phone("573003579384"), "5730****9384");
        assert_eq!(mask_phone("1234"), "****");
    }

    #[test]
    fn normalize_phone_accepts_8_to_15_digits() {
        assert_eq!(normalize_phone("+573003579384").as_deref(), Some("573003579384"));
        assert_eq!(normalize_phone("50241571896").as_deref(), Some("50241571896"));
        assert_eq!(normalize_phone("123"), None);
        assert_eq!(normalize_phone("57300abc9384"), None);
    }
}
