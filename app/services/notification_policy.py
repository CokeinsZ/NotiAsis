"""Política de escalación de notificaciones de guías.

Una guía se notifica como máximo MAX_GUIDE_NOTIFICATIONS veces:

| notification_count actual | Paso al recibirla de nuevo        |
|---------------------------|-----------------------------------|
| 0 (nunca notificada)      | INITIAL (guia + mensaje_guia)     |
| 1                         | REMINDER (recordatorio)           |
| 2                         | FINAL_REMINDER (recordatorio_final)|
| >= 3                      | STOP (no hacer nada)              |

Es una función pura compartida por el flujo del webhook (PDF por
WhatsApp) y el del Google Sheet (notificación masiva).
"""

from enum import Enum

MAX_GUIDE_NOTIFICATIONS = 3


class NotificationStep(Enum):
    INITIAL = "initial"
    REMINDER = "reminder"
    FINAL_REMINDER = "final_reminder"
    STOP = "stop"


def step_for_notification_count(notification_count: int) -> NotificationStep:
    """Decide qué hacer según cuántas veces se ha notificado la guía."""
    if notification_count <= 0:
        return NotificationStep.INITIAL
    if notification_count == 1:
        return NotificationStep.REMINDER
    if notification_count < MAX_GUIDE_NOTIFICATIONS:
        return NotificationStep.FINAL_REMINDER
    return NotificationStep.STOP
