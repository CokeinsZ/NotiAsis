use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("missing environment variable {0}")]
    Missing(&'static str),
    #[error("invalid value for {key}: {reason}")]
    Invalid { key: &'static str, reason: String },
}

/// De dónde se leen las variables (el proceso, o un mapa en los tests).
pub trait EnvSource {
    fn get(&self, key: &str) -> Option<String>;
}

pub struct ProcessEnv;

impl EnvSource for ProcessEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// Variables en memoria (tests).
#[derive(Debug, Clone, Default)]
pub struct MapEnv(HashMap<String, String>);

impl MapEnv {
    pub fn set(&mut self, key: &str, value: &str) {
        self.0.insert(key.to_string(), value.to_string());
    }

    pub fn remove(&mut self, key: &str) {
        self.0.remove(key);
    }
}

impl<const N: usize> From<[(&str, &str); N]> for MapEnv {
    fn from(pairs: [(&str, &str); N]) -> Self {
        Self(pairs.into_iter().map(|(key, value)| (key.to_string(), value.to_string())).collect())
    }
}

impl EnvSource for MapEnv {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

/// Lectura tipada de variables. Una variable vacía cuenta como no definida.
pub struct Env<'a> {
    source: &'a dyn EnvSource,
}

impl<'a> Env<'a> {
    pub fn new(source: &'a dyn EnvSource) -> Self {
        Self { source }
    }

    pub fn optional(&self, key: &str) -> Option<String> {
        self.source.get(key).map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
    }

    pub fn required(&self, key: &'static str) -> Result<String, ConfigError> {
        self.optional(key).ok_or(ConfigError::Missing(key))
    }

    pub fn or(&self, key: &str, default: &str) -> String {
        self.optional(key).unwrap_or_else(|| default.to_string())
    }

    /// Valor parseado, o `default` si no está.
    pub fn parse<T>(&self, key: &'static str, default: T) -> Result<T, ConfigError>
    where
        T: FromStr,
        T::Err: std::fmt::Display,
    {
        self.parse_optional(key).map(|value| value.unwrap_or(default))
    }

    pub fn parse_optional<T>(&self, key: &'static str) -> Result<Option<T>, ConfigError>
    where
        T: FromStr,
        T::Err: std::fmt::Display,
    {
        self.optional(key)
            .map(|raw| raw.parse::<T>().map_err(|error| ConfigError::Invalid { key, reason: error.to_string() }))
            .transpose()
    }

    pub fn seconds(&self, key: &'static str, default: u64) -> Result<Duration, ConfigError> {
        self.parse(key, default).map(Duration::from_secs)
    }

    pub fn flag(&self, key: &'static str, default: bool) -> Result<bool, ConfigError> {
        match self.optional(key).map(|value| value.to_lowercase()) {
            None => Ok(default),
            Some(value) => match value.as_str() {
                "true" | "1" | "yes" | "on" => Ok(true),
                "false" | "0" | "no" | "off" => Ok(false),
                _ => Err(ConfigError::Invalid { key, reason: format!("expected true/false, got {value:?}") }),
            },
        }
    }

    /// Lista separada por comas (vacía si no está).
    pub fn list(&self, key: &str) -> Vec<String> {
        self.optional(key)
            .map(|raw| raw.split(',').map(str::trim).filter(|item| !item.is_empty()).map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// Lista separada por comas, o `default` si no está.
    pub fn list_or(&self, key: &str, default: &[String]) -> Vec<String> {
        match self.optional(key) {
            Some(_) => self.list(key),
            None => default.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_values_count_as_missing() {
        let source = MapEnv::from([("A", "  "), ("B", " x ")]);
        let env = Env::new(&source);
        assert_eq!(env.required("A"), Err(ConfigError::Missing("A")));
        assert_eq!(env.required("B").unwrap(), "x");
    }

    #[test]
    fn parses_numbers_flags_and_lists() {
        let source = MapEnv::from([("N", "42"), ("F", "Yes"), ("L", "a, b,,c"), ("BAD", "x")]);
        let env = Env::new(&source);
        assert_eq!(env.parse::<u32>("N", 1).unwrap(), 42);
        assert_eq!(env.parse::<u32>("MISSING", 7).unwrap(), 7);
        assert!(env.flag("F", false).unwrap());
        assert_eq!(env.list("L"), vec!["a", "b", "c"]);
        assert!(env.parse::<u32>("BAD", 1).is_err());
    }
}
