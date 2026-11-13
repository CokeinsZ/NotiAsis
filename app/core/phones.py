import re
from dataclasses import dataclass


def normalize_phone(phone: str) -> str:
    """Normaliza un número de teléfono colombiano al formato sin '+'
    (ej. 573001234567).

    Todos los números se guardan y comparan en este formato para evitar
    duplicados por diferencias de prefijo entre Meta, el LLM, la DB y
    el Google Sheet (que trae los números sin código de país).
    """
    normalized = phone.strip().removeprefix("+")
    # Números de 10 dígitos (ej. del Google Sheet) llevan código de país 57.
    if len(normalized) == 10 and normalized.isdigit():
        normalized = f"57{normalized}"
    return normalized


@dataclass(frozen=True)
class PhoneCountry:
    """Indicativo y longitud del número nacional del país de un business
    (tabla countries del backend)."""

    phone_prefix: str
    phone_length: int
    # Código ISO del país ("CO", "GT"): de él sale la moneda si un pedido
    # no la trae.
    code: str = ""


def to_international(phone: str, country: PhoneCountry) -> str:
    """Teléfono sin '+' con el indicativo del país: un número nacional
    ("51319585" en Guatemala) lo recibe; uno que ya lo trae queda igual.

    Los teléfonos de Dropi, del Google Sheet y de los PDF llegan sin
    indicativo; los de los webhooks de Meta ya lo traen.
    """
    digits = re.sub(r"\D", "", str(phone or ""))
    if len(digits) == country.phone_length:
        return country.phone_prefix + digits
    return digits
