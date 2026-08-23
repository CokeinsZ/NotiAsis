from dataclasses import dataclass

from app.core.phones import normalize_phone


@dataclass(frozen=True)
class RecipientInfo:
    """Información del destinatario extraída de una guía de envío.

    El teléfono se guarda siempre normalizado (sin '+').
    """

    name: str
    phone: str
    tracking_number: str = ""
    delivery_address: str = ""
    product: str = ""

    def __post_init__(self) -> None:
        # Invariante del modelo: el teléfono siempre queda normalizado.
        object.__setattr__(self, "phone", normalize_phone(self.phone))

    @classmethod
    def from_dict(cls, data: dict) -> "RecipientInfo":
        return cls(
            name=str(data.get("name") or ""),
            phone=str(data.get("phone") or ""),
            tracking_number=str(data.get("tracking_number") or ""),
            delivery_address=str(data.get("delivery_address") or ""),
            product=str(data.get("product") or "")
        )

    @property
    def is_complete(self) -> bool:
        return bool(self.name and self.phone and self.tracking_number and self.delivery_address and self.product)
