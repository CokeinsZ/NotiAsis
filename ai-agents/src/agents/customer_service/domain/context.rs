use chrono::NaiveDateTime;

use super::guide::GuideSummary;
use crate::domain::ChatKey;

/// Todo lo que el agente sabe del chat antes de leer el historial.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomerContext {
    pub chat: ChatKey,
    pub business: BusinessProfile,
    pub persona: AgentPersona,
    /// Nombre de perfil de WhatsApp del cliente.
    pub customer_name: String,
    /// Guías del cliente con este business, la más reciente primero.
    pub guides: Vec<GuideSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessProfile {
    pub name: String,
    pub country: String,
    pub timezone: String,
    /// Fecha y hora actual en la zona del business.
    pub local_now: NaiveDateTime,
}

/// Cómo se presenta el agente y qué reglas del negocio debe seguir
/// (`business_ai_agents`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPersona {
    pub name: String,
    pub policies: String,
}

impl CustomerContext {
    /// Palabras de este cliente que pueden aparecer en sus notas de voz
    /// (productos, transportadoras, ciudades, su nombre).
    pub fn vocabulary(&self) -> Vec<String> {
        let mut terms = vec![self.business.name.clone(), self.customer_name.clone()];
        for guide in &self.guides {
            terms.extend(guide.products.iter().map(|product| product.name.clone()));
            terms.extend(guide.shipping_company.clone());
            terms.extend(guide.city.clone());
            terms.extend(guide.customer_name.clone());
        }
        terms.retain(|term| !term.trim().is_empty());
        terms
    }
}
