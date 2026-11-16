//! Tipos del agente de atención al cliente.

mod actions;
mod context;
mod event;
mod guide;
mod snapshot;
mod transcript;

pub use actions::{Escalation, EscalationCategory, GuideUpdate, GuideUpdateKind};
pub use context::{AgentPersona, BusinessProfile, CustomerContext};
pub use event::IncomingMessageEvent;
pub use guide::{GuideDetails, GuideStage, GuideSummary, NotificationEvent, NotificationKind, ProductLine};
pub use snapshot::{AgentSwitch, ChatSnapshot, IncomingRef};
pub use transcript::{Author, MediaKind, PendingVoiceNote, TranscriptEntry};
