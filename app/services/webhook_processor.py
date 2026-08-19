from typing import Iterable, Iterator

from app.services.shipping_notifier import ShippingNotificationService


class WhatsAppWebhookProcessor:
    """Interpreta los eventos del webhook de Meta y dispara las acciones
    de negocio correspondientes.

    Filtra los mensajes entrantes (remitentes autorizados, solo documentos
    PDF) y delega el procesamiento de cada guía al servicio de notificación.
    """

    def __init__(
        self,
        notifier: ShippingNotificationService,
        allowed_sender_numbers: Iterable[str],
    ) -> None:
        self._notifier = notifier
        self._allowed_sender_numbers = set(allowed_sender_numbers)

    def handle_event(self, data: dict) -> None:
        print(data)  # Debug
        try:
            for message in self._iter_messages(data):
                self._handle_message(message)
        except Exception as e:
            print(f"Error processing webhook: {e}")

    def _iter_messages(self, data: dict) -> Iterator[dict]:
        """Recorre la estructura del payload de Meta y yielda cada mensaje."""
        for entry in data.get("entry", []):
            for change in entry.get("changes", []):
                value = change.get("value", {})
                yield from value.get("messages", [])

    def _handle_message(self, message: dict) -> None:
        if message.get("from") not in self._allowed_sender_numbers:
            return

        if message.get("type") != "document":
            return

        document = message.get("document", {})
        if "pdf" not in document.get("mime_type", ""):
            return

        caption = document.get("caption", "")

        media_id = document.get("id")
        if not media_id:
            return

        print(f"Received PDF with media_id: {media_id}")
        self._notifier.notify_pdf_guide(media_id)
