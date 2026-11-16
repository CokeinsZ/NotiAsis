use super::calendar::{short_date, short_stamp};
use crate::agents::customer_service::domain::{
    CaseHistory, EscalationCategory, GuideUpdateKind, PastEscalation, PastGuideUpdate,
};

/// Escalamientos y novedades anteriores, una línea por evento y en orden:
///
/// ```text
/// [vie 13/11 10:00] Escalaste a un asesor (problema de envío) · guía 240064842467: quiere recibirlo en casa. Un asesor escribió después.
/// [sáb 14/11 09:12] Registraste · guía 240064842467: dijo que lo recoge el lun 16/11 (cobra el lunes).
/// ```
pub fn case_history_for_prompt(history: &CaseHistory) -> String {
    if history.is_empty() {
        return "(nada: no has escalado este chat ni registrado novedades)".to_string();
    }
    let mut lines: Vec<_> = history
        .escalations
        .iter()
        .map(|item| (item.at, escalation_line(item)))
        .chain(history.updates.iter().map(|item| (item.at, update_line(item))))
        .collect();
    lines.sort_by_key(|(at, _)| *at);
    lines.into_iter().map(|(_, line)| line).collect::<Vec<_>>().join("\n")
}

fn escalation_line(item: &PastEscalation) -> String {
    let escalation = &item.escalation;
    let status = if item.answered { "Un asesor escribió después." } else { "Ningún asesor ha respondido todavía." };
    format!(
        "[{}] Escalaste a un asesor ({}){}: {}. {status}",
        short_stamp(item.at),
        category_label(escalation.category),
        guide_suffix(escalation.guide_number.as_deref()),
        escalation.reason.trim().trim_end_matches('.'),
    )
}

fn update_line(item: &PastGuideUpdate) -> String {
    let update = &item.update;
    let what = match (update.kind, update.promised_date) {
        (GuideUpdateKind::WillPickup, Some(date)) => format!("dijo que lo recoge el {}", short_date(date)),
        (GuideUpdateKind::WillPickup, None) => "dijo que lo va a recoger".to_string(),
        (GuideUpdateKind::PickedUp, _) => "dijo que ya lo recogió".to_string(),
        (GuideUpdateKind::Declined, _) => "dijo que no lo quiere".to_string(),
    };
    let note = update
        .note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty())
        .map(|note| format!(" ({note})"))
        .unwrap_or_default();
    format!("[{}] Registraste{}: {what}{note}.", short_stamp(item.at), guide_suffix(Some(&update.guide_number)))
}

fn guide_suffix(guide: Option<&str>) -> String {
    guide.map(|guide| format!(" · guía {guide}")).unwrap_or_default()
}

fn category_label(category: EscalationCategory) -> &'static str {
    match category {
        EscalationCategory::Cancellation => "cancelación",
        EscalationCategory::ShippingIssue => "problema de envío",
        EscalationCategory::ProductIssue => "problema con el producto",
        EscalationCategory::ProductQuestion => "pregunta del producto",
        EscalationCategory::OptOut => "no quiere más mensajes",
        EscalationCategory::Complaint => "queja",
        EscalationCategory::HumanRequested => "pidió hablar con una persona",
        EscalationCategory::Other => "otro motivo",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::customer_service::domain::{Escalation, GuideUpdate};
    use chrono::{NaiveDate, NaiveDateTime};

    fn at(day: u32, hour: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap().and_hms_opt(hour, 0, 0).unwrap()
    }

    #[test]
    fn events_are_merged_in_order_with_their_status() {
        let history = CaseHistory {
            escalations: vec![PastEscalation {
                at: at(13, 10),
                escalation: Escalation {
                    category: EscalationCategory::ShippingIssue,
                    reason: "Quiere recibirlo en casa.".into(),
                    guide_number: Some("240064842467".into()),
                },
                answered: true,
            }],
            updates: vec![
                PastGuideUpdate {
                    at: at(12, 9),
                    update: GuideUpdate {
                        guide_number: "240064842467".into(),
                        kind: GuideUpdateKind::WillPickup,
                        promised_date: NaiveDate::from_ymd_opt(2026, 11, 16),
                        note: Some("cobra el lunes".into()),
                    },
                },
                PastGuideUpdate {
                    at: at(14, 9),
                    update: GuideUpdate {
                        guide_number: "240064842467".into(),
                        kind: GuideUpdateKind::PickedUp,
                        promised_date: None,
                        note: None,
                    },
                },
            ],
        };
        assert_eq!(
            case_history_for_prompt(&history),
            "[jue 12/11 09:00] Registraste · guía 240064842467: dijo que lo recoge el lun 16/11 (cobra el lunes).\n\
             [vie 13/11 10:00] Escalaste a un asesor (problema de envío) · guía 240064842467: Quiere recibirlo en casa. \
             Un asesor escribió después.\n\
             [sáb 14/11 09:00] Registraste · guía 240064842467: dijo que ya lo recogió."
        );
    }

    #[test]
    fn an_unanswered_escalation_says_so() {
        let history = CaseHistory {
            escalations: vec![PastEscalation {
                at: at(13, 10),
                escalation: Escalation {
                    category: EscalationCategory::HumanRequested,
                    reason: "pidió un asesor".into(),
                    guide_number: None,
                },
                answered: false,
            }],
            updates: vec![],
        };
        assert_eq!(
            case_history_for_prompt(&history),
            "[vie 13/11 10:00] Escalaste a un asesor (pidió hablar con una persona): pidió un asesor. \
             Ningún asesor ha respondido todavía."
        );
    }

    #[test]
    fn nothing_recorded_is_explicit() {
        assert_eq!(
            case_history_for_prompt(&CaseHistory::default()),
            "(nada: no has escalado este chat ni registrado novedades)"
        );
    }
}
