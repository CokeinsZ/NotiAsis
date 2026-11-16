//! Muestra lo que el agente de atención al cliente vería en un chat: el
//! prompt de sistema (persona, políticas, guías) y el historial, sin llamar
//! al modelo ni enviar nada. Solo lee la base de datos.
//!
//! ```bash
//! DATABASE_URL=postgres://... cargo run --example preview_prompt -- <business_id> <telefono>
//! ```

use ai_agents::agents::customer_service::CustomerServiceConfig;
use ai_agents::agents::customer_service::ports::{CustomerContextReader, TranscriptReader};
use ai_agents::agents::customer_service::prompts::PromptBuilder;
use ai_agents::agents::customer_service::repositories::{PgCustomerContextReader, PgTranscriptReader};
use ai_agents::domain::ChatKey;
use ai_agents::llm::ChatMessage;
use sqlx::postgres::PgPoolOptions;

const USAGE: &str = "uso: cargo run --example preview_prompt -- <business_id> <telefono>";

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let mut args = std::env::args().skip(1);
    let business_id: i32 = args.next().and_then(|value| value.parse().ok()).expect(USAGE);
    let phone = args.next().expect(USAGE);
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL no está definida");

    let pool = PgPoolOptions::new().max_connections(2).connect(&url).await.expect("no se pudo conectar a la base");
    let chat = ChatKey::new(business_id, phone);
    let config = CustomerServiceConfig::default();

    let context = PgCustomerContextReader::new(pool.clone())
        .load(&chat, config.guide_limit)
        .await
        .expect("error leyendo el contexto")
        .expect("el chat no existe");
    let transcript =
        PgTranscriptReader::new(pool).recent(&chat, config.history_limit).await.expect("error leyendo el historial");
    let conversation = PromptBuilder::standard().conversation(&context, &transcript).expect("prompt inválido");

    for message in conversation.messages() {
        match message {
            ChatMessage::System(text) => println!("=================== SYSTEM ===================\n{text}\n"),
            ChatMessage::User(text) => println!("==================== USER ====================\n{text}"),
            _ => {}
        }
    }
}
