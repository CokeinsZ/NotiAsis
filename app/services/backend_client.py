import requests

from app.core.interfaces import NotificationBackend
from app.core.phones import normalize_phone


class RustBackendClient(NotificationBackend):
    """Implementación HTTP del puerto NotificationBackend contra la API de Rust.

    Se autentica con la api_key (POST /auth/api-key) y adjunta el JWT en
    todas las llamadas. Si el token vence (401), renueva el login una vez
    y reintenta la petición.

    Nunca lanza excepciones: si el backend no responde, registra el error
    y deja continuar el flujo del bot (fail-open).
    """

    def __init__(self, base_url: str, api_key: str, timeout: int = 10) -> None:
        self._base_url = base_url.rstrip("/")
        self._api_key = api_key
        self._timeout = timeout
        self._session = requests.Session()
        self._token: str | None = None

    # ---------------- Autenticación ----------------

    def _login(self) -> None:
        response = self._session.post(
            f"{self._base_url}/auth/api-key",
            json={"api_key": self._api_key},
            timeout=self._timeout,
        )
        response.raise_for_status()
        self._token = response.json()["token"]
        print("Backend JWT obtained via api_key.")

    def _request(self, method: str, path: str, **kwargs) -> requests.Response:
        if self._token is None:
            self._login()

        response = self._session.request(
            method,
            f"{self._base_url}{path}",
            headers={"Authorization": f"Bearer {self._token}"},
            timeout=self._timeout,
            **kwargs,
        )
        if response.status_code == 401:
            # Token vencido: re-login y un solo reintento.
            self._login()
            response = self._session.request(
                method,
                f"{self._base_url}{path}",
                headers={"Authorization": f"Bearer {self._token}"},
                timeout=self._timeout,
                **kwargs,
            )
        return response

    # ---------------- NotificationBackend ----------------

    def fetch_authorized_associates(self) -> dict[str, int]:
        try:
            response = self._request("GET", "/associates")
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
            response = self._request(
                "POST",
                "/guides",
                json={
                    "number": number,
                    "user_phone": normalize_phone(user_phone),
                    "user_name": user_name,
                },
            )
            response.raise_for_status()
            created = bool(response.json().get("created", True))
            if not created:
                print(f"Guide {number} was already registered, skipping notification.")
            return created
        except Exception as e:
            print(f"Error registering guide {number} in backend: {e}")
            return True  # fail-open: mejor notificar duplicado que perder la notificación

    def get_guide(self, number: str) -> dict | None:
        try:
            response = self._request("GET", f"/guides/{number}")
            if response.status_code == 404:
                return None
            response.raise_for_status()
            return response.json().get("guide")
        except Exception as e:
            print(f"Error fetching guide {number} from backend: {e}")
            return None

    def get_business_sheet(self, business_id: int) -> dict | None:
        try:
            response = self._request("GET", f"/businesses/{business_id}/sheet")
            if response.status_code == 404:
                print(f"Business {business_id} has no sheet config.")
                return None
            response.raise_for_status()
            return response.json().get("sheet")
        except Exception as e:
            print(f"Error fetching sheet config for business {business_id}: {e}")
            return None

    def mark_guide_notified(self, number: str) -> None:
        try:
            response = self._request("POST", f"/guides/{number}/notified")
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
            response = self._request(
                "POST",
                "/messages/incoming",
                json={
                    "user_phone": normalize_phone(user_phone),
                    "user_name": user_name,
                    "meta_message_id": meta_message_id,
                    "media_type": media_type,
                    "message": message,
                    "media_id": media_id,
                    "timestamp": timestamp,
                },
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
            response = self._request(
                "POST",
                "/messages/outgoing",
                json={
                    "business_id": business_id,
                    "user_phone": normalize_phone(user_phone),
                    "user_name": user_name,
                    "meta_message_id": meta_message_id,
                    "media_type": media_type,
                    "message": message,
                    "media_id": media_id,
                },
            )
            response.raise_for_status()
        except Exception as e:
            print(f"Error registering outgoing message {meta_message_id}: {e}")

    def update_message_status(self, meta_message_id: str, status: str) -> None:
        try:
            response = self._request(
                "PATCH",
                f"/messages/{meta_message_id}/status",
                json={"status": status},
            )
            response.raise_for_status()
        except Exception as e:
            print(f"Error updating status of message {meta_message_id}: {e}")
