use serde_json::{Map, Value, json};

use super::calendar::short_stamp;
use super::money::format_money;
use crate::agents::customer_service::domain::{GuideDetails, GuideSummary};

/// Pedidos del cliente para el prompt (JSON legible, claves en español).
pub fn guides_for_prompt(guides: &[GuideSummary]) -> String {
    if guides.is_empty() {
        return "(no tiene pedidos registrados con este negocio)".to_string();
    }
    let list: Vec<Value> = guides.iter().map(guide_json).collect();
    serde_json::to_string_pretty(&list).unwrap_or_default()
}

/// Una guía en JSON, sin los campos vacíos.
pub fn guide_json(guide: &GuideSummary) -> Value {
    let products = guide
        .products
        .iter()
        .map(|product| format!("{} x {}", product.quantity, product.name))
        .collect::<Vec<_>>()
        .join(", ");

    let mut fields = Map::new();
    let mut put = |key: &str, value: Option<Value>| {
        if let Some(value) = value {
            fields.insert(key.to_string(), value);
        }
    };
    put("guia", Some(json!(guide.number)));
    put("cliente", guide.customer_name.clone().map(Value::from));
    put("productos", (!products.is_empty()).then(|| Value::from(products)));
    put("direccion", guide.delivery_address.clone().map(Value::from));
    put("ciudad", guide.city.clone().map(Value::from));
    put("transportadora", guide.shipping_company.clone().map(Value::from));
    put("valor_pedido", guide.order_total.map(|total| Value::from(format_money(total, guide.currency.as_deref()))));
    put("estado_segun_registro", Some(json!(guide.stage.description())));
    put("en_oficina_desde", guide.at_office_since.map(|date| Value::from(date.to_string())));
    put("avisos_de_oficina_enviados", (guide.office_notifications > 0).then(|| json!(guide.office_notifications)));
    put("ultimo_aviso", guide.last_notified_on.map(|date| Value::from(date.to_string())));
    Value::Object(fields)
}

/// Resultado de la herramienta `detalle_guia`: la guía y todos sus avisos.
pub fn guide_details_json(details: &GuideDetails) -> Value {
    let mut value = guide_json(&details.summary);
    let timeline: Vec<Value> = details
        .timeline
        .iter()
        .map(|event| json!({ "fecha": short_stamp(event.at), "aviso": event.label() }))
        .collect();
    if let Value::Object(fields) = &mut value {
        fields.insert("encontrada".into(), json!(true));
        fields.insert("avisos".into(), Value::from(timeline));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::customer_service::domain::{GuideStage, NotificationEvent, NotificationKind, ProductLine};
    use chrono::NaiveDate;

    fn guide() -> GuideSummary {
        GuideSummary {
            number: "240064842467".into(),
            customer_name: Some("Carmen Buitrago".into()),
            products: vec![ProductLine { name: "EXTRACTO PIMIENTA DE CAYENA 3000 Mg".into(), quantity: 1 }],
            delivery_address: Some("Oficina Interrapidisimo".into()),
            city: Some("SILVANIA".into()),
            shipping_company: Some("INTERRAPIDISIMO".into()),
            order_total: Some(79_900.0),
            currency: Some("COP".into()),
            stage: GuideStage::AtOffice,
            office_notifications: 8,
            at_office_since: NaiveDate::from_ymd_opt(2026, 11, 6),
            last_notified_on: NaiveDate::from_ymd_opt(2026, 11, 15),
        }
    }

    #[test]
    fn guide_json_uses_spanish_keys_and_formatted_money() {
        let value = guide_json(&guide());
        assert_eq!(value["productos"], "1 x EXTRACTO PIMIENTA DE CAYENA 3000 Mg");
        assert_eq!(value["valor_pedido"], "$79.900");
        assert_eq!(value["estado_segun_registro"], "En la oficina de la transportadora, listo para reclamar");
        assert_eq!(value["en_oficina_desde"], "2026-11-06");
        assert_eq!(value["avisos_de_oficina_enviados"], 8);
    }

    #[test]
    fn empty_fields_are_omitted() {
        let mut guide = guide();
        guide.customer_name = None;
        guide.products.clear();
        guide.office_notifications = 0;
        let value = guide_json(&guide);
        assert!(value.get("cliente").is_none());
        assert!(value.get("productos").is_none());
        assert!(value.get("avisos_de_oficina_enviados").is_none());
    }

    #[test]
    fn details_include_the_notification_timeline() {
        let at = NaiveDate::from_ymd_opt(2026, 11, 6).unwrap().and_hms_opt(13, 6, 0).unwrap();
        let details = GuideDetails {
            summary: guide(),
            order_id: Some("95204087".into()),
            timeline: vec![NotificationEvent { kind: NotificationKind::Office, step: 1, at }],
        };
        let value = guide_details_json(&details);
        assert_eq!(value["encontrada"], true);
        assert_eq!(
            value["avisos"][0],
            json!({"fecha": "vie 06/11 13:06", "aviso": "Llegó a la oficina (aviso inicial)"})
        );
    }

    #[test]
    fn no_guides_is_explicit() {
        assert_eq!(guides_for_prompt(&[]), "(no tiene pedidos registrados con este negocio)");
    }
}
