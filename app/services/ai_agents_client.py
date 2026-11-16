import requests

from app.core.interfaces import CustomerMessageListener


class AiAgentsClient(CustomerMessageListener):
    """Avisa al servicio ai-agents (Rust) que un cliente escribió, para que
    el agente de atención al cliente le responda.

    `POST {base_url}/agents/customer-service/events` con el header
    `api-key`. El servicio responde 202 y trabaja en segundo plano: espera
    unos segundos por si el cliente sigue escribiendo, lee el chat desde la
    base de datos y responde por el backend.

    Nunca lanza excepciones: si el servicio no responde, el mensaje ya quedó
    registrado y un asesor lo ve en la webapp (fail-open).
    """

    EVENTS_PATH = "/agents/customer-service/events"

    def __init__(self, base_url: str, api_key: str, timeout: int = 5, session=None) -> None:
        self._url = f"{base_url.rstrip('/')}{self.EVENTS_PATH}"
        self._api_key = api_key
        self._timeout = timeout
        self._session = session or requests.Session()

    def on_customer_message(self, message: dict) -> None:
        try:
            response = self._session.post(
                self._url,
                json={
                    "message_id": message["id"],
                    "business_id": message["business_id"],
                    "user_phone": message["user_id"],
                },
                headers={"api-key": self._api_key},
                timeout=self._timeout,
            )
            if response.status_code >= 400:
                print(f"ai-agents rejected message {message.get('id')}: {response.status_code} {response.text[:200]}")
        except Exception as e:
            print(f"Error notifying ai-agents about message {message.get('id')}: {e}")
