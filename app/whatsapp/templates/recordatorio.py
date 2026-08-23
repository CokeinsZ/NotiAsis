from app.models.shipping import RecipientInfo
from app.whatsapp.templates.base import TemplateMessage


class RecordatorioTemplate(TemplateMessage):
    """Plantilla de recordatorio de recogida de pedido.

    Se envía a usuarios ya notificados anteriormente que aún no han
    reclamado su pedido. No adjunta documento ni usa variables.
    """

    TEMPLATE_NAME = "recordatorio"  # Nombre registrado en Meta Business

    # media_id no se usa en esta plantilla, pero se recibe para mantener
    # la misma firma de construcción que las demás.
    def __init__(self, recipient: RecipientInfo, media_id: str = "") -> None:
        self._recipient = recipient

    @property
    def name(self) -> str:
        return self.TEMPLATE_NAME

    def build_components(self) -> list[dict]:
        return []

    def log_message(self) -> str | None:
        # Texto completo de la plantilla tal como la renderiza Meta.
        # Si la plantilla cambia en Meta Business, actualizar aquí.
        return (
            "Tu pedido te espera\n"
            "Hola\n"
            "\n"
            "Te escribimos para recordarte que tu pedido "
            "ya esta listo para recoger en las oficinas en tu ciudad de la transportadora. \n\n"
            "¡Que disfrutes tu compra!"
        )
