from dataclasses import dataclass


@dataclass(frozen=True)
class RecipientInfo:
    """Información del destinatario extraída de una guía de envío."""

    name: str
    phone: str
    tracking_number: str = ""
    delivery_address: str = ""
    product: str = ""

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
