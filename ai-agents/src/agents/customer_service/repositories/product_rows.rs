//! Columnas y fila de una ficha de producto, compartidas por el contexto
//! del cliente y el catálogo.

use crate::agents::customer_service::domain::ProductSheet;

/// Columnas de una ficha. Requiere `products p` en el FROM.
macro_rules! product_columns {
    () => {
        r#"
        p.name, p.description, p.ingredients, p.presentation, p.dosage, p.usage_instructions,
        p.warnings, p.treatment_duration, p.shipping, p.warranty, p.agent_notes
        "#
    };
}
pub(crate) use product_columns;

#[derive(Debug, sqlx::FromRow)]
pub struct ProductRow {
    pub name: String,
    pub description: Option<String>,
    pub ingredients: Option<String>,
    pub presentation: Option<String>,
    pub dosage: Option<String>,
    pub usage_instructions: Option<String>,
    pub warnings: Option<String>,
    pub treatment_duration: Option<String>,
    pub shipping: Option<String>,
    pub warranty: Option<String>,
    pub agent_notes: Option<String>,
}

impl From<ProductRow> for ProductSheet {
    fn from(row: ProductRow) -> Self {
        Self {
            name: row.name.trim().to_string(),
            description: filled(row.description),
            ingredients: filled(row.ingredients),
            presentation: filled(row.presentation),
            dosage: filled(row.dosage),
            usage_instructions: filled(row.usage_instructions),
            warnings: filled(row.warnings),
            treatment_duration: filled(row.treatment_duration),
            shipping: filled(row.shipping),
            warranty: filled(row.warranty),
            agent_notes: filled(row.agent_notes),
        }
    }
}

/// Un texto vacío es un dato que no tenemos.
fn filled(value: Option<String>) -> Option<String> {
    value.map(|text| text.trim().to_string()).filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_fields_are_missing_data() {
        assert_eq!(filled(Some("  ".into())), None);
        assert_eq!(filled(Some(" 2 cápsulas al día ".into())).as_deref(), Some("2 cápsulas al día"));
        assert_eq!(filled(None), None);
    }
}
