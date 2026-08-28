from typing import Callable, Iterable

from app.core.interfaces import (
    MediaRepository,
    MessageSender,
    NotificationBackend,
    RecipientInfoExtractor,
    TextExtractor,
)
from app.models.shipping import RecipientInfo
from app.services.associate_directory import AssociateDirectory
from app.services.notification_policy import NotificationStep, step_for_notification_count
from app.whatsapp.templates.base import TemplateMessage
from app.whatsapp.templates.guia import GuiaTemplate
from app.whatsapp.templates.mensaje_guia import MensajeGuiaTemplate
from app.whatsapp.templates.recordatorio import RecordatorioTemplate
from app.whatsapp.templates.recordatorio_final import RecordatorioFinalTemplate

# Plantillas que se envían la primera vez que se notifica una guía,
# en orden de envío: primero el PDF de la guía y luego el mensaje
# con la información extraída.
DEFAULT_TEMPLATE_FACTORIES: tuple = (GuiaTemplate, MensajeGuiaTemplate)

# Plantilla de recordatorio según el número de notificaciones previas.
REMINDER_FACTORIES = {
    NotificationStep.REMINDER: RecordatorioTemplate,
    NotificationStep.FINAL_REMINDER: RecordatorioFinalTemplate,
}


class ShippingNotificationService:
    """Orquesta el flujo de negocio de notificación de guías de envío:

    descargar el PDF de la guía -> extraer su texto -> extraer los datos
    del destinatario -> decidir el paso de notificación según cuántas
    veces se haya notificado antes (política de escalación) -> enviar las
    plantillas -> registrar los mensajes en el backend.
    """

    def __init__(
        self,
        media_repository: MediaRepository,
        message_sender: MessageSender,
        pdf_extractor: TextExtractor,
        recipient_extractor: RecipientInfoExtractor,
        backend: NotificationBackend,
        associate_directory: AssociateDirectory,
        debug_notification_number: str | None = None,
        notification_override_number: str | None = None,
        template_factories: Iterable[Callable[[RecipientInfo, str], TemplateMessage]] = DEFAULT_TEMPLATE_FACTORIES,
    ) -> None:
        self._media_repository = media_repository
        self._message_sender = message_sender
        self._pdf_extractor = pdf_extractor
        self._recipient_extractor = recipient_extractor
        self._backend = backend
        self._associate_directory = associate_directory
        self._debug_notification_number = debug_notification_number
        self._notification_override_number = notification_override_number
        self._template_factories = tuple(template_factories)

    def notify_pdf_guide(self, media_id: str, associate_phone: str) -> bool:
        """Procesa una guía en PDF y notifica al destinatario según la
        escalación: inicial (guía+mensaje) -> recordatorio ->
        recordatorio_final -> nada (máximo alcanzado).

        Retorna True si se envió alguna notificación.
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

        business_id = self._associate_directory.business_id_for(associate_phone)
        if business_id is None:
            print(
                f"Associate {associate_phone} has no business in the backend; "
                "outgoing messages will not be registered in the chat history."
            )

        tracking_number = recipient.tracking_number
        existing_guide = self._backend.get_guide(tracking_number)

        if existing_guide is None:
            # Guía nunca notificada: registrar y enviar plantillas iniciales.
            if not self._backend.register_guide(tracking_number, recipient.phone, recipient.name):
                # Carrera rara: otro proceso la registró mientras tanto.
                return False
            templates = [factory(recipient, media_id) for factory in self._template_factories]
        else:
            step = step_for_notification_count(existing_guide.get("notification_count") or 0)
            print(f"Guide {tracking_number} already notified (step: {step.value})")

            if step is NotificationStep.STOP:
                return False
            if step is NotificationStep.INITIAL:
                templates = [factory(recipient, media_id) for factory in self._template_factories]
            else:
                templates = [REMINDER_FACTORIES[step](recipient, media_id)]

        sent = self._notify_recipient(recipient, templates, business_id)
        if sent:
            self._backend.mark_guide_notified(tracking_number)
        return sent

    def _notify_recipient(
        self,
        recipient: RecipientInfo,
        templates: list[TemplateMessage],
        business_id: int | None,
    ) -> bool:
        """Envía las plantillas dadas, en orden, y las registra."""
        # Mientras NOTIFICATION_OVERRIDE_NUMBER esté configurado, todas las
        # notificaciones se desvían a ese número (útil en pruebas).
        target_number = self._notification_override_number or recipient.phone

        sent_templates: list[tuple[TemplateMessage, str]] = []
        for template in templates:
            meta_message_id = self._message_sender.send_template(target_number, template)
            if meta_message_id:
                sent_templates.append((template, meta_message_id))

        # Copia de depuración a un número interno, si está configurado.
        debug_number = self._debug_notification_number
        if debug_number and debug_number != target_number:
            for template in templates:
                self._message_sender.send_template(debug_number, template)

        # Registrar los mensajes en el historial del chat (business <-> user).
        # Se registra con el teléfono REAL del destinatario aunque el envío
        # físico se desvíe al número de pruebas.
        if business_id is not None:
            for template, meta_message_id in sent_templates:
                self._backend.register_outgoing_message(
                    business_id=business_id,
                    user_phone=recipient.phone,
                    user_name=recipient.name,
                    meta_message_id=meta_message_id,
                    media_type=template.media_type(),
                    message=template.log_message(),
                    media_id=template.log_media_id(),
                )

        return bool(sent_templates)
