use chrono::{NaiveDate, NaiveDateTime};

/// Producto de un pedido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductLine {
    pub name: String,
    pub quantity: i32,
}

/// En qué va el envío, según los avisos que NotiAsis ya le envió al
/// cliente (no es el estado en vivo de la transportadora).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideStage {
    AtOffice,
    InTransit,
    Dispatched,
    Registered,
}

impl GuideStage {
    /// `notification_count` > 0 = avisos de oficina; `tracking_level` 1 =
    /// despachado, 2 = en camino.
    pub fn from_counters(office_notifications: i32, tracking_level: i16) -> Self {
        match (office_notifications, tracking_level) {
            (count, _) if count > 0 => Self::AtOffice,
            (_, level) if level >= 2 => Self::InTransit,
            (_, 1) => Self::Dispatched,
            _ => Self::Registered,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::AtOffice => "En la oficina de la transportadora, listo para reclamar",
            Self::InTransit => "En camino (en transporte o en reparto)",
            Self::Dispatched => "Despachado (guía generada / entregado a la transportadora)",
            Self::Registered => "Registrado, sin seguimiento",
        }
    }
}

/// Guía (pedido) de un cliente, como se le muestra al agente.
#[derive(Debug, Clone, PartialEq)]
pub struct GuideSummary {
    pub number: String,
    pub customer_name: Option<String>,
    pub products: Vec<ProductLine>,
    pub delivery_address: Option<String>,
    pub city: Option<String>,
    pub shipping_company: Option<String>,
    pub order_total: Option<f64>,
    pub currency: Option<String>,
    pub stage: GuideStage,
    pub office_notifications: i32,
    /// Primer aviso de oficina (fecha local).
    pub at_office_since: Option<NaiveDate>,
    /// Último aviso enviado (fecha local).
    pub last_notified_on: Option<NaiveDate>,
}

/// Tipo de aviso enviado (`guide_notifications.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    Office,
    Tracking,
}

/// Un aviso que NotiAsis le envió al cliente sobre una guía.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationEvent {
    pub kind: NotificationKind,
    pub step: i32,
    /// Fecha y hora local.
    pub at: NaiveDateTime,
}

impl NotificationEvent {
    pub fn label(&self) -> &'static str {
        match (self.kind, self.step) {
            (NotificationKind::Tracking, 1) => "Pedido despachado",
            (NotificationKind::Tracking, _) => "Pedido en camino",
            (NotificationKind::Office, 1) => "Llegó a la oficina (aviso inicial)",
            (NotificationKind::Office, 2) => "Recordatorio",
            (NotificationKind::Office, 3) => "Recordatorio final: riesgo de devolución",
            (NotificationKind::Office, _) => "Seguimiento diario en oficina",
        }
    }
}

/// Guía con todos sus avisos (herramienta `detalle_guia`).
#[derive(Debug, Clone, PartialEq)]
pub struct GuideDetails {
    pub summary: GuideSummary,
    /// Id del pedido en Dropi (para consultar su estado en vivo).
    pub order_id: Option<String>,
    pub timeline: Vec<NotificationEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_prefers_office_over_tracking_level() {
        assert_eq!(GuideStage::from_counters(3, 2), GuideStage::AtOffice);
        assert_eq!(GuideStage::from_counters(0, 2), GuideStage::InTransit);
        assert_eq!(GuideStage::from_counters(0, 1), GuideStage::Dispatched);
        assert_eq!(GuideStage::from_counters(0, 0), GuideStage::Registered);
    }

    #[test]
    fn notification_labels_follow_the_escalation() {
        let at = NaiveDate::from_ymd_opt(2026, 11, 6).unwrap().and_hms_opt(13, 0, 0).unwrap();
        let event = |kind, step| NotificationEvent { kind, step, at }.label();
        assert_eq!(event(NotificationKind::Office, 3), "Recordatorio final: riesgo de devolución");
        assert_eq!(event(NotificationKind::Office, 8), "Seguimiento diario en oficina");
        assert_eq!(event(NotificationKind::Tracking, 2), "Pedido en camino");
    }
}
