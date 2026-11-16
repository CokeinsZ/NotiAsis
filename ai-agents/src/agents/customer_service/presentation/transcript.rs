use super::calendar::short_stamp;
use crate::agents::customer_service::domain::{Author, MediaKind, TranscriptEntry};

/// Historial del chat como texto, una línea por mensaje:
///
/// ```text
/// [dom 15/11 13:09] NOTIFICACIÓN AUTOMÁTICA · guía 240064842467: Hola Carmen...
/// [dom 15/11 13:50] CLIENTE: Mañana con el favor de Dios
/// ```
pub fn format_transcript(entries: &[TranscriptEntry]) -> String {
    if entries.is_empty() {
        return "(el chat no tiene mensajes)".to_string();
    }
    entries.iter().map(format_entry).collect::<Vec<_>>().join("\n")
}

pub fn format_entry(entry: &TranscriptEntry) -> String {
    let mut header = format!("[{}] {}", short_stamp(entry.at), author_label(entry.author));
    if let Some(guide) = &entry.guide_number {
        header.push_str(&format!(" · guía {guide}"));
    }
    if entry.failed {
        header.push_str(" · NO ENTREGADO");
    }
    format!("{header}: {}", body(entry))
}

fn author_label(author: Author) -> &'static str {
    match author {
        Author::Customer => "CLIENTE",
        Author::Assistant => "TÚ (asistente)",
        Author::Associate => "ASESOR HUMANO",
        Author::Notification => "NOTIFICACIÓN AUTOMÁTICA",
    }
}

fn body(entry: &TranscriptEntry) -> String {
    let from_customer = entry.author == Author::Customer;
    let caption = entry.text.as_deref().map(str::trim).filter(|text| !text.is_empty());
    let with_caption = |label: &str| match caption {
        Some(text) => format!("{label} {text}"),
        None => label.to_string(),
    };
    match &entry.media {
        MediaKind::Text => caption.unwrap_or("").to_string(),
        MediaKind::Audio => format!(
            "[nota de voz] {}",
            entry
                .transcription
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .unwrap_or("(sin transcripción)")
        ),
        MediaKind::Image => with_caption("[imagen]"),
        MediaKind::Video if from_customer => with_caption("[video]"),
        MediaKind::Video => with_caption("[video del producto]"),
        MediaKind::Document if from_customer => with_caption("[documento]"),
        MediaKind::Document => with_caption("[PDF de la guía de envío]"),
        MediaKind::Other(kind) => with_caption(&format!("[{kind}]")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn entry(author: Author, media: MediaKind, text: Option<&str>) -> TranscriptEntry {
        TranscriptEntry {
            at: NaiveDate::from_ymd_opt(2026, 11, 15).unwrap().and_hms_opt(13, 50, 0).unwrap(),
            author,
            media,
            text: text.map(str::to_string),
            transcription: None,
            guide_number: None,
            failed: false,
        }
    }

    #[test]
    fn labels_authors_guides_and_failures() {
        let mut notification = entry(Author::Notification, MediaKind::Text, Some("Tu pedido te espera"));
        notification.guide_number = Some("240064842467".into());
        notification.failed = true;
        assert_eq!(
            format_entry(&notification),
            "[dom 15/11 13:50] NOTIFICACIÓN AUTOMÁTICA · guía 240064842467 · NO ENTREGADO: Tu pedido te espera"
        );
        assert_eq!(
            format_entry(&entry(Author::Customer, MediaKind::Text, Some("Mañana paso"))),
            "[dom 15/11 13:50] CLIENTE: Mañana paso"
        );
    }

    #[test]
    fn describes_media_by_author() {
        let mut voice = entry(Author::Customer, MediaKind::Audio, None);
        assert!(format_entry(&voice).ends_with("[nota de voz] (sin transcripción)"));
        voice.transcription = Some("Voy el lunes".into());
        assert!(format_entry(&voice).ends_with("[nota de voz] Voy el lunes"));

        assert!(format_entry(&entry(Author::Notification, MediaKind::Video, None)).ends_with("[video del producto]"));
        assert!(
            format_entry(&entry(Author::Customer, MediaKind::Image, Some("esto llegó")))
                .ends_with("[imagen] esto llegó")
        );
        assert!(
            format_entry(&entry(Author::Notification, MediaKind::Document, None))
                .ends_with("[PDF de la guía de envío]")
        );
    }

    #[test]
    fn empty_chat_is_explicit() {
        assert_eq!(format_transcript(&[]), "(el chat no tiene mensajes)");
    }
}
