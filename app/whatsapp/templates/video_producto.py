from app.whatsapp.templates.base import TemplateMessage


class VideoProductoTemplate(TemplateMessage):
    """Plantilla con el video del producto en el header (sin variables
    de texto). Va una sola vez por guía: antes de pedido_enviado_2 (nivel 1
    del seguimiento) o, si no la recibió ahí, antes del mensaje_guia en el
    aviso inicial en oficina.
    """

    TEMPLATE_NAME = "video_producto"  # Nombre registrado en Meta Business

    def __init__(self, media_id: str) -> None:
        self._media_id = media_id

    @property
    def name(self) -> str:
        return self.TEMPLATE_NAME

    def media_type(self) -> str:
        return "video"

    def log_media_id(self) -> str | None:
        return self._media_id

    def build_components(self) -> list[dict]:
        return [
            {
                "type": "header",
                "parameters": [{"type": "video", "video": {"id": self._media_id}}],
            }
        ]
