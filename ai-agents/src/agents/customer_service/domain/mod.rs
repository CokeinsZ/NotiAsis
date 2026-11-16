//! Tipos del agente de atención al cliente.

mod actions;
mod case_history;
mod context;
mod event;
mod guide;
mod product;
mod snapshot;
mod transcript;

pub use actions::{Escalation, EscalationCategory, GuideUpdate, GuideUpdateKind};
pub use case_history::{CaseHistory, PastEscalation, PastGuideUpdate};
pub use context::{AgentPersona, BusinessProfile, CustomerContext};
pub use event::IncomingMessageEvent;
pub use guide::{GuideDetails, GuideStage, GuideSummary, NotificationEvent, NotificationKind, ProductLine};
pub use product::{ProductSheet, best_matches};
pub use snapshot::{AgentSwitch, ChatSnapshot, IncomingRef};
pub use transcript::{Author, MediaKind, PendingVoiceNote, QuotedMessage, TranscriptEntry};
