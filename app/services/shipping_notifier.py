from typing import Callable

from app.core.interfaces import (
    MediaRepository,
    MessageSender,
    RecipientInfoExtractor,
    TextExtractor,
)
from app.models.shipping import RecipientInfo
from app.whatsapp.templates.base import TemplateMessage
from app.whatsapp.templates.pedido_enviado import PedidoEnviadoTemplate


class ShippingNotificationService:
    """Orquesta el flujo de negocio de notificación de guías de envío:

    descargar el PDF de la guía -> extraer su texto -> extraer los datos
    del destinatario -> notificar por WhatsApp con la plantilla.
    """

    def __init__(
        self,
        media_repository: MediaRepository,
        message_sender: MessageSender,
        pdf_extractor: TextExtractor,
        recipient_extractor: RecipientInfoExtractor,
        debug_notification_number: str | None = None,
        template_factory: Callable[[RecipientInfo, str], TemplateMessage] = PedidoEnviadoTemplate,
    ) -> None:
        self._media_repository = media_repository
        self._message_sender = message_sender
        self._pdf_extractor = pdf_extractor
        self._recipient_extractor = recipient_extractor
        self._debug_notification_number = debug_notification_number
        self._template_factory = template_factory

    def notify_pdf_guide(self, media_id: str) -> bool:
        """Procesa una guía en PDF y notifica al destinatario.

        Retorna True si la notificación fue enviada con éxito.
        """
        media_url = self._media_repository.get_media_url(media_id)
        if not media_url:
            return False

        pdf_bytes = self._media_repository.download_media(media_url)
        if not pdf_bytes:
            return False

        pdf_text = self._pdf_extractor.extract_text(pdf_bytes)
        if not pdf_text.strip():
            print("No text extracted from PDF")
            return False

        recipient = self._recipient_extractor.extract(pdf_text)
        if recipient is None:
            return False

        if not recipient.is_complete:
            print("Could not extract all necesary information from the PDF.")
            return False

        print(
            f"Extracted info - Name: {recipient.name}, Phone: {recipient.phone}, "
            f"Guía: {recipient.tracking_number}, Dirección: {recipient.delivery_address}"
            f"Producto: {recipient.product}"
        )
        return self._notify_recipient(recipient, media_id)

    def _notify_recipient(self, recipient: RecipientInfo, media_id: str) -> bool:
        template = self._template_factory(recipient, media_id)

        #sent = self._message_sender.send_template(recipient.phone, template) Decomentar para produccion

        # Copia de depuración a un número interno, si está configurado.
        debug_number = self._debug_notification_number
        if debug_number and debug_number != recipient.phone:
            self._message_sender.send_template(debug_number, template)

        return sent
