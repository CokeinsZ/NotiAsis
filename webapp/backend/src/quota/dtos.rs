use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::tools::phones::{PHONE_REGEX, normalize_phone};

/// Cupo por defecto de clientes nuevos en 24 h. El límite de Meta para un
/// negocio sin verificar es 250; se deja margen porque nuestra cuenta puede
/// diferir de la de Meta (mensajes enviados por fuera del bot, relojes).
/// Se cambia con la variable de entorno MESSAGING_LIMIT.
pub const DEFAULT_MESSAGING_LIMIT: i64 = 230;

/// Tope por defecto del seguimiento (niveles 1 y 2) dentro del cupo: lo
/// demás queda para oficina, que tiene prioridad. MESSAGING_TRACKING_LIMIT.
pub const DEFAULT_TRACKING_LIMIT: i64 = 80;

/// Vigencia de una reserva que el bot no libera (se cayó a mitad de envío).
pub const DEFAULT_RESERVATION_MINUTES: i32 = 30;

/// Tipo de envío. Oficina (pedidos en "reclame en oficina", Dropi o Google
/// Sheet) puede usar todo el cupo; el seguimiento solo hasta su tope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuotaKind {
    Office,
    Tracking,
}

impl QuotaKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            QuotaKind::Office => "office",
            QuotaKind::Tracking => "tracking",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaLimits {
    /// Clientes nuevos en 24 h (MESSAGING_LIMIT).
    pub limit: i64,
    /// Cuántos de ellos puede ocupar el seguimiento (MESSAGING_TRACKING_LIMIT).
    pub tracking_limit: i64,
}

/// Una corrida del bot pide cupo para los teléfonos a los que va a escribir.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReserveQuotaDto {
    /// En orden de prioridad: si no alcanza el cupo se aprueban los primeros.
    #[validate(
        length(min = 1, max = 1000, message = "phones debe tener entre 1 y 1000 teléfonos"),
        custom(function = "validate_phones")
    )]
    pub phones: Vec<String>,

    pub kind: QuotaKind,

    /// Business que envía. Solo para auditoría: el cupo es del número de
    /// WhatsApp y lo comparten todos los business.
    pub business_id: Option<i32>,

    /// Minutos que dura la reserva si no se libera antes (por defecto 30).
    #[validate(range(min = 1, max = 180, message = "ttl_minutes debe estar entre 1 y 180"))]
    pub ttl_minutes: Option<i32>,
}

impl ReserveQuotaDto {
    /// Teléfonos normalizados como messages.user_id (sin '+'), sin
    /// repetidos y en el orden pedido.
    pub fn normalized_phones(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        self.phones
            .iter()
            .map(|phone| normalize_phone(phone))
            .filter(|phone| seen.insert(phone.clone()))
            .collect()
    }
}

fn validate_phones(phones: &[String]) -> Result<(), ValidationError> {
    if phones.iter().all(|phone| PHONE_REGEX.is_match(phone.trim())) {
        return Ok(());
    }
    let mut error = ValidationError::new("phones");
    error.message = Some("Cada teléfono debe tener entre 8 y 15 dígitos, con '+' opcional".into());
    Err(error)
}

/// Uso del cupo en las últimas 24 h.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[derive(sqlx::FromRow)]
pub struct QuotaTotals {
    /// Clientes únicos que ocupan cupo: con un mensaje saliente en las
    /// últimas 24 h que no falló y que se envió fuera de su ventana de
    /// atención (el cliente no nos había escrito en las 24 h anteriores).
    pub used: i64,
    /// Clientes reservados por envíos en curso que aún no ocupan cupo por
    /// un mensaje.
    pub reserved: i64,
    /// De los que ocupan cupo, los que recibieron una notificación de
    /// seguimiento en las últimas 24 h.
    pub tracking_used: i64,
    /// Reservados por corridas de seguimiento que aún no cuentan en
    /// tracking_used.
    pub tracking_reserved: i64,
}

impl QuotaTotals {
    fn global_free(&self, limits: QuotaLimits) -> i64 {
        (limits.limit - self.used - self.reserved).max(0)
    }

    fn tracking_free(&self, limits: QuotaLimits) -> i64 {
        (limits.tracking_limit - self.tracking_used - self.tracking_reserved).max(0)
    }

    /// Clientes nuevos que todavía caben para este tipo de envío.
    pub fn available(&self, kind: QuotaKind, limits: QuotaLimits) -> i64 {
        match kind {
            QuotaKind::Office => self.global_free(limits),
            QuotaKind::Tracking => self.global_free(limits).min(self.tracking_free(limits)),
        }
    }
}

/// Situación de un teléfono pedido.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[derive(sqlx::FromRow)]
pub struct PhoneQuotaState {
    pub phone: String,
    /// Nos escribió hace menos de 24 h y su ventana sigue abierta hasta
    /// que vence la reserva: escribirle no consume cupo de Meta.
    pub in_window: bool,
    /// Ya ocupa cupo (le escribimos en las últimas 24 h).
    pub reached: bool,
    /// Otra corrida en curso lo tiene reservado.
    #[sqlx(rename = "reserved_elsewhere")]
    pub reserved: bool,
    /// Ya cuenta en el tope del seguimiento (notificación de seguimiento en
    /// las últimas 24 h, o reservado por otra corrida de seguimiento).
    pub tracking_counted: bool,
}

/// Qué se aprueba de una solicitud.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allocation {
    pub approved: Vec<String>,
    pub denied: Vec<String>,
    /// Aprobados que quedan en la reserva: ocupan cupo mientras dure el
    /// envío (hasta que su mensaje quede registrado).
    pub reserve: Vec<String>,
    /// Teléfonos pedidos que ocupan un cupo nuevo (aprobados o no).
    pub new_clients: i64,
    /// Cupo libre para este tipo antes de la solicitud.
    pub available: i64,
}

/// Aprueba, en el orden pedido, los teléfonos que no consumen cupo y los
/// nuevos mientras quepan. Un teléfono de seguimiento debe caber en el cupo
/// del número y en el tope del seguimiento.
pub fn allocate(kind: QuotaKind, limits: QuotaLimits, totals: QuotaTotals, phones: &[PhoneQuotaState]) -> Allocation {
    let mut global_free = totals.global_free(limits);
    let mut tracking_free = totals.tracking_free(limits);
    let mut allocation = Allocation { available: totals.available(kind, limits), ..Default::default() };

    for state in phones {
        if state.in_window {
            allocation.approved.push(state.phone.clone());
            continue;
        }

        let needs_global = !state.reached && !state.reserved;
        let needs_tracking = kind == QuotaKind::Tracking && !state.tracking_counted;
        if needs_global {
            allocation.new_clients += 1;
        }

        let fits = (!needs_global || global_free > 0) && (!needs_tracking || tracking_free > 0);
        if !fits {
            allocation.denied.push(state.phone.clone());
            continue;
        }

        if needs_global {
            global_free -= 1;
        }
        if needs_tracking {
            tracking_free -= 1;
        }
        allocation.approved.push(state.phone.clone());
        // Uno que ya ocupa cupo no hace falta reservarlo, salvo para contarlo
        // en el tope del seguimiento. Uno reservado por otra corrida sí: si
        // esa termina primero, sigue contando mientras dure esta.
        if !state.reached || needs_tracking {
            allocation.reserve.push(state.phone.clone());
        }
    }

    allocation
}

/// Respuesta de POST /quota/reservations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaDecision {
    /// Todos los teléfonos pedidos fueron aprobados.
    pub approved: bool,
    pub kind: QuotaKind,
    /// En el orden pedido. Enviar solo a estos.
    pub approved_phones: Vec<String>,
    /// Sin cupo: no enviar (la próxima corrida los vuelve a pedir).
    pub denied_phones: Vec<String>,
    /// Pedidos que ocupan un cupo nuevo.
    pub new_clients: i64,
    /// Cupo libre para este tipo antes de la solicitud.
    pub available: i64,
    pub limit: i64,
    pub tracking_limit: i64,
    pub used: i64,
    pub reserved: i64,
    pub tracking_used: i64,
    pub tracking_reserved: i64,
    /// Reserva creada (si algún aprobado ocupa cupo): liberarla al terminar.
    pub reservation_id: Option<i32>,
    pub expires_at: Option<NaiveDateTime>,
}

/// Cupo actual (GET /quota), sin reservar nada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaStatus {
    pub limit: i64,
    pub tracking_limit: i64,
    pub used: i64,
    pub reserved: i64,
    pub tracking_used: i64,
    pub tracking_reserved: i64,
    pub available_office: i64,
    pub available_tracking: i64,
}

impl QuotaStatus {
    pub fn new(limits: QuotaLimits, totals: QuotaTotals) -> Self {
        Self {
            limit: limits.limit,
            tracking_limit: limits.tracking_limit,
            used: totals.used,
            reserved: totals.reserved,
            tracking_used: totals.tracking_used,
            tracking_reserved: totals.tracking_reserved,
            available_office: totals.available(QuotaKind::Office, limits),
            available_tracking: totals.available(QuotaKind::Tracking, limits),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseOutcome {
    Released,
    /// Ya estaba liberada: liberar dos veces no hace nada.
    AlreadyReleased,
    NotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: QuotaLimits = QuotaLimits { limit: 10, tracking_limit: 4 };

    fn new_phone(phone: &str) -> PhoneQuotaState {
        PhoneQuotaState { phone: phone.into(), ..Default::default() }
    }

    fn totals(used: i64, reserved: i64, tracking_used: i64, tracking_reserved: i64) -> QuotaTotals {
        QuotaTotals { used, reserved, tracking_used, tracking_reserved }
    }

    #[test]
    fn office_approves_new_phones_in_order_while_they_fit() {
        let phones: Vec<_> = ["1", "2", "3", "4"].map(new_phone).into();
        let allocation = allocate(QuotaKind::Office, LIMITS, totals(6, 1, 0, 0), &phones);

        assert_eq!(allocation.available, 3);
        assert_eq!(allocation.approved, ["1", "2", "3"]);
        assert_eq!(allocation.denied, ["4"]);
        assert_eq!(allocation.reserve, ["1", "2", "3"]);
        assert_eq!(allocation.new_clients, 4);
    }

    #[test]
    fn phones_that_do_not_consume_quota_are_always_approved() {
        let phones = vec![
            PhoneQuotaState { in_window: true, ..new_phone("ventana") },
            PhoneQuotaState { reached: true, ..new_phone("ya-contactado") },
            PhoneQuotaState { reserved: true, ..new_phone("de-otra-corrida") },
            new_phone("nuevo"),
        ];
        let allocation = allocate(QuotaKind::Office, LIMITS, totals(10, 0, 0, 0), &phones);

        assert_eq!(allocation.approved, ["ventana", "ya-contactado", "de-otra-corrida"]);
        assert_eq!(allocation.denied, ["nuevo"]);
        // El de otra corrida se reserva también: sigue contando si esa
        // termina primero. Los demás no ocupan cupo nuevo.
        assert_eq!(allocation.reserve, ["de-otra-corrida"]);
        assert_eq!(allocation.new_clients, 1);
    }

    #[test]
    fn tracking_is_capped_even_with_free_quota() {
        let phones: Vec<_> = ["1", "2", "3"].map(new_phone).into();
        let allocation = allocate(QuotaKind::Tracking, LIMITS, totals(2, 0, 2, 0), &phones);

        assert_eq!(allocation.available, 2); // 8 libres, pero el tope deja 2
        assert_eq!(allocation.approved, ["1", "2"]);
        assert_eq!(allocation.denied, ["3"]);
    }

    #[test]
    fn office_can_use_what_tracking_cannot() {
        let phones: Vec<_> = ["1", "2", "3"].map(new_phone).into();
        let allocation = allocate(QuotaKind::Office, LIMITS, totals(2, 0, 4, 0), &phones);
        assert_eq!(allocation.approved, ["1", "2", "3"]);
    }

    #[test]
    fn tracking_to_a_client_already_reached_by_office_counts_only_in_its_cap() {
        let phones = vec![PhoneQuotaState { reached: true, ..new_phone("1") }, new_phone("2")];
        let allocation = allocate(QuotaKind::Tracking, LIMITS, totals(9, 0, 3, 0), &phones);

        // "1" no ocupa cupo nuevo del número, pero sí el último del tope.
        assert_eq!(allocation.approved, ["1"]);
        assert_eq!(allocation.denied, ["2"]);
        assert_eq!(allocation.reserve, ["1"]);
    }

    #[test]
    fn already_counted_tracking_phone_does_not_use_the_cap_again() {
        let phones = vec![PhoneQuotaState { reached: true, tracking_counted: true, ..new_phone("1") }];
        let allocation = allocate(QuotaKind::Tracking, LIMITS, totals(10, 0, 4, 0), &phones);
        assert_eq!(allocation.approved, ["1"]);
        assert!(allocation.reserve.is_empty());
    }

    #[test]
    fn availability_is_never_negative() {
        assert_eq!(totals(12, 3, 0, 0).available(QuotaKind::Office, LIMITS), 0);
        assert_eq!(totals(0, 0, 5, 1).available(QuotaKind::Tracking, LIMITS), 0);
    }

    #[test]
    fn reserve_validation_and_normalization() {
        let dto = |phones: Vec<&str>, ttl_minutes| ReserveQuotaDto {
            phones: phones.into_iter().map(String::from).collect(),
            kind: QuotaKind::Office,
            business_id: Some(1),
            ttl_minutes,
        };
        assert!(dto(vec!["573001112233", "+50251319585", "488760023"], None).validate().is_ok());
        assert!(dto(vec![], None).validate().is_err());
        assert!(dto(vec!["1234567"], None).validate().is_err());
        assert!(dto(vec!["573001112233"], Some(0)).validate().is_err());
        assert!(dto(vec!["573001112233"; 1001], None).validate().is_err());

        let request = dto(vec!["+573001112233", "573001112233", " 50251319585 ", "573001112233"], None);
        assert_eq!(request.normalized_phones(), ["573001112233", "50251319585"]);
    }

    #[test]
    fn kind_comes_in_lowercase() {
        let dto: ReserveQuotaDto = serde_json::from_str(r#"{"phones": ["573001112233"], "kind": "tracking"}"#).unwrap();
        assert_eq!(dto.kind, QuotaKind::Tracking);
        assert!(serde_json::from_str::<ReserveQuotaDto>(r#"{"phones": ["573001112233"], "kind": "otro"}"#).is_err());
    }
}
