use crate::shared::text::search_words;

/// Ficha de un producto del catálogo del business (`products`): lo que el
/// agente le puede contar al cliente. Un campo en `None` es un dato que no
/// tenemos y que el agente no debe inventar.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductSheet {
    pub name: String,
    /// Para qué sirve.
    pub description: Option<String>,
    pub ingredients: Option<String>,
    pub presentation: Option<String>,
    pub dosage: Option<String>,
    pub usage_instructions: Option<String>,
    /// Precauciones y contraindicaciones.
    pub warnings: Option<String>,
    pub treatment_duration: Option<String>,
    pub shipping: Option<String>,
    pub warranty: Option<String>,
    /// Indicaciones del negocio para el agente; no se le leen al cliente.
    pub agent_notes: Option<String>,
}

/// Desde este largo, una palabra también vale dentro de otra
/// ("perfume" en "perfumes").
const MIN_PARTIAL_WORD: usize = 4;
/// Con este prefijo en común dos palabras se consideran la misma
/// ("cayena" y "cayenne", "testosterona" y "testosterone").
const MIN_SHARED_PREFIX: usize = 5;

/// Los productos cuyo nombre mejor coincide con `query`, de más a menos
/// palabras en común (a igualdad, en el orden del catálogo). Vacío si
/// ninguno comparte una palabra.
pub fn best_matches<'a>(catalog: &'a [ProductSheet], query: &str, limit: usize) -> Vec<&'a ProductSheet> {
    let query_words = search_words(query);
    let mut scored: Vec<(usize, &ProductSheet)> = catalog
        .iter()
        .map(|product| {
            let name_words = search_words(&product.name);
            let score = query_words.iter().filter(|word| name_words.iter().any(|name| same_word(word, name))).count();
            (score, product)
        })
        .filter(|(score, _)| *score > 0)
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().take(limit).map(|(_, product)| product).collect()
}

fn same_word(a: &str, b: &str) -> bool {
    let partial = |short: &str, long: &str| short.chars().count() >= MIN_PARTIAL_WORD && long.contains(short);
    let shared_prefix = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
    a == b || partial(a, b) || partial(b, a) || shared_prefix >= MIN_SHARED_PREFIX
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Vec<ProductSheet> {
        [
            "Combo perfumes árabes: Asad de Lattafa + Club de Nuit",
            "Pimienta de Cayena (Cayenne Pepper)",
            "Testosterone+ Advanced 15 en 1",
        ]
        .into_iter()
        .map(|name| ProductSheet { name: name.into(), ..Default::default() })
        .collect()
    }

    fn names(matches: Vec<&ProductSheet>) -> Vec<&str> {
        matches.into_iter().map(|product| product.name.as_str()).collect()
    }

    #[test]
    fn matches_dropi_names_and_how_customers_say_it() {
        let catalog = catalog();
        assert_eq!(
            names(best_matches(&catalog, "EXTRACTO PIMIENTA DE CAYENA 3000 Mg", 3)),
            ["Pimienta de Cayena (Cayenne Pepper)"]
        );
        assert_eq!(names(best_matches(&catalog, "CAYENNE PEPPER PRIV", 3)), ["Pimienta de Cayena (Cayenne Pepper)"]);
        assert_eq!(names(best_matches(&catalog, "la testosterona", 3)), ["Testosterone+ Advanced 15 en 1"]);
        assert_eq!(
            names(best_matches(&catalog, "PERFUMES ASAD+CLUBTHE NUITFULL", 3)),
            ["Combo perfumes árabes: Asad de Lattafa + Club de Nuit"]
        );
    }

    #[test]
    fn best_match_first_and_nothing_for_unrelated_words() {
        let mut catalog = catalog();
        catalog.push(ProductSheet { name: "Cayena en polvo".into(), ..Default::default() });
        assert_eq!(
            names(best_matches(&catalog, "cayenne pepper", 3)),
            ["Pimienta de Cayena (Cayenne Pepper)", "Cayena en polvo"]
        );
        assert!(best_matches(&catalog, "collar girasol", 3).is_empty());
        assert!(best_matches(&catalog, "", 3).is_empty());
    }
}
