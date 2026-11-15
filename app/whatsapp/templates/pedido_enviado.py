from app.models.shipping import RecipientInfo
from app.whatsapp.templates.base import TemplateMessage


class PedidoEnviadoTemplate(TemplateMessage):
    """Seguimiento de pedidos de Dropi, nivel 1: el pedido está listo para
    que la transportadora lo traslade.

    Variables del body: nombre, número de guía, producto, precio a pagar
    con el símbolo de su moneda ("$84.900", "Q229": la plantilla no lo
    incluye) y transportadora.
    """

    TEMPLATE_NAME = "pedido_enviado_2"  # Nombre registrado en Meta Business

    def __init__(self, recipient: RecipientInfo, price: str, carrier: str) -> None:
        self._recipient = recipient
        self._price = price
        self._carrier = carrier

    @property
    def name(self) -> str:
        return self.TEMPLATE_NAME

    def log_message(self) -> str | None:
        # Texto completo de la plantilla tal como la renderiza Meta.
        # Si la plantilla cambia en Meta Business, actualizar aquí.
        return (
            f"Hola {self._recipient.name}, ¡tu pedido ya está listo para envío! 📦\n"
            "\n"
            f"Guía: {self._recipient.tracking_number}\n"
            f"Producto: {self._recipient.product}\n"
            f"Precio: *{self._price}*\n"
            f"Transportadora: {self._carrier}\n"
            "\n"
            "La transportadora iniciará el traslado de tu paquete en los próximos días. "
            "¡Mantente atento a cualquier novedad con tu entrega!"
        )

    def build_components(self) -> list[dict]:
        return [
            {
                "type": "body",
                "parameters": [
                    {"type": "text", "text": self._recipient.name},
                    {"type": "text", "text": self._recipient.tracking_number},
                    {"type": "text", "text": self._recipient.product},
                    {"type": "text", "text": self._price},
                    {"type": "text", "text": self._carrier},
                ],
            }
        ]
