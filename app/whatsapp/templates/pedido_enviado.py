from app.models.shipping import RecipientInfo
from app.whatsapp.templates.base import TemplateMessage


class PedidoEnviadoTemplate(TemplateMessage):
    """Plantilla de notificación de pedido enviado.

    Adjunta la guía de envío como documento en el header y envía
    nombre, número de guía y dirección en el cuerpo del mensaje.
    """

    TEMPLATE_NAME = "pedido_eviado" # Nombre registrado en Meta Business
    DOCUMENT_FILENAME = "Guia_de_envio.pdf"

    def __init__(self, recipient: RecipientInfo, media_id: str) -> None:
        self._recipient = recipient
        self._media_id = media_id

    @property
    def name(self) -> str:
        return self.TEMPLATE_NAME

    def build_components(self) -> list[dict]:
        return [
            self._build_header(),
            self._build_body(),
        ]

    def _build_header(self) -> dict:
        return {
            "type": "header",
            "parameters": [
                {
                    "type": "document",
                    "document": {
                        "id": self._media_id,
                        "filename": self.DOCUMENT_FILENAME,
                    },
                }
            ],
        }

    def _build_body(self) -> dict:
        return {
            "type": "body",
            "parameters": [
                {"type": "text", "text": self._recipient.name},
                {"type": "text", "text": self._recipient.tracking_number},
                {"type": "text", "text": self._recipient.delivery_address},
            ],
        }
