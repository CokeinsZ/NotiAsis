use super::env::{ConfigError, Env};

#[derive(Debug, Clone)]
pub struct ServerSettings {
    pub port: u16,
    /// Header `api-key` que deben mandar quienes llaman al servicio (el bot).
    pub api_key: String,
}

#[derive(Debug, Clone)]
pub struct DatabaseSettings {
    pub url: String,
    pub max_connections: u32,
}

pub fn server(env: &Env) -> Result<ServerSettings, ConfigError> {
    Ok(ServerSettings { port: env.parse("PORT", 3002)?, api_key: env.required("AI_AGENTS_API_KEY")? })
}

pub fn database(env: &Env) -> Result<DatabaseSettings, ConfigError> {
    Ok(DatabaseSettings {
        url: env.required("DATABASE_URL")?,
        max_connections: env.parse("DATABASE_MAX_CONNECTIONS", 5)?,
    })
}
