import os
from dataclasses import dataclass

from dotenv import load_dotenv

load_dotenv()

@dataclass(frozen=True)
class Settings:
    """Configuración centralizada e inmutable de la aplicación."""

    whatsapp_token: str
    whatsapp_phone_id: str
    verify_token: str
    deepseek_api_key: str
    backend_api_url: str
    # Respaldo local de números autorizados, usado solo si el backend
    # no responde al iniciar (la fuente principal es la base de datos).
    allowed_sender_numbers: frozenset
    debug_notification_number: str | None = None
    notification_override_number: str | None = None

    @classmethod
    def from_env(cls) -> "Settings":
        raw_allowed = os.getenv("ALLOWED_SENDER_NUMBERS")
        allowed = frozenset(
            number.strip() for number in raw_allowed.split(",") if number.strip()
        )
        return cls(
            whatsapp_token=os.getenv("WHATSAPP_TOKEN", ""),
            whatsapp_phone_id=os.getenv("WHATSAPP_PHONE_ID", ""),
            verify_token=os.getenv("VERIFY_TOKEN", "my_secret_token"),
            deepseek_api_key=os.getenv("DEEPSEEK_API_KEY", ""),
            backend_api_url=os.getenv("BACKEND_API_URL", "http://backend:3001"),
            allowed_sender_numbers=allowed,
            debug_notification_number=os.getenv("DEBUG_NOTIFICATION_NUMBER"),
            notification_override_number=os.getenv("NOTIFICATION_OVERRIDE_NUMBER"),
        )
