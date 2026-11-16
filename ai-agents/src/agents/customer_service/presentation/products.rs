use serde_json::{Map, Value, json};

use crate::agents::customer_service::domain::ProductSheet;

/// Fichas de producto para el prompt (JSON legible, claves en español).
pub fn products_for_prompt(products: &[ProductSheet]) -> String {
    if products.is_empty() {
        return "(no hay fichas de los productos de sus pedidos)".to_string();
    }
    let list: Vec<Value> = products.iter().map(product_json).collect();
    serde_json::to_string_pretty(&list).unwrap_or_default()
}

/// Una ficha en JSON, sin los datos que no tenemos.
pub fn product_json(product: &ProductSheet) -> Value {
    let mut fields = Map::new();
    fields.insert("producto".into(), json!(product.name));
    let optional = [
        ("para_que_sirve", &product.description),
        ("ingredientes", &product.ingredients),
        ("presentacion", &product.presentation),
        ("dosis", &product.dosage),
        ("modo_de_uso", &product.usage_instructions),
        ("precauciones", &product.warnings),
        ("duracion_del_tratamiento", &product.treatment_duration),
        ("envio", &product.shipping),
        ("garantia", &product.warranty),
        ("notas_para_ti", &product.agent_notes),
    ];
    for (key, value) in optional {
        if let Some(value) = value {
            fields.insert(key.into(), json!(value));
        }
    }
    Value::Object(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_json_uses_spanish_keys_and_omits_missing_data() {
        let product = ProductSheet {
            name: "Pimienta de Cayena (Cayenne Pepper)".into(),
            dosage: Some("2 cápsulas al día".into()),
            agent_notes: Some("Habla de duración, no de cápsulas.".into()),
            ..Default::default()
        };
        assert_eq!(
            product_json(&product),
            json!({
                "producto": "Pimienta de Cayena (Cayenne Pepper)",
                "dosis": "2 cápsulas al día",
                "notas_para_ti": "Habla de duración, no de cápsulas."
            })
        );
    }

    #[test]
    fn no_products_is_explicit() {
        assert_eq!(products_for_prompt(&[]), "(no hay fichas de los productos de sus pedidos)");
    }
}
