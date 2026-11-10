from typing import Iterator

from app.core.interfaces import NotificationBackend
from app.services.associate_directory import AssociateDirectory
from app.services.shipping_notifier import ShippingNotificationService

# Tipos de mensaje de Meta que el backend soporta en media_type_enum.
SUPPORTED_MEDIA_TYPES = ("text", "document", "audio", "image", "video")

# Toque de un botón de respuesta rápida de una plantilla: se guarda como texto.
BUTTON_TYPE = "button"

# Estados de mensajes salientes que Meta reporta en los webhooks.
SUPPORTED_STATUSES = ("sent", "delivered", "read")

# Meta aceptó el mensaje pero no lo pudo entregar (trae `errors`).
FAILED_STATUS = "failed"


class WhatsAppWebhookProcessor:
    """Interpreta los eventos del webhook de Meta y dispara las acciones
    de negocio correspondientes:

    - PDFs de guías enviados por asociados autorizados -> notificación.
    - Mensajes de usuarios finales -> historial del chat en el backend.
    - Actualizaciones de estado (sent/delivered/read) -> backend.
    - Errores de envío (failed) -> backend (deshace la notificación de la guía).
    """

    def __init__(
        self,
        notifier: ShippingNotificationService,
        associate_directory: AssociateDirectory,
        backend: NotificationBackend,
    ) -> None:
        self._notifier = notifier
        self._associate_directory = associate_directory
        self._backend = backend

    def handle_event(self, data: dict) -> None:
        print(data)  # Debug
        try:
            for value in self._iter_values(data):
                self._handle_statuses(value.get("statuses", []))
                contact_name = self._extract_contact_name(value)
                for message in value.get("messages", []):
                    self._handle_message(message, contact_name)
        except Exception as e:
            print(f"Error processing webhook: {e}")

    def _iter_values(self, data: dict) -> Iterator[dict]:
        """Recorre la estructura del payload de Meta y yielda cada value."""
        for entry in data.get("entry", []):
            for change in entry.get("changes", []):
                yield change.get("value", {})

    @staticmethod
    def _extract_contact_name(value: dict) -> str | None:
        contacts = value.get("contacts", [])
        if contacts:
            return contacts[0].get("profile", {}).get("name")
        return None

    def _handle_statuses(self, statuses: list) -> None:
        """Meta reporta aquí los estados de los mensajes que enviamos."""
        for status in statuses:
            meta_message_id = status.get("id")
            state = status.get("status")
            if not meta_message_id:
                continue
            if state in SUPPORTED_STATUSES:
                self._backend.update_message_status(meta_message_id, state)
            elif state == FAILED_STATUS:
                self._handle_failure(meta_message_id, status)

    def _handle_failure(self, meta_message_id: str, status: dict) -> None:
        """Meta aceptó el envío (HTTP 200) pero no lo entregó. El backend
        guarda el error, deja el mensaje en 'failed' y, si era el de una
        notificación de guía, la deshace: la próxima corrida del job la
        vuelve a enviar (aquí no se reintenta)."""
        errors = status.get("errors") or [{}]
        error = errors[0] if isinstance(errors[0], dict) else {}
        code = error.get("code")
        print(f"Message {meta_message_id} failed: {code} {error.get('title')!r}")
        self._backend.register_message_error(
            meta_message_id,
            error_code=code if isinstance(code, int) else None,
            error_title=error.get("title"),
            payload=status,
        )

    def _handle_message(self, message: dict, contact_name: str | None) -> None:
        sender = message.get("from", "")

        if self._associate_directory.is_authorized(sender):
            self._handle_associate_message(message)
        else:
            self._handle_user_message(message, contact_name)

    def _handle_associate_message(self, message: dict) -> None:
        """De los asociados solo nos interesan los PDFs (guías de envío)."""
        if message.get("type") != "document":
            return

        document = message.get("document", {})
        if "pdf" not in document.get("mime_type", ""):
            return

        media_id = document.get("id")
        if not media_id:
            return

        print(f"Received PDF with media_id: {media_id}")
        self._notifier.notify_pdf_guide(
            media_id, associate_phone=message.get("from", "")
        )

    def _handle_user_message(self, message: dict, contact_name: str | None) -> None:
        """Mensaje de un usuario final: se registra en el historial del chat."""
        message_type = message.get("type", "")
        media_type = "text" if message_type == BUTTON_TYPE else message_type
        if media_type not in SUPPORTED_MEDIA_TYPES:
            print(f"Unsupported message type from user: {message_type}")
            return

        meta_message_id = message.get("id")
        user_phone = message.get("from")
        if not meta_message_id or not user_phone:
            return

        text, media_id = self._extract_content(message, message_type)

        timestamp = message.get("timestamp")
        self._backend.register_incoming_message(
            user_phone=user_phone,
            user_name=contact_name,
            meta_message_id=meta_message_id,
            media_type=media_type,
            message=text,
            media_id=media_id,
            timestamp=int(timestamp) if timestamp else None,
            # Mensaje citado al responder (o plantilla del botón tocado):
            # dice a qué business y guía responde el cliente.
            reply_to_meta_message_id=(message.get("context") or {}).get("id"),
        )

    @staticmethod
    def _extract_content(message: dict, message_type: str) -> tuple[str | None, str | None]:
        """Extrae (texto, media_id) según el tipo de mensaje de Meta."""
        if message_type == "text":
            return message.get("text", {}).get("body"), None
        if message_type == BUTTON_TYPE:
            return message.get("button", {}).get("text"), None
        content = message.get(message_type, {})
        return content.get("caption"), content.get("id")
