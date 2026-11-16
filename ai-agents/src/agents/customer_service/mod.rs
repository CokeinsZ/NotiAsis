//! Agente de atención al cliente por WhatsApp.
//!
//! Cuando un cliente escribe, el bot registra el mensaje en el backend y
//! avisa aquí (`POST /agents/customer-service/events`). Cada aviso corre un
//! pipeline de pasos (`steps/`), en este orden:
//!
//! 1. `WaitForSilence`: espera unos segundos por si el cliente sigue escribiendo.
//! 2. `LockChat`: un solo turno a la vez por chat.
//! 3. `CheckEligibility`: reglas de `policy/` (IA activa, último mensaje,
//!    ventana de 24 h, asesor humano, escalamiento abierto, límite...).
//! 4. `ReserveCapacity`: límite de turnos simultáneos con el modelo.
//! 5. `LoadCustomerContext`: business, cliente y sus guías.
//! 6. `TranscribeVoiceNotes`: notas de voz pendientes → texto (Gemini).
//! 7. `LoadTranscript`: últimos mensajes del chat (tabla `messages`).
//! 8. `GenerateReply`: el agente (DeepSeek + `tools/`) escribe la respuesta.
//! 9. `DeliverReply`: revisa las reglas otra vez y la envía por el backend.
//!
//! Dónde cambiar cada cosa:
//! - El texto del prompt: `prompts/system.md` y `prompts/user.md`.
//! - Una herramienta nueva: un archivo en `tools/` + una línea en `tools/mod.rs`.
//! - Una regla nueva: un archivo en `policy/rules/` + una línea en `policy/mod.rs`.
//! - Un paso nuevo: un archivo en `steps/` + una línea en `assembly.rs`.

mod assembly;
pub mod config;
pub mod controller;
pub mod delivery;
pub mod dispatcher;
pub mod domain;
pub mod eligibility;
pub mod policy;
pub mod ports;
pub mod presentation;
pub mod prompts;
pub mod reply;
pub mod repositories;
pub mod steps;
pub mod tools;
pub mod turn;

pub use assembly::{CustomerServiceDeps, build_pipeline, router};
pub use config::CustomerServiceConfig;
