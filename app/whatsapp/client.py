import requests

from app.core.interfaces import MediaRepository, MediaUploader, MessageSender
from app.whatsapp.templates.base import TemplateMessage


class WhatsAppClient(MediaRepository, MessageSender, MediaUploader):
    """Cliente de bajo nivel para la WhatsApp Cloud API (Graph API de Meta).

    Solo conoce HTTP y la API de Meta: descarga de multimedia y envío de
    mensajes. No contiene lógica de negocio ni detalles de plantillas.
    """

    GRAPH_API_URL = "https://graph.facebook.com/v19.0"

    def __init__(
        self,
        access_token: str,
        phone_number_id: str,
        timeout: int = 30,
        session: requests.Session | None = None,
    ) -> None:
        self._phone_number_id = phone_number_id
        self._timeout = timeout
        self._session = session or requests.Session()
        self._session.headers.update({"Authorization": f"Bearer {access_token}"})

    def get_media_url(self, media_id: str) -> str | None:
        """Obtiene la URL de descarga de un media ID desde la API de Meta."""
        response = self._session.get(
            f"{self.GRAPH_API_URL}/{media_id}", timeout=self._timeout
        )
        if response.status_code == 200:
            return response.json().get("url")
        print(f"Error getting media URL: {response.text}")
        return None

    def download_media(self, media_url: str) -> bytes | None:
        """Descarga el contenido binario de una URL de multimedia de Meta."""
        response = self._session.get(media_url, timeout=self._timeout)
        if response.status_code == 200:
            return response.content
        print(f"Error downloading media: {response.text}")
        return None

    def upload_media(self, data: bytes, filename: str) -> str | None:
        """Sube un archivo a Meta y retorna su media_id.

        El media_id se usa luego en plantillas con documento y permite
        visualizar el archivo desde la webapp.
        """
        url = f"{self.GRAPH_API_URL}/{self._phone_number_id}/media"
        files = {"file": (filename, data, "application/pdf")}
        form = {"messaging_product": "whatsapp", "type": "application/pdf"}
        response = self._session.post(
            url, files=files, data=form, timeout=self._timeout
        )
        if response.status_code == 200:
            media_id = response.json().get("id")
            print(f"Media uploaded successfully ({media_id})")
            return media_id
        print(f"Error uploading media: {response.text}")
        return None

    def send_template(self, to_number: str, template: TemplateMessage) -> str | None:
        """Envía un mensaje de plantilla a un número de WhatsApp.

        Retorna el meta_message_id (wamid) asignado por Meta, necesario
        para rastrear los estados del mensaje. None si el envío falló.
        """
        url = f"{self.GRAPH_API_URL}/{self._phone_number_id}/messages"
        payload = template.build_payload(to_number)
        response = self._session.post(url, json=payload, timeout=self._timeout)
        if response.status_code == 200:
            message_id = self._extract_message_id(response)
            print(f"Template '{template.name}' sent successfully to {to_number} ({message_id})")
            return message_id
        print(f"Error sending template '{template.name}' to {to_number}: {response.text}")
        return None

    @staticmethod
    def _extract_message_id(response: requests.Response) -> str | None:
        try:
            return response.json()["messages"][0]["id"]
        except (KeyError, IndexError, TypeError, ValueError):
            return None
