use crate::shared::text::truncate_chars;

/// Marca con la que el modelo indica que no hay que responder.
pub const SILENCE_MARKER: &str = "[NO_RESPONDER]";

/// Largo máximo de un mensaje de texto de WhatsApp (y del backend).
pub const MAX_REPLY_CHARS: usize = 4096;

/// Qué hacer con la salida del modelo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentReply {
    Send(String),
    Silent,
}

impl AgentReply {
    pub fn from_model_output(output: Option<&str>) -> Self {
        let Some(text) = output.map(str::trim).filter(|text| !text.is_empty()) else {
            return Self::Silent;
        };
        if text.contains(SILENCE_MARKER) {
            return Self::Silent;
        }
        let unquoted = text.strip_prefix('"').and_then(|inner| inner.strip_suffix('"')).map(str::trim).unwrap_or(text);
        if unquoted.is_empty() {
            return Self::Silent;
        }
        Self::Send(truncate_chars(unquoted, MAX_REPLY_CHARS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_marker_and_empty_output_mean_no_reply() {
        assert_eq!(AgentReply::from_model_output(Some("[NO_RESPONDER]")), AgentReply::Silent);
        assert_eq!(AgentReply::from_model_output(Some("  [NO_RESPONDER]\n")), AgentReply::Silent);
        assert_eq!(AgentReply::from_model_output(Some("   ")), AgentReply::Silent);
        assert_eq!(AgentReply::from_model_output(None), AgentReply::Silent);
    }

    #[test]
    fn replies_are_trimmed_unquoted_and_capped() {
        assert_eq!(
            AgentReply::from_model_output(Some("  \"¡Con gusto, Carmen!\" ")),
            AgentReply::Send("¡Con gusto, Carmen!".into())
        );
        let long = "a".repeat(5000);
        assert_eq!(AgentReply::from_model_output(Some(&long)), AgentReply::Send("a".repeat(MAX_REPLY_CHARS)));
    }
}
