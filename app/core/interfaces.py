"""
Puertos (abstracciones) del dominio.

Los servicios de negocio dependen de estas interfaces y no de
implementaciones concretas (principio de inversión de dependencias).
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from datetime import date
from typing import TYPE_CHECKING

from app.core.phones import PhoneCountry
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


class SheetSource(ABC):
    """Fuente de hojas de cálculo (Google Sheets) con usuarios a notificar."""

    @abstractmethod
    def fetch_sheet(self, document_id: str, sheet_gid: str):
        """Descarga una hoja del documento y la retorna como DataFrame."""

    @abstractmethod
    def download_file(self, url: str) -> bytes | None:
        """Descarga un archivo público (ej. el PDF de la guía)."""


class DropiOrderSource(ABC):
    """Pedidos de Dropi, consultados a través de los webhooks de N8N
    conectados a su MCP (que tiene rate limit). Cada business tiene su
    propia cuenta de Dropi: `business_id` elige cuál se consulta."""

    @abstractmethod
    def fetch_orders(
        self, business_id: int, status: str, from_date: date, until_date: date, result_number: int, start: int
    ) -> tuple[list[dict], int]:
        """Una página de pedidos en el estado `status`: (items, count).

        Lanza excepción si la consulta falla.
        """

    @abstractmethod
    def fetch_order(self, business_id: int, order_id: str) -> dict | None:
        """Detalle de un pedido (cliente y productos). None si falla o si
        el pedido no es de la cuenta del business."""


class MediaUploader(ABC):
    """Subida de archivos multimedia al proveedor de mensajería."""

    @abstractmethod
    def upload_media(self, data: bytes, filename: str, mime_type: str = "application/pdf") -> str | None:
        """Sube un archivo y retorna su media_id. None si falla."""


class ProductVideoSource(ABC):
    """Videos de producto listos para enviar por WhatsApp."""

    @abstractmethod
    def video_for(self, business_id: int, products: list[dict]) -> tuple[str, str] | None:
        """(media_id, nombre del archivo) del video del primer producto
        que tenga uno. None si ninguno tiene video o no se pudo subir."""

    @abstractmethod
    def discard(self, media_id: str) -> None:
        """Olvida un media_id que Meta rechazó: se vuelve a subir la
        próxima vez."""


class RecipientInfoExtractor(ABC):
    """Extracción de datos del destinatario a partir de texto plano."""

    @abstractmethod
    def extract(self, text: str) -> RecipientInfo | None:
        """Extrae la información del destinatario. Retorna None si falla."""


class NotificationBackend(ABC):
    """Puerto hacia el backend de persistencia (API de Rust).

    Todas las operaciones son tolerantes a fallos: si el backend no
    responde, se registra el error y el flujo del bot continúa (fail-open),
    salvo `register_guide`, cuya respuesta determina si se notifica o no,
    y `lookup_guides` y `business_exists`, que retornan None para que el
    llamador se detenga.
    """

    @abstractmethod
    def fetch_authorized_associates(self) -> dict[str, int]:
        """Mapa {teléfono normalizado: business_id} de los asociados
        autorizados a enviar guías. {} si el backend no responde."""

    @abstractmethod
    def register_guide(
        self,
        number: str,
        user_phone: str,
        user_name: str,
        business_id: int,
        details: dict | None = None,
    ) -> bool:
        """Registra una guía recibida, asociada a su business.

        `details` son los datos del pedido (cliente y productos); el
        backend los guarda aunque la guía ya existiera.

        Retorna True si la guía es nueva (hay que notificar) o si el
        backend no responde (fail-open); False si ya fue registrada.
        """

    @abstractmethod
    def get_guide(self, number: str) -> dict | None:
        """Consulta una guía por número. None si no existe o el backend falla."""

    @abstractmethod
    def lookup_guides(self, numbers: list[str]) -> dict[str, dict] | None:
        """Guías existentes entre `numbers`, con sus datos de pedido
        ({número: guía}). None si el backend falla: sin saber cuántas
        veces se notificó cada guía no se puede decidir qué enviar."""

    @abstractmethod
    def business_exists(self, business_id: int) -> bool | None:
        """True/False según exista el business. None si el backend falla."""

    @abstractmethod
    def get_business_country(self, business_id: int) -> PhoneCountry | None:
        """País del business (indicativo y longitud de sus teléfonos).
        None si no existe o el backend falla."""

    @abstractmethod
    def get_business_sheet(self, business_id: int) -> dict | None:
        """Configuración del Google Sheet del business (document_id,
        office_id, delivered_id). None si no existe o el backend falla."""

    @abstractmethod
    def mark_guide_notified(self, number: str, meta_message_id: str | None = None) -> None:
        """Marca la guía como notificada al destinatario.

        `meta_message_id` es el mensaje principal de la notificación: si
        Meta reporta después que falló, el backend la deshace y la guía se
        reintenta en la próxima corrida."""

    @abstractmethod
    def mark_guide_tracking(self, number: str, level: int, meta_message_id: str | None = None) -> None:
        """Registra que la guía recibió la notificación de seguimiento del
        envío del nivel `level` (1 o 2). El backend solo sube el nivel y no
        toca `notification_count` (el del nivel 3, reclame en oficina).
        `meta_message_id` como en `mark_guide_notified`."""

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
        reply_to_meta_message_id: str | None = None,
    ) -> None:
        """Registra un mensaje entrante de un usuario final.

        `reply_to_meta_message_id` es el mensaje que el cliente citó al
        responder: el backend lo usa para saber a qué business y guía
        responde (un cliente puede tener chat con varios business).
        """

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
        guide_number: str | None = None,
    ) -> None:
        """Registra un mensaje saliente (plantilla) en el historial del
        chat, con la guía que notifica."""

    @abstractmethod
    def update_message_status(self, meta_message_id: str, status: str) -> None:
        """Actualiza el estado (sent/delivered/read) de un mensaje saliente."""

    @abstractmethod
    def register_message_error(
        self,
        meta_message_id: str,
        error_code: int | None,
        error_title: str | None,
        payload: dict,
    ) -> None:
        """Meta reportó que un mensaje enviado falló (estado 'failed').

        El backend guarda el error con el estado completo (`payload`), deja
        el mensaje en 'failed' y deshace la notificación de guía ligada a
        él. No se reintenta aquí: lo hace la próxima corrida del job."""
