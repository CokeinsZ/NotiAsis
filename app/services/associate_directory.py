from typing import Iterable

from app.core.interfaces import NotificationBackend
from app.core.phones import normalize_phone


class AssociateDirectory:
    """Directorio en memoria de los business_associates autorizados.

    Se carga desde el backend al iniciar la aplicación. Si el backend no
    responde, usa como respaldo la lista local de números autorizados
    (sin business_id asociado).
    """

    def __init__(self, phones_to_business: dict[str, int | None]) -> None:
        # Las llaves se guardan normalizadas (sin '+') para comparar
        # sin importar el formato en que lleguen los números.
        self._phones_to_business = {
            normalize_phone(phone): business_id
            for phone, business_id in phones_to_business.items()
        }

    @classmethod
    def load(
        cls,
        backend: NotificationBackend,
        fallback_numbers: Iterable[str] = (),
    ) -> "AssociateDirectory":
        associates = backend.fetch_authorized_associates()
        if associates:
            print(f"Loaded {len(associates)} authorized associate(s) from backend.")
            return cls(associates)

        fallback = {normalize_phone(number): None for number in fallback_numbers}
        if fallback:
            print(
                f"Backend unavailable or without associates; "
                f"falling back to {len(fallback)} local authorized number(s)."
            )
        else:
            print("WARNING: no authorized associate numbers configured.")
        return cls(fallback)

    def is_authorized(self, phone: str) -> bool:
        return normalize_phone(phone) in self._phones_to_business

    def business_id_for(self, phone: str) -> int | None:
        """Business del asociado; None si solo se conoce del fallback local."""
        return self._phones_to_business.get(normalize_phone(phone))
