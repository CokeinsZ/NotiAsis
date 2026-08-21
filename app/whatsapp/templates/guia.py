from app.models.shipping import RecipientInfo
from app.whatsapp.templates.base import TemplateMessage


class GuiaTemplate(TemplateMessage):
    """Plantilla de envio del pdf de la guia.

    Adjunta solo la guía de envío como documento en el header.
    """

    TEMPLATE_NAME = "guia" # Nombre registrado en Meta Business
    DOCUMENT_FILENAME = "Guia_de_envio.pdf"

    def __init__(self, recipient: RecipientInfo, media_id: str) -> None:
        self._recipient = recipient
        self._media_id = media_id

    @property
    def name(self) -> str:
        return self.TEMPLATE_NAME

    def media_type(self) -> str:
        return "document"

    def log_media_id(self) -> str | None:
        return self._media_id

    def build_components(self) -> list[dict]:
        return [
            self._build_header(),
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