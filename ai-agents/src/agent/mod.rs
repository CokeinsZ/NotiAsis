//! Runtime genérico de agentes: un modelo de chat que puede llamar
//! herramientas en bucle hasta dar una respuesta final.
//!
//! No sabe nada de NotiAsis: el tipo `C` es el contexto que el agente
//! concreto le pasa a sus herramientas (por ejemplo, el chat que atiende).

mod conversation;
mod runner;
mod tool;
mod toolbox;
mod typed_tool;

pub use conversation::Conversation;
pub use runner::{AgentError, AgentRun, AgentRunner};
pub use tool::{Tool, ToolError};
pub use toolbox::ToolBox;
pub use typed_tool::{TypedTool, parameters_schema};
