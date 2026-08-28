from app.models.shipping import RecipientInfo
from app.whatsapp.templates.base import TemplateMessage


class RecordatorioFinalTemplate(TemplateMessage):
    """Plantilla de advertencia de que el pedido puede que sea devuelto.
    """

    TEMPLATE_NAME = "recordatorio_final" # Nombre registrado en Meta Business

    # media_id no se usa en esta plantilla (no adjunta documento), pero se
    # recibe para mantener la misma firma de construcción que las demás.
    def __init__(self, recipient: RecipientInfo, media_id: str = "") -> None:
        self._recipient = recipient

    @property
    def name(self) -> str:
        return self.TEMPLATE_NAME

    def log_message(self) -> str | None:
        # Texto completo de la plantilla tal como la renderiza Meta.
        # Si la plantilla cambia en Meta Business, actualizar aquí.
        return (
            "Reclama tu pedido\n"
            f"Hola {self._recipient.name}, ¡tu {self._recipient.product}\n"
            "está a punto de ser devuelto!\n"
            "\n"
            "Te notificamos que el plazo máximo para reclamar tu \n"
            "paquete en la oficina de la transportadora está llegando \n"
            "a su fin. \n"
            "\n"
            "Por favor pasa a recogerlo a la brevedad posible para que \n"
            "no sea retornado a nuestro almacén.\n"
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
                {"type": "text", "text": self._recipient.product}
            ],
        }
