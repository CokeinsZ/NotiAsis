//! Lo que el agente de IA (servicio ai-agents) registró en un chat:
//! escalaciones a un asesor (`ai_escalations`) y novedades de guías
//! (`guide_customer_updates`). Solo lectura: las escribe ai-agents.
pub mod dtos;
pub mod controller;
pub mod service;
pub mod repository;
