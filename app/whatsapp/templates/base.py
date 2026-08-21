from abc import ABC, abstractmethod


class TemplateMessage(ABC):
    """Contrato base para las plantillas de la WhatsApp Cloud API.

    Cada plantilla registrada en Meta Business tiene su propia clase en
    este paquete. Para agregar una nueva plantilla:

    1. Crea un archivo nuevo en `app/whatsapp/templates/`.
    2. Define una clase que herede de `TemplateMessage`.
    3. Implementa `name` y `build_components()`.
    4. Envíala con `WhatsAppClient.send_template(numero, MiPlantilla(...))`.
    """

    language_code: str = "es_CO"

    @property
    @abstractmethod
    def name(self) -> str:
        """Nombre de la plantilla tal como está registrado en Meta Business."""

    @abstractmethod
    def build_components(self) -> list[dict]:
        """Componentes de la plantilla (header, body, buttons, etc.)."""

    def build_payload(self, to_number: str) -> dict:
        """Construye el payload completo para el endpoint /messages de Meta."""
        return {
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "to": to_number,
            "type": "template",
            "template": {
                "name": self.name,
                "language": {"code": self.language_code},
                "components": self.build_components(),
            },
        }

    # --- Metadatos para el historial del chat en el backend ---

    def media_type(self) -> str:
        """Tipo de medio con el que se registra el mensaje (text/document/...)."""
        return "text"

    def log_message(self) -> str | None:
        """Texto representativo del mensaje para el historial del chat."""
        return None

    def log_media_id(self) -> str | None:
        """Media id adjunto, si la plantilla incluye documento/imagen."""
        return None
