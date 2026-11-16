//! Prompts del agente. El texto vive en `system.md` y `user.md` (se
//! compilan dentro del binario): para cambiar el comportamiento se edita
//! el markdown, no el código.

use crate::agent::Conversation;
use crate::agents::customer_service::domain::{CustomerContext, TranscriptEntry};
use crate::agents::customer_service::presentation::{
    calendar::describe_moment, guides::guides_for_prompt, transcript::format_transcript,
};
use crate::shared::template::{PromptTemplate, TemplateError, TemplateValues};

const SYSTEM_TEMPLATE: &str = include_str!("system.md");
const USER_TEMPLATE: &str = include_str!("user.md");

/// Políticas cuando el business no configuró ninguna.
pub const DEFAULT_POLICIES: &str = "No hay políticas configuradas. No prometas plazos, horarios, medios de pago, \
cambios ni devoluciones: si te preguntan, di que lo consultas con el equipo y escala.";

/// Arma la conversación inicial (prompt de sistema + historial).
pub struct PromptBuilder {
    system: PromptTemplate,
    user: PromptTemplate,
}

impl PromptBuilder {
    pub fn new(system: &str, user: &str) -> Result<Self, TemplateError> {
        Ok(Self { system: PromptTemplate::parse(system)?, user: PromptTemplate::parse(user)? })
    }

    /// Los prompts de `system.md` y `user.md`.
    pub fn standard() -> Self {
        Self::new(SYSTEM_TEMPLATE, USER_TEMPLATE).expect("built-in prompts must be valid templates")
    }

    pub fn conversation(
        &self,
        context: &CustomerContext,
        transcript: &[TranscriptEntry],
    ) -> Result<Conversation, TemplateError> {
        let policies = match context.persona.policies.trim() {
            "" => DEFAULT_POLICIES,
            policies => policies,
        };
        let system = self.system.render(
            &TemplateValues::new()
                .with("nombre_agente", &context.persona.name)
                .with("negocio", &context.business.name)
                .with("pais", &context.business.country)
                .with("fecha_local", describe_moment(context.business.local_now))
                .with("nombre_whatsapp", display_name(&context.customer_name))
                .with("pedidos", guides_for_prompt(&context.guides))
                .with("politicas", policies),
        )?;
        let user = self.user.render(&TemplateValues::new().with("historial", format_transcript(transcript)))?;
        Ok(Conversation::new(system, user))
    }
}

fn display_name(name: &str) -> &str {
    match name.trim() {
        "" => "(sin nombre)",
        name => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::customer_service::domain::{AgentPersona, Author, BusinessProfile, MediaKind};
    use crate::domain::ChatKey;
    use crate::llm::ChatMessage;
    use chrono::NaiveDate;

    fn context(policies: &str) -> CustomerContext {
        CustomerContext {
            chat: ChatKey::new(1, "573214436449"),
            business: BusinessProfile {
                name: "Victor Store".into(),
                country: "Colombia".into(),
                timezone: "America/Bogota".into(),
                local_now: NaiveDate::from_ymd_opt(2026, 11, 15).unwrap().and_hms_opt(15, 41, 0).unwrap(),
            },
            persona: AgentPersona { name: "Verónica".into(), policies: policies.into() },
            customer_name: "Carmen Buitrago".into(),
            guides: vec![],
        }
    }

    #[test]
    fn built_in_prompts_render_every_placeholder() {
        let transcript = vec![TranscriptEntry {
            at: NaiveDate::from_ymd_opt(2026, 11, 15).unwrap().and_hms_opt(13, 50, 0).unwrap(),
            author: Author::Customer,
            media: MediaKind::Text,
            text: Some("Mañana con el favor de Dios".into()),
            transcription: None,
            guide_number: None,
            failed: false,
        }];
        let conversation = PromptBuilder::standard().conversation(&context(""), &transcript).unwrap();
        let [ChatMessage::System(system), ChatMessage::User(user)] = conversation.messages() else {
            panic!("expected system + user");
        };
        assert!(system.starts_with("Eres Verónica, del equipo de atención al cliente de Victor Store (Colombia)."));
        assert!(system.contains("domingo 15 de noviembre de 2026, 3:41 p. m."));
        assert!(system.contains(DEFAULT_POLICIES));
        assert!(!system.contains("{{"), "unrendered placeholder in system prompt");
        assert!(user.contains("[dom 15/11 13:50] CLIENTE: Mañana con el favor de Dios"));
    }

    #[test]
    fn business_policies_replace_the_default() {
        let conversation = PromptBuilder::standard()
            .conversation(&context("La transportadora guarda el pedido 8 días."), &[])
            .unwrap();
        let ChatMessage::System(system) = &conversation.messages()[0] else { panic!() };
        assert!(system.contains("La transportadora guarda el pedido 8 días."));
        assert!(!system.contains(DEFAULT_POLICIES));
    }
}
