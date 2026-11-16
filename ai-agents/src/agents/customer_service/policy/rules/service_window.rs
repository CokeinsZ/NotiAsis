use chrono::Duration;

use crate::agents::customer_service::policy::{ReplyRule, RuleInput};

/// Ventana de atención de Meta: los mensajes libres solo se pueden enviar
/// hasta 24 h después del último mensaje del cliente.
const WINDOW: Duration = Duration::hours(24);

/// Margen para no intentar enviar justo cuando la ventana está cerrando.
const SAFETY_MARGIN: Duration = Duration::minutes(5);

pub struct ServiceWindow;

impl ReplyRule for ServiceWindow {
    fn name(&self) -> &'static str {
        "service_window"
    }

    fn check(&self, input: &RuleInput<'_>) -> Result<(), String> {
        let latest = input.snapshot.latest_incoming.ok_or("el chat no tiene mensajes del cliente")?;
        if input.snapshot.now - latest.at < WINDOW - SAFETY_MARGIN {
            Ok(())
        } else {
            Err("la ventana de 24 h de Meta está cerrada".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::customer_service::policy::fixtures::{event, snapshot};

    #[test]
    fn closes_before_24_hours() {
        let event = event();
        let mut snapshot = snapshot();
        assert!(ServiceWindow.check(&RuleInput { event: &event, snapshot: &snapshot }).is_ok());

        snapshot.now = snapshot.latest_incoming.unwrap().at + Duration::hours(24) - Duration::minutes(1);
        assert!(ServiceWindow.check(&RuleInput { event: &event, snapshot: &snapshot }).is_err());
    }
}
