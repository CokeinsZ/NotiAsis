use std::time::Duration;

use super::env::{ConfigError, Env};

#[derive(Debug, Clone)]
pub struct BackendSettings {
    pub base_url: String,
    /// api_key de la tabla `api_keys` (propia de este servicio).
    pub api_key: String,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct DropiSettings {
    /// Base de los webhooks de N8N (`/orden`).
    pub base_url: String,
    pub api_key: String,
    pub timeout: Duration,
}

pub fn backend(env: &Env) -> Result<BackendSettings, ConfigError> {
    Ok(BackendSettings {
        base_url: env.required("BACKEND_API_URL")?,
        api_key: env.required("BACKEND_API_KEY")?,
        timeout: env.seconds("BACKEND_TIMEOUT_SECS", 30)?,
    })
}

pub fn dropi(env: &Env) -> Result<DropiSettings, ConfigError> {
    Ok(DropiSettings {
        base_url: env.or("DROPI_N8N_URL", "https://n8n.notiasis.com/webhook/dropi"),
        api_key: env.required("N8N_API_KEY")?,
        timeout: env.seconds("DROPI_TIMEOUT_SECS", 30)?,
    })
}
