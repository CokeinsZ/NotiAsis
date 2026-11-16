//! Agentes concretos. Cada agente es un módulo autocontenido que expone un
//! `router` y se monta en `app.rs`: agregar un agente es crear su carpeta
//! y montarlo con una línea.

pub mod customer_service;
