import threading
from typing import Hashable


class RunGuard:
    """Corridas en curso por clave (el business_id): evita lanzar una
    segunda corrida mientras la primera no ha terminado.

    En memoria: vale para un solo proceso (el bot corre uvicorn con un
    solo worker).
    """

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._running: set[Hashable] = set()

    def try_start(self, key: Hashable) -> bool:
        """Marca la corrida como iniciada; False si ya había una en curso."""
        with self._lock:
            if key in self._running:
                return False
            self._running.add(key)
            return True

    def finish(self, key: Hashable) -> None:
        with self._lock:
            self._running.discard(key)
