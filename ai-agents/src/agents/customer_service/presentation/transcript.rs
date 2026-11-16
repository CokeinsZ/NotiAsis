use super::calendar::short_stamp;
use crate::agents::customer_service::domain::{Author, MediaKind, QuotedMessage, TranscriptEntry};
use crate::shared::text::truncate_chars;

/// Caracteres del mensaje citado que se muestran.
const QUOTE_CHARS: usize = 120;

/// Historial del chat como texto, una línea por mensaje:
///
/// ```text
/// [dom 15/11 13:09] NOTIFICACIÓN AUTOMÁTICA · guía 240064842467: Hola Carmen...
/// [dom 15/11 13:50] CLIENTE (cita a ASESOR HUMANO del dom 15/11 13:20: «¿Me envías una foto?»): Ya te la mando
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
    if let Some(quoted) = &entry.quoted {
        header.push_str(&format!(" ({})", quote(quoted)));
    }
    format!("{header}: {}", body(entry.author, &entry.media, entry.text.as_deref(), entry.transcription.as_deref()))
}

/// `cita a ASESOR HUMANO del dom 15/11 13:20: «...»`, en una línea.
fn quote(quoted: &QuotedMessage) -> String {
    let content = body(quoted.author, &quoted.media, quoted.text.as_deref(), quoted.transcription.as_deref());
    let flat = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let excerpt = truncate_chars(&flat, QUOTE_CHARS);
    let ellipsis = if excerpt.len() < flat.len() { "…" } else { "" };
    format!("cita a {} del {}: «{excerpt}{ellipsis}»", author_label(quoted.author), short_stamp(quoted.at))
}

fn author_label(author: Author) -> &'static str {
    match author {
        Author::Customer => "CLIENTE",
        Author::Assistant => "TÚ (asistente)",
        Author::Associate => "ASESOR HUMANO",
        Author::Notification => "NOTIFICACIÓN AUTOMÁTICA",
    }
}

fn body(author: Author, media: &MediaKind, text: Option<&str>, transcription: Option<&str>) -> String {
    let from_customer = author == Author::Customer;
    let caption = text.map(str::trim).filter(|text| !text.is_empty());
    let with_caption = |label: &str| match caption {
        Some(text) => format!("{label} {text}"),
        None => label.to_string(),
    };
    match media {
        MediaKind::Text => caption.unwrap_or("").to_string(),
        MediaKind::Audio => format!(
            "[nota de voz] {}",
            transcription.map(str::trim).filter(|text| !text.is_empty()).unwrap_or("(sin transcripción)")
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
            quoted: None,
        }
    }

    #[test]
    fn a_reply_shows_the_quoted_message_in_one_line() {
        let mut reply = entry(Author::Customer, MediaKind::Text, Some("Ya te la mando"));
        reply.quoted = Some(QuotedMessage {
            at: NaiveDate::from_ymd_opt(2026, 11, 15).unwrap().and_hms_opt(13, 20, 0).unwrap(),
            author: Author::Associate,
            media: MediaKind::Text,
            text: Some("¿Me envías\nuna foto?".into()),
            transcription: None,
        });
        assert_eq!(
            format_entry(&reply),
            "[dom 15/11 13:50] CLIENTE (cita a ASESOR HUMANO del dom 15/11 13:20: «¿Me envías una foto?»): Ya te la mando"
        );

        let long = "x".repeat(200);
        reply.quoted.as_mut().unwrap().text = Some(long);
        assert!(format_entry(&reply).contains(&format!("«{}…»", "x".repeat(120))));
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
