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
    def send_template(self, to_number: str, template: TemplateMessage) -> bool:
        """Envía un mensaje de plantilla. Retorna True si tuvo éxito."""


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
