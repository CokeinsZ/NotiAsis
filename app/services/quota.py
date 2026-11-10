"""Cupo de mensajería de una corrida de notificaciones.

Meta limita cuántos clientes nuevos (únicos, en una ventana móvil de 24 h)
pueden recibir plantillas. El backend lleva la cuenta (POST
/quota/reservations): antes de enviar, cada corrida reserva los teléfonos
a los que piensa escribir y solo envía a los aprobados.
"""

from app.core.interfaces import NotificationBackend
from app.core.phones import normalize_phone

# Lo que dura la reserva si la corrida se cae sin liberarla. Una corrida de
# Dropi tarda ~4 s por pedido: 60 min alcanzan para unos 900.
RESERVATION_MINUTES = 60


class RunQuota:
    """Cupo de una corrida.

    1. `reserve(phones)`: una sola llamada con los teléfonos de la corrida
       en orden de prioridad (si no alcanza, se aprueban los primeros).
    2. `allows(phone)` antes de cada envío. A un teléfono sin cupo no se le
       envía ni se le marca nada: la próxima corrida lo vuelve a intentar.
       Un teléfono que no estaba en el plan (salió del detalle del pedido)
       se pide en ese momento.
    3. Al salir del `with` se liberan las reservas, para que el cupo no
       quede ocupado hasta que venzan.

    Si el backend no responde no se envía (`unavailable`): pasarse del
    límite hace que Meta rechace los mensajes y baja la calidad del número.
    """

    def __init__(
        self,
        backend: NotificationBackend,
        kind: str,
        business_id: int,
        ttl_minutes: int = RESERVATION_MINUTES,
    ) -> None:
        self._backend = backend
        self._kind = kind
        self._business_id = business_id
        self._ttl_minutes = ttl_minutes
        self._approved: set[str] = set()
        self._denied: set[str] = set()
        self._reservations: list[int] = []
        self.unavailable = False

    def __enter__(self) -> "RunQuota":
        return self

    def __exit__(self, *exc_info) -> bool:
        self.release()
        return False

    def reserve(self, phones: list[str]) -> None:
        pending: list[str] = []
        for phone in phones:
            normalized = normalize_phone(phone) if phone else ""
            if normalized and normalized not in pending and not self._decided(normalized):
                pending.append(normalized)
        if not pending:
            return

        result = self._backend.reserve_quota(pending, self._kind, self._business_id, self._ttl_minutes)
        if result is None:
            self.unavailable = True
            self._denied.update(pending)
            return

        if result.reservation_id is not None:
            self._reservations.append(result.reservation_id)
        self._approved.update(result.approved_phones)
        # Lo que el backend no aprobó (o no mencionó) no se envía.
        self._denied.update(phone for phone in pending if phone not in self._approved)

    def allows(self, phone: str) -> bool:
        normalized = normalize_phone(phone)
        if not self._decided(normalized):
            self.reserve([normalized])
        return normalized in self._approved

    def release(self) -> None:
        while self._reservations:
            self._backend.release_quota(self._reservations.pop())

    def _decided(self, phone: str) -> bool:
        return phone in self._approved or phone in self._denied
