from app.models.shipping import RecipientInfo
from app.whatsapp.templates.base import TemplateMessage


class MensajeGuiaTemplate(TemplateMessage):
    """Plantilla de informacion de guia de pedido enviado.

    envía nombre, número de guía, producto y dirección en el cuerpo del mensaje.
    """

    TEMPLATE_NAME = "mensaje_guia_es" # Nombre registrado en Meta Business

    # media_id no se usa en esta plantilla (no adjunta documento), pero se
    # recibe para mantener la misma firma de construcción que las demás.
    def __init__(self, recipient: RecipientInfo, media_id: str) -> None:
        self._recipient = recipient
        self._media_id = media_id

    @property
    def name(self) -> str:
        return self.TEMPLATE_NAME

    def log_message(self) -> str | None:
        return (
            f"Hola {self._recipient.name}, tu pedido ({self._recipient.product}) "
            f"va en camino. Guía: {self._recipient.tracking_number}. "
            f"Dirección: {self._recipient.delivery_address}"
        )

    def build_components(self) -> list[dict]:
        return [
            self._build_body(),
        ]

    def _build_body(self) -> dict:
        return {
            "type": "body",
            "parameters": [
                {"type": "text", "text": self._recipient.name},
                {"type": "text", "text": self._recipient.tracking_number},
                {"type": "text", "text": self._recipient.product},
                {"type": "text", "text": self._recipient.delivery_address},
            ],
        }
