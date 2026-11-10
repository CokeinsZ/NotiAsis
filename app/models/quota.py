from dataclasses import dataclass

# Tipos de envío del cupo de mensajería (ver POST /quota/reservations del
# backend). Oficina tiene prioridad: puede usar todo el cupo; el
# seguimiento solo hasta su tope (MESSAGING_TRACKING_LIMIT).
OFFICE = "office"
TRACKING = "tracking"


@dataclass(frozen=True)
class QuotaReservation:
    """Respuesta del backend a una solicitud de cupo."""

    # Enviar solo a estos (normalizados, en el orden pedido).
    approved_phones: tuple[str, ...]
    # Sin cupo: no enviar; la próxima corrida los vuelve a pedir.
    denied_phones: tuple[str, ...]
    # Reserva a liberar al terminar (None si ningún aprobado ocupaba cupo).
    reservation_id: int | None = None
