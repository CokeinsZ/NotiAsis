def normalize_phone(phone: str) -> str:
    """Normaliza un número de teléfono al formato sin '+' (ej. 573001234567).

    Todos los números se guardan y comparan en este formato para evitar
    duplicados por diferencias de prefijo entre Meta, el LLM y la DB.
    """
    return phone.strip().removeprefix("+")
