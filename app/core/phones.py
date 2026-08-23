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
