"""
Puertos (abstracciones) del dominio.

Los servicios de negocio dependen de estas interfaces y no de
implementaciones concretas (principio de inversión de dependencias).
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import TYPE_CHECKING

from app.models.shipping import RecipientInfo

if TYPE_CHECKING:
    # Import solo para anotaciones de tipo: evita una dependencia en runtime
    # del núcleo hacia la infraestructura de WhatsApp (y un import circular).
    from app.whatsapp.templates.base import TemplateMessage


class MediaRepository(ABC):
    """Acceso a archivos multimedia recibidos por mensajería."""

    @abstractmethod
    def get_media_url(self, media_id: str) -> str | None:
        """Obtiene la URL de descarga de un archivo a partir de su ID."""

    @abstractmethod
    def download_media(self, media_url: str) -> bytes | None:
        """Descarga el contenido binario de un archivo multimedia."""


class MessageSender(ABC):
    """Envío de mensajes a través de un canal de mensajería."""

    @abstractmethod
    def send_template(self, to_number: str, template: TemplateMessage) -> str | None:
        """Envía un mensaje de plantilla.

        Retorna el ID del mensaje asignado por el proveedor (wamid en Meta),
        o None si el envío falló.
        """


class TextExtractor(ABC):
    """Extracción de texto a partir de contenido binario."""

    @abstractmethod
    def extract_text(self, data: bytes) -> str:
        """Extrae el texto de un documento. Retorna "" si falla."""


class RecipientInfoExtractor(ABC):
    """Extracción de datos del destinatario a partir de texto plano."""

    @abstractmethod
    def extract(self, text: str) -> RecipientInfo | None:
        """Extrae la información del destinatario. Retorna None si falla."""


class NotificationBackend(ABC):
    """Puerto hacia el backend de persistencia (API de Rust).

    Todas las operaciones son tolerantes a fallos: si el backend no
    responde, se registra el error y el flujo del bot continúa (fail-open),
    salvo `register_guide`, cuya respuesta determina si se notifica o no.
    """

    @abstractmethod
    def fetch_authorized_associates(self) -> dict[str, int]:
        """Mapa {teléfono normalizado: business_id} de los asociados
        autorizados a enviar guías. {} si el backend no responde."""

    @abstractmethod
    def register_guide(self, number: str, user_phone: str, user_name: str) -> bool:
        """Registra una guía recibida.

        Retorna True si la guía es nueva (hay que notificar) o si el
        backend no responde (fail-open); False si ya fue registrada.
        """

    @abstractmethod
    def mark_guide_notified(self, number: str) -> None:
        """Marca la guía como notificada al destinatario."""

    @abstractmethod
    def register_incoming_message(
        self,
        *,
        user_phone: str,
        user_name: str | None,
        meta_message_id: str,
        media_type: str,
        message: str | None,
        media_id: str | None,
        timestamp: int | None,
    ) -> None:
        """Registra un mensaje entrante de un usuario final."""

    @abstractmethod
    def register_outgoing_message(
        self,
        *,
        business_id: int,
        user_phone: str,
        user_name: str | None,
        meta_message_id: str,
        media_type: str,
        message: str | None,
        media_id: str | None,
    ) -> None:
        """Registra un mensaje saliente (plantilla) en el historial del chat."""

    @abstractmethod
    def update_message_status(self, meta_message_id: str, status: str) -> None:
        """Actualiza el estado (sent/delivered/read) de un mensaje saliente."""
