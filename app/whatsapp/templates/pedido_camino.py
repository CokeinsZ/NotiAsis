from app.models.shipping import RecipientInfo
from app.whatsapp.templates.base import TemplateMessage


class PedidoCaminoTemplate(TemplateMessage):
    """Seguimiento de pedidos de Dropi, nivel 2: la transportadora ya está
    llevando el pedido a su destino.

    Variables del body: nombre, producto, número de guía y dirección
    completa de entrega.
    """

    TEMPLATE_NAME = "pedido_camino_1"  # Nombre registrado en Meta Business

    def __init__(self, recipient: RecipientInfo) -> None:
        self._recipient = recipient

    @property
    def name(self) -> str:
        return self.TEMPLATE_NAME

    def log_message(self) -> str | None:
        # Texto completo de la plantilla tal como la renderiza Meta.
        # Si la plantilla cambia en Meta Business, actualizar aquí.
        return (
            "Tu pedido esta en camino.\n"
            f"Hola {self._recipient.name}.\n"
            "\n"
            f"Tu pedido con el producto {self._recipient.product} y número de guia "
            f"{self._recipient.tracking_number} , ya está siendo transportado a "
            f"{self._recipient.delivery_address}.\n"
            "\n"
            "Puedes usar tu número de guía para hacerle seguimiento en la página oficial "
            "de la transportadora. ¡Pronto estará en tus manos!"
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
                ],
            }
        ]
