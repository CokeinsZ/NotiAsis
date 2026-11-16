use chrono::NaiveDateTime;

/// Quién escribió un mensaje del chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Author {
    Customer,
    /// El propio agente (`sender = 'ai'`).
    Assistant,
    /// Un asesor desde la webapp (`sender = 'associate'`).
    Associate,
    /// Plantillas automáticas del bot (`sender = 'bot'` o mensajes
    /// anteriores a la columna `sender`).
    Notification,
}

impl Author {
    pub fn from_columns(from_user: bool, sender: Option<&str>) -> Self {
        match (from_user, sender) {
            (true, _) => Self::Customer,
            (false, Some("ai")) => Self::Assistant,
            (false, Some("associate")) => Self::Associate,
            (false, _) => Self::Notification,
        }
    }
}

/// Tipo de contenido (`messages.media_type`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaKind {
    Text,
    Audio,
    Image,
    Video,
    Document,
    Other(String),
}

impl MediaKind {
    pub fn from_column(value: &str) -> Self {
        match value {
            "text" => Self::Text,
            "audio" => Self::Audio,
            "image" => Self::Image,
            "video" => Self::Video,
            "document" => Self::Document,
            other => Self::Other(other.to_string()),
        }
    }
}

/// Un mensaje del historial del chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptEntry {
    /// Fecha y hora local del business.
    pub at: NaiveDateTime,
    pub author: Author,
    pub media: MediaKind,
    /// Texto o caption.
    pub text: Option<String>,
    /// Transcripción de una nota de voz.
    pub transcription: Option<String>,
    pub guide_number: Option<String>,
    /// Meta no lo entregó: el cliente nunca lo vio.
    pub failed: bool,
    /// El mensaje que el cliente citó al responder (puede ser anterior al
    /// historial que ve el agente).
    pub quoted: Option<QuotedMessage>,
}

/// Un mensaje citado, con lo necesario para mostrar de qué se habla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotedMessage {
    pub at: NaiveDateTime,
    pub author: Author,
    pub media: MediaKind,
    pub text: Option<String>,
    pub transcription: Option<String>,
}

/// Nota de voz del cliente aún sin transcribir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingVoiceNote {
    pub message_id: i32,
    pub media_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn author_comes_from_direction_and_sender() {
        assert_eq!(Author::from_columns(true, None), Author::Customer);
        assert_eq!(Author::from_columns(false, Some("ai")), Author::Assistant);
        assert_eq!(Author::from_columns(false, Some("associate")), Author::Associate);
        assert_eq!(Author::from_columns(false, Some("bot")), Author::Notification);
        assert_eq!(Author::from_columns(false, None), Author::Notification);
    }
}
