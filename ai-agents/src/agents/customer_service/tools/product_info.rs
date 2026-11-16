use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::CustomerToolContext;
use crate::agent::{ToolError, TypedTool};
use crate::agents::customer_service::domain::{ProductSheet, best_matches};
use crate::agents::customer_service::ports::ProductCatalog;
use crate::agents::customer_service::presentation::products::product_json;

/// Máximo de fichas por búsqueda.
const MAX_MATCHES: usize = 3;

/// `info_producto`: busca una ficha en el catálogo del business del chat.
pub struct ProductInfoTool {
    catalog: Arc<dyn ProductCatalog>,
}

impl ProductInfoTool {
    pub fn new(catalog: Arc<dyn ProductCatalog>) -> Self {
        Self { catalog }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProductInfoArgs {
    /// Nombre del producto como lo dice el cliente o como aparece en su
    /// pedido (por ejemplo "pimienta de cayena" o "testosterona").
    pub producto: String,
}

#[async_trait]
impl TypedTool<CustomerToolContext> for ProductInfoTool {
    type Args = ProductInfoArgs;

    const NAME: &'static str = "info_producto";
    const DESCRIPTION: &'static str = "Ficha de un producto del catálogo del negocio (para qué sirve, ingredientes, \
presentación, dosis, modo de uso, precauciones, duración, envío y garantía). Úsala cuando pregunten por un producto \
que no está en las fichas que ya tienes.";

    async fn call(&self, context: &CustomerToolContext, args: ProductInfoArgs) -> Result<Value, ToolError> {
        let catalog = self
            .catalog
            .products(context.chat.business_id)
            .await
            .map_err(|error| ToolError::Failed(error.to_string()))?;
        Ok(search_result(&catalog, &args.producto))
    }
}

/// Las fichas que coinciden o, si ninguna, los nombres del catálogo para
/// que el modelo pueda volver a buscar.
fn search_result(catalog: &[ProductSheet], query: &str) -> Value {
    if catalog.is_empty() {
        return json!({ "encontrado": false, "mensaje": "Este negocio no tiene fichas de productos cargadas." });
    }
    let matches = best_matches(catalog, query, MAX_MATCHES);
    if matches.is_empty() {
        let names: Vec<&str> = catalog.iter().map(|product| product.name.as_str()).collect();
        return json!({
            "encontrado": false,
            "mensaje": "No hay un producto con ese nombre en el catálogo.",
            "productos_del_catalogo": names
        });
    }
    json!({
        "encontrado": true,
        "productos": matches.into_iter().map(product_json).collect::<Vec<_>>()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Vec<ProductSheet> {
        vec![
            ProductSheet {
                name: "Pimienta de Cayena (Cayenne Pepper)".into(),
                dosage: Some("2 cápsulas al día".into()),
                ..Default::default()
            },
            ProductSheet { name: "Testosterone+ Advanced 15 en 1".into(), ..Default::default() },
        ]
    }

    #[test]
    fn returns_the_matching_sheet() {
        let result = search_result(&catalog(), "la pimienta de cayena");
        assert_eq!(result["encontrado"], true);
        assert_eq!(result["productos"][0]["dosis"], "2 cápsulas al día");
        assert_eq!(result["productos"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn unknown_product_lists_the_catalog() {
        let result = search_result(&catalog(), "collar girasol");
        assert_eq!(result["encontrado"], false);
        assert_eq!(
            result["productos_del_catalogo"],
            json!(["Pimienta de Cayena (Cayenne Pepper)", "Testosterone+ Advanced 15 en 1"])
        );
    }

    #[test]
    fn empty_catalog_says_so() {
        assert_eq!(search_result(&[], "cayena")["mensaje"], "Este negocio no tiene fichas de productos cargadas.");
    }
}
