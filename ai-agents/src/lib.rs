//! Agentes de IA de NotiAsis.
//!
//! Capas, de lo más genérico a lo más específico (cada una solo depende de
//! las anteriores):
//!
//! - [`shared`]: utilidades transversales (reintentos, HTTP JSON, plantillas,
//!   locks por clave, texto).
//! - [`llm`] y [`transcription`]: puertos de los modelos de IA y sus
//!   adaptadores (DeepSeek para chat, Gemini para transcribir audio).
//! - [`agent`]: runtime genérico de agentes (bucle modelo ↔ herramientas).
//! - [`pipeline`]: ejecución de pasos encadenados sobre un estado.
//! - [`domain`]: tipos de NotiAsis compartidos por todos los agentes.
//! - [`integrations`]: clientes de servicios externos (backend, Dropi).
//! - [`agents`]: agentes concretos (hoy: atención al cliente).
//! - [`config`], [`http`], [`observability`] y [`app`]: configuración,
//!   servidor HTTP, logs y composition root.

pub mod agent;
pub mod agents;
pub mod app;
pub mod config;
pub mod domain;
pub mod http;
pub mod integrations;
pub mod llm;
pub mod observability;
pub mod pipeline;
pub mod shared;
pub mod transcription;
