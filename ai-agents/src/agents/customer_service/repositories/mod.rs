//! Implementaciones con Postgres de los puertos del agente. Una por puerto;
//! todas comparten el pool. Las fechas de la base son UTC y se pasan a la
//! hora local del business en SQL (`countries.timezone`).

mod case_history;
mod chat_state;
mod customer_context;
mod escalations;
mod guide_rows;
mod guide_updates;
mod guides;
mod product_rows;
mod products;
mod transcript;
mod voice_notes;

pub use chat_state::PgChatStateReader;
pub use customer_context::PgCustomerContextReader;
pub use escalations::PgEscalationWriter;
pub use guide_updates::PgGuideUpdateWriter;
pub use guides::PgGuideReader;
pub use products::PgProductCatalog;
pub use transcript::PgTranscriptReader;
pub use voice_notes::PgVoiceNoteStore;
