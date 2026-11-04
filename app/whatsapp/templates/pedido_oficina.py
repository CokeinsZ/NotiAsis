from app.models.shipping import RecipientInfo
from app.whatsapp.templates.base import TemplateMessage


class PedidoOficinaTemplate(TemplateMessage):
    """Plantilla de pedido en oficina (pedidos de Dropi).

    Variables del body: nombre, producto, número de guía, dirección
    completa de la oficina y valor a pagar (sin "$": la plantilla ya lo
    incluye).
    """

    TEMPLATE_NAME = "pedido_oficina"  # Nombre registrado en Meta Business

    def __init__(self, recipient: RecipientInfo, amount: str) -> None:
        self._recipient = recipient
        self._amount = amount

    @property
    def name(self) -> str:
        return self.TEMPLATE_NAME

    def log_message(self) -> str | None:
        # Texto completo de la plantilla tal como la renderiza Meta.
        # Si la plantilla cambia en Meta Business, actualizar aquí.
        return (
            f"Hola {self._recipient.name} !!😀\n"
            "\n"
            "Hablas con Veronica ¿Cómo estas?, estaba verificando y tu pedido de "
            f"{self._recipient.product} ya está en la oficina para ser reclamado.\n"
            "\n"
            f"El  número de guia es: {self._recipient.tracking_number}\n"
            f"En  la oficina de: {self._recipient.delivery_address}\n"
            f"Y debes pagar: *${self._amount}*\n"
            "\n"
            "¿Hoy lo puedes recoger? Quedo atenta \n"
            "Bendiciones"
        )

    def build_components(self) -> list[dict]:
        return [
            {
                "type": "body",
                "parameters": [
                    {"type": "text", "text": self._recipient.name},
                    {"type": "text", "text": self._recipient.product},
                    {"type": "text", "text": self._recipient.tracking_number},
                    {"type": "text", "text": self._recipient.delivery_address},
                    {"type": "text", "text": self._amount},
                ],
            }
        ]
