import time
from datetime import date
from typing import Callable

import requests

from app.core.interfaces import DropiOrderSource

# Respuestas que vale la pena reintentar: rate limit del MCP de Dropi y
# errores transitorios de N8N.
RETRYABLE_STATUS = (429, 500, 502, 503, 504)


class N8nDropiClient(DropiOrderSource):
    """Consulta los pedidos de Dropi a través de los webhooks de N8N
    conectados a su MCP:

    - GET {base}/oficina  (body JSON: from, until, result_number, start)
    - GET {base}/orden?order_id=...

    El MCP tiene rate limit: ante 429/5xx o errores de conexión se
    reintenta con espera exponencial.
    """

    def __init__(
        self,
        base_url: str,
        timeout: int = 60,
        max_attempts: int = 3,
        backoff_seconds: float = 2.0,
        session: requests.Session | None = None,
        sleep: Callable[[float], None] = time.sleep,
    ) -> None:
        self._base_url = base_url.rstrip("/")
        self._timeout = timeout
        self._max_attempts = max_attempts
        self._backoff_seconds = backoff_seconds
        self._session = session or requests.Session()
        self._sleep = sleep

    def fetch_office_orders(
        self, from_date: date, until_date: date, result_number: int, start: int
    ) -> tuple[list[dict], int]:
        response = self._get(
            "/oficina",
            json={
                "from": from_date.isoformat(),
                "until": until_date.isoformat(),
                "result_number": result_number,
                "start": start,
            },
        )
        response.raise_for_status()
        data = response.json()
        items = data.get("items") or []
        count = data.get("count")
        return items, int(count) if count is not None else len(items)

    def fetch_order(self, order_id: str) -> dict | None:
        try:
            response = self._get("/orden", params={"order_id": order_id})
            if response.status_code == 200:
                return response.json()
            print(f"Error fetching Dropi order {order_id} ({response.status_code}): {response.text}")
            return None
        except Exception as e:
            print(f"Error fetching Dropi order {order_id}: {e}")
            return None

    def _get(self, path: str, **kwargs) -> requests.Response:
        url = f"{self._base_url}{path}"
        for attempt in range(1, self._max_attempts + 1):
            last_attempt = attempt == self._max_attempts
            try:
                response = self._session.get(url, timeout=self._timeout, **kwargs)
            except requests.RequestException as e:
                if last_attempt:
                    raise
                print(f"Dropi {path} failed ({e}); retrying ({attempt}/{self._max_attempts})")
            else:
                if response.status_code not in RETRYABLE_STATUS or last_attempt:
                    return response
                print(f"Dropi {path} returned {response.status_code}; retrying ({attempt}/{self._max_attempts})")
            self._sleep(self._backoff_seconds * 2 ** (attempt - 1))
        raise AssertionError("unreachable")
