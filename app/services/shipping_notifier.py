from typing import Callable, Iterable

from app.core.interfaces import (
    MediaRepository,
    MessageSender,
    RecipientInfoExtractor,
    TextExtractor,
)
from app.models.shipping import RecipientInfo
from app.whatsapp.templates.base import TemplateMessage
from app.whatsapp.templates.guia import GuiaTemplate
from app.whatsapp.templates.mensaje_guia import MensajeGuiaTemplate

# Plantillas que se envían por cada guía recibida, en orden de envío:
# primero el PDF de la guía y luego el mensaje con la información extraída.
DEFAULT_TEMPLATE_FACTORIES: tuple = (GuiaTemplate, MensajeGuiaTemplate)


class ShippingNotificationService:
    """Orquesta el flujo de negocio de notificación de guías de envío:

    descargar el PDF de la guía -> extraer su texto -> extraer los datos
    del destinatario -> notificar por WhatsApp con las plantillas.
    """

    def __init__(
        self,
        media_repository: MediaRepository,
        message_sender: MessageSender,
        pdf_extractor: TextExtractor,
        recipient_extractor: RecipientInfoExtractor,
        debug_notification_number: str | None = None,
        notification_override_number: str | None = None,
        template_factories: Iterable[Callable[[RecipientInfo, str], TemplateMessage]] = DEFAULT_TEMPLATE_FACTORIES,
    ) -> None:
        self._media_repository = media_repository
        self._message_sender = message_sender
        self._pdf_extractor = pdf_extractor
        self._recipient_extractor = recipient_extractor
        self._debug_notification_number = debug_notification_number
        self._notification_override_number = notification_override_number
        self._template_factories = tuple(template_factories)

    def notify_pdf_guide(self, media_id: str) -> bool:
        """Procesa una guía en PDF y notifica al destinatario.

        Retorna True si todas las notificaciones fueron enviadas con éxito.
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
            print("Could not extract all necessary information from the PDF.")
            return False

        print(
            f"Extracted info - Name: {recipient.name}, Phone: {recipient.phone}, "
            f"Guía: {recipient.tracking_number}, Dirección: {recipient.delivery_address}, "
            f"Producto: {recipient.product}"
        )
        return self._notify_recipient(recipient, media_id)

    def _notify_recipient(self, recipient: RecipientInfo, media_id: str) -> bool:
        """Envía las plantillas configuradas, en orden, al destinatario."""
        templates = [factory(recipient, media_id) for factory in self._template_factories]

        # Mientras NOTIFICATION_OVERRIDE_NUMBER esté configurado, todas las notificaciones se desvían a ese número.
        target_number = self._notification_override_number or recipient.phone
        results = [
            self._message_sender.send_template(target_number, template)
            for template in templates
        ]

        # Copia de depuración a un número interno, si está configurado.
        debug_number = self._debug_notification_number
        if debug_number and debug_number != target_number:
            for template in templates:
                self._message_sender.send_template(debug_number, template)

        return all(results)
