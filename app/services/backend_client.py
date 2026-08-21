import requests

from app.core.interfaces import NotificationBackend
from app.core.phones import normalize_phone


class RustBackendClient(NotificationBackend):
    """Implementación HTTP del puerto NotificationBackend contra la API de Rust.

    Nunca lanza excepciones: si el backend no responde, registra el error
    y deja continuar el flujo del bot (fail-open).
    """

    def __init__(self, base_url: str, timeout: int = 10) -> None:
        self._base_url = base_url.rstrip("/")
        self._timeout = timeout
        self._session = requests.Session()

    def fetch_authorized_associates(self) -> dict[str, int]:
        # GET /associates devuelve los asociados con su business_id.
        try:
            response = self._session.get(
                f"{self._base_url}/associates", timeout=self._timeout
            )
            response.raise_for_status()
            associates = response.json().get("associates", [])
            return {
                normalize_phone(a["phone_number"]): a["business_id"]
                for a in associates
            }
        except Exception as e:
            print(f"Error fetching authorized associates from backend: {e}")
            return {}

    def register_guide(self, number: str, user_phone: str, user_name: str) -> bool:
        try:
            response = self._session.post(
                f"{self._base_url}/guides",
                json={
                    "number": number,
                    "user_phone": normalize_phone(user_phone),
                    "user_name": user_name,
                },
                timeout=self._timeout,
            )
            response.raise_for_status()
            created = bool(response.json().get("created", True))
            if not created:
                print(f"Guide {number} was already registered, skipping notification.")
            return created
        except Exception as e:
            print(f"Error registering guide {number} in backend: {e}")
            return True  # fail-open: mejor notificar duplicado que perder la notificación

    def mark_guide_notified(self, number: str) -> None:
        try:
            response = self._session.post(
                f"{self._base_url}/guides/{number}/notified",
                timeout=self._timeout,
            )
            response.raise_for_status()
        except Exception as e:
            print(f"Error marking guide {number} as notified: {e}")

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
        try:
            response = self._session.post(
                f"{self._base_url}/messages/incoming",
                json={
                    "user_phone": normalize_phone(user_phone),
                    "user_name": user_name,
                    "meta_message_id": meta_message_id,
                    "media_type": media_type,
                    "message": message,
                    "media_id": media_id,
                    "timestamp": timestamp,
                },
                timeout=self._timeout,
            )
            if response.status_code == 404:
                print(f"Incoming message from {user_phone} ignored: no chat registered yet.")
                return
            response.raise_for_status()
        except Exception as e:
            print(f"Error registering incoming message {meta_message_id}: {e}")

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
        try:
            response = self._session.post(
                f"{self._base_url}/messages/outgoing",
                json={
                    "business_id": business_id,
                    "user_phone": normalize_phone(user_phone),
                    "user_name": user_name,
                    "meta_message_id": meta_message_id,
                    "media_type": media_type,
                    "message": message,
                    "media_id": media_id,
                },
                timeout=self._timeout,
            )
            response.raise_for_status()
        except Exception as e:
            print(f"Error registering outgoing message {meta_message_id}: {e}")

    def update_message_status(self, meta_message_id: str, status: str) -> None:
        try:
            response = self._session.patch(
                f"{self._base_url}/messages/{meta_message_id}/status",
                json={"status": status},
                timeout=self._timeout,
            )
            response.raise_for_status()
        except Exception as e:
            print(f"Error updating status of message {meta_message_id}: {e}")
