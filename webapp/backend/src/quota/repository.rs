use async_trait::async_trait;
use chrono::NaiveDateTime;

use crate::quota::dtos::{
    PhoneQuotaState, QuotaDecision, QuotaKind, QuotaLimits, QuotaTotals, ReleaseOutcome, allocate,
};

#[async_trait]
pub trait QuotaRepositoryTrait: Send + Sync {
    /// Uso actual del cupo (sin bloquear ni reservar).
    async fn usage(&self) -> Result<QuotaTotals, String>;
    /// Calcula la situación de cada teléfono, decide cuáles caben
    /// (`allocate`) y los reserva por `ttl_minutes`. Todo pasa en una
    /// transacción con un bloqueo global: dos solicitudes simultáneas nunca
    /// ven el mismo cupo libre. `phones` ya viene normalizado y sin repetidos.
    async fn reserve(
        &self,
        limits: QuotaLimits,
        kind: QuotaKind,
        phones: &[String],
        ttl_minutes: i32,
        business_id: Option<i32>,
    ) -> Result<QuotaDecision, String>;
    /// Libera una reserva (el envío terminó). Idempotente.
    async fn release(&self, reservation_id: i32) -> Result<ReleaseOutcome, String>;
}

pub struct PostgresQuotaRepository {
    db_pool: sqlx::PgPool,
}

impl PostgresQuotaRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { db_pool: pool }
    }
}

/// Bloqueo global del cupo, tomado dentro de la transacción de cada
/// reserva y liberado al terminarla (commit o rollback, incluso si la
/// conexión se cae). Usa la forma de dos claves de 32 bits: no se cruza
/// con los bloqueos por mensaje (`tools::message_lock`, una clave de 64).
const QUOTA_LOCK: &str = "SELECT pg_advisory_xact_lock(hashtext('notiasis'), hashtext('messaging_quota'))";

/// Si otra reserva tuviera el bloqueo demasiado tiempo, se falla en vez de
/// esperar indefinidamente ocupando una conexión del pool.
const LOCK_TIMEOUT: &str = "SET LOCAL lock_timeout = '10s'";

/// Error cuando el bloqueo no se obtuvo a tiempo (el envío no se aprueba;
/// el bot puede reintentar).
pub const QUOTA_BUSY: &str = "Quota busy, retry later";

fn lock_error(error: sqlx::Error) -> String {
    match &error {
        // 55P03 lock_not_available: venció lock_timeout.
        sqlx::Error::Database(db) if db.code().as_deref() == Some("55P03") => QUOTA_BUSY.to_string(),
        _ => error.to_string(),
    }
}

/// Conjuntos con los que se calcula el cupo, todos con la hora de la base
/// en UTC (como messages.created_at), no la del servidor:
///
/// - reached: clientes que ocupan cupo: con un mensaje saliente en las
///   últimas 24 h que no falló y que se envió fuera de su ventana de
///   atención (el cliente no nos había escrito en las 24 h anteriores a ese
///   mensaje). La ventana es del número de WhatsApp: cuenta un mensaje del
///   cliente a cualquier business.
/// - tracking_reached: de esos, los que recibieron una notificación de
///   seguimiento (guide_notifications kind 'tracking') en las últimas 24 h.
/// - active: teléfonos de reservas sin liberar y sin vencer.
/// - totals: los cuatro números del cupo. Un teléfono reservado que ya
///   ocupa cupo por un mensaje cuenta una sola vez.
macro_rules! quota_ctes {
    () => {
        r#"
    WITH now_utc AS (
        SELECT clock_timestamp() AT TIME ZONE 'UTC' AS t
    ),
    reached AS (
        SELECT DISTINCT m.user_id
        FROM messages m CROSS JOIN now_utc
        WHERE NOT m.from_user
          AND m.status IS DISTINCT FROM 'failed'
          AND m.created_at > now_utc.t - INTERVAL '24 hours'
          AND NOT EXISTS (
              SELECT 1 FROM messages incoming
              WHERE incoming.from_user
                AND incoming.user_id = m.user_id
                AND incoming.created_at <= m.created_at
                AND incoming.created_at > m.created_at - INTERVAL '24 hours'
          )
    ),
    tracking_reached AS (
        SELECT DISTINCT n.user_id
        FROM guide_notifications n CROSS JOIN now_utc
        WHERE n.kind = 'tracking'
          AND n.created_at > now_utc.t - INTERVAL '24 hours'
          AND n.user_id IN (SELECT user_id FROM reached)
    ),
    active AS (
        SELECT DISTINCT p.user_id, r.kind
        FROM message_quota_reservations r
        JOIN message_quota_reservation_phones p ON p.reservation_id = r.id
        CROSS JOIN now_utc
        WHERE r.released_at IS NULL AND r.expires_at > now_utc.t
    ),
    totals AS (
        SELECT
            (SELECT COUNT(*) FROM reached) AS used,
            (SELECT COUNT(DISTINCT a.user_id) FROM active a
             WHERE a.user_id NOT IN (SELECT user_id FROM reached)) AS reserved,
            (SELECT COUNT(*) FROM tracking_reached) AS tracking_used,
            (SELECT COUNT(DISTINCT a.user_id) FROM active a
             WHERE a.kind = 'tracking' AND a.user_id NOT IN (SELECT user_id FROM tracking_reached)) AS tracking_reserved
    )
"#
    };
}

/// Cupo y situación de cada teléfono pedido en UNA sola sentencia: todo
/// sale de la misma foto de la base. $1 teléfonos (en orden), $2 minutos
/// de la reserva: la ventana del cliente debe seguir abierta hasta que
/// venza, o el envío podría salir ya con la ventana cerrada.
const PHONES: &str = concat!(
    quota_ctes!(),
    r#"
        SELECT
            q.phone,
            EXISTS (
                SELECT 1 FROM messages incoming CROSS JOIN now_utc
                WHERE incoming.from_user
                  AND incoming.user_id = q.phone
                  AND incoming.created_at > now_utc.t - INTERVAL '24 hours' + make_interval(mins => $2)
            ) AS in_window,
            q.phone IN (SELECT user_id FROM reached) AS reached,
            q.phone IN (SELECT user_id FROM active) AS reserved_elsewhere,
            (q.phone IN (SELECT user_id FROM tracking_reached)
             OR q.phone IN (SELECT user_id FROM active WHERE kind = 'tracking')) AS tracking_counted,
            totals.used, totals.reserved, totals.tracking_used, totals.tracking_reserved
        FROM unnest($1::text[]) WITH ORDINALITY AS q(phone, ord)
        CROSS JOIN totals
        ORDER BY q.ord
        "#
);

const USAGE: &str = concat!(quota_ctes!(), "SELECT used, reserved, tracking_used, tracking_reserved FROM totals");

/// La reserva y sus teléfonos en una sola sentencia.
const INSERT_RESERVATION: &str = r#"
    WITH now_utc AS (
        SELECT clock_timestamp() AT TIME ZONE 'UTC' AS t
    ),
    reservation AS (
        INSERT INTO message_quota_reservations (kind, business_id, created_at, expires_at)
        SELECT $1, $2, now_utc.t, now_utc.t + make_interval(mins => $3)
        FROM now_utc
        RETURNING id, expires_at
    ),
    phones AS (
        INSERT INTO message_quota_reservation_phones (reservation_id, user_id)
        SELECT reservation.id, phone
        FROM reservation CROSS JOIN unnest($4::text[]) AS phone
    )
    SELECT id, expires_at FROM reservation
"#;

/// En una sola sentencia: bloquea la fila, la libera si estaba vigente y
/// dice si existía. Una liberación concurrente espera el bloqueo de la fila
/// y la ve ya liberada. No toma el bloqueo global: liberar solo agranda el
/// cupo, y una reserva simultánea que no lo vea queda del lado seguro.
const RELEASE: &str = r#"
    WITH target AS (
        SELECT id, released_at FROM message_quota_reservations
        WHERE id = $1
        FOR UPDATE
    ),
    released AS (
        UPDATE message_quota_reservations r
        SET released_at = clock_timestamp() AT TIME ZONE 'UTC'
        FROM target
        WHERE r.id = target.id AND target.released_at IS NULL
        RETURNING r.id
    )
    SELECT EXISTS(SELECT 1 FROM target), EXISTS(SELECT 1 FROM released)
"#;

#[derive(sqlx::FromRow)]
struct PhoneQuotaRow {
    #[sqlx(flatten)]
    state: PhoneQuotaState,
    #[sqlx(flatten)]
    totals: QuotaTotals,
}

#[async_trait]
impl QuotaRepositoryTrait for PostgresQuotaRepository {
    async fn usage(&self) -> Result<QuotaTotals, String> {
        sqlx::query_as(USAGE)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| e.to_string())
    }

    async fn reserve(
        &self,
        limits: QuotaLimits,
        kind: QuotaKind,
        phones: &[String],
        ttl_minutes: i32,
        business_id: Option<i32>,
    ) -> Result<QuotaDecision, String> {
        // Si algo falla antes del commit, el drop de `tx` hace rollback y
        // suelta el bloqueo: nunca queda una reserva a medias.
        let mut tx = self.db_pool.begin().await.map_err(|e| e.to_string())?;

        sqlx::query(LOCK_TIMEOUT).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        sqlx::query(QUOTA_LOCK).execute(&mut *tx).await.map_err(lock_error)?;

        // Con el bloqueo tomado, la reserva anterior ya hizo commit: esta
        // consulta (READ COMMITTED, foto nueva por sentencia) la incluye.
        let rows: Vec<PhoneQuotaRow> = sqlx::query_as(PHONES)
            .bind(phones)
            .bind(ttl_minutes)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        let totals = rows.first().map(|row| row.totals).unwrap_or_default();
        let states: Vec<PhoneQuotaState> = rows.into_iter().map(|row| row.state).collect();
        let allocation = allocate(kind, limits, totals, &states);

        let (reservation_id, expires_at) = if allocation.reserve.is_empty() {
            (None, None)
        } else {
            let (id, expires_at): (i32, NaiveDateTime) = sqlx::query_as(INSERT_RESERVATION)
                .bind(kind.as_str())
                .bind(business_id)
                .bind(ttl_minutes)
                .bind(&allocation.reserve)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            (Some(id), Some(expires_at))
        };

        tx.commit().await.map_err(|e| e.to_string())?;

        Ok(QuotaDecision {
            approved: allocation.denied.is_empty(),
            kind,
            approved_phones: allocation.approved,
            denied_phones: allocation.denied,
            new_clients: allocation.new_clients,
            available: allocation.available,
            limit: limits.limit,
            tracking_limit: limits.tracking_limit,
            used: totals.used,
            reserved: totals.reserved,
            tracking_used: totals.tracking_used,
            tracking_reserved: totals.tracking_reserved,
            reservation_id,
            expires_at,
        })
    }

    async fn release(&self, reservation_id: i32) -> Result<ReleaseOutcome, String> {
        let (exists, released): (bool, bool) = sqlx::query_as(RELEASE)
            .bind(reservation_id)
            .fetch_one(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(match (exists, released) {
            (false, _) => ReleaseOutcome::NotFound,
            (true, true) => ReleaseOutcome::Released,
            (true, false) => ReleaseOutcome::AlreadyReleased,
        })
    }
}
