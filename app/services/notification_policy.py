"""Política de escalación de notificaciones de guías.

Una guía se notifica como máximo MAX_GUIDE_NOTIFICATIONS veces y NUNCA
más de una vez por día:

| notification_count actual | Paso al recibirla de nuevo           |
|---------------------------|--------------------------------------|
| 0 (nunca notificada)      | INITIAL (guia + mensaje_guia)        |
| 1                         | REMINDER (recordatorio)              |
| 2                         | FINAL_REMINDER (recordatorio_final)  |
| >= 3                      | STOP (máximo alcanzado)              |
| cualquiera, notificada hoy| STOP (ya se notificó en el día)      |

Es una función pura compartida por el flujo del webhook (PDF por
WhatsApp) y el del Google Sheet (notificación masiva). Las fechas se
inyectan para que sea testeable.
"""

from datetime import date, datetime
from enum import Enum

MAX_GUIDE_NOTIFICATIONS = 3


class NotificationStep(Enum):
    INITIAL = "initial"
    REMINDER = "reminder"
    FINAL_REMINDER = "final_reminder"
    STOP = "stop"


def step_for_notification_count(
    notification_count: int,
    last_notified_at: str | datetime | None = None,
    today: date | None = None,
) -> NotificationStep:
    """Decide qué hacer según cuántas veces se ha notificado la guía.

    Regla dura: una guía nunca recibe más de una notificación por día
    (así evitamos que una guía duplicada escale de recordatorio a
    recordatorio_final en la misma corrida/día).
    """
    if today is None:
        today = date.today()

    last_notified_date = _as_date(last_notified_at)
    if last_notified_date is not None and last_notified_date >= today:
        return NotificationStep.STOP

    if notification_count <= 0:
        return NotificationStep.INITIAL
    if notification_count == 1:
        return NotificationStep.REMINDER
    if notification_count < MAX_GUIDE_NOTIFICATIONS:
        return NotificationStep.FINAL_REMINDER
    return NotificationStep.STOP


def _as_date(value: str | datetime | None) -> date | None:
    """Acepta datetime, o string ISO tipo '2026-08-22T16:31:47' (como
    viene del backend), y lo convierte a date."""
    if value is None:
        return None
    if isinstance(value, datetime):
        return value.date()
    if isinstance(value, str) and value.strip():
        try:
            return datetime.fromisoformat(value).date()
        except ValueError:
            return None
    return None
