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
    backend_api_key: str
    # Respaldo local de números autorizados, usado solo si el backend
    # no responde al iniciar (la fuente principal es la base de datos).
    allowed_sender_numbers: frozenset
    debug_notification_number: str | None = None
    notification_override_number: str | None = None
    # Webhooks de N8N conectados al MCP de Dropi (/ordenes y /orden) y
    # su header api-key.
    dropi_n8n_url: str = "https://n8n.notiasis.com/webhook/dropi"
    n8n_api_key: str = ""
    # Videos de producto ({media_dir}/business/{id}/products/*.mp4) y
    # caché de sus media_id de WhatsApp. Rutas dentro del contenedor.
    media_dir: str = "/data/media"
    media_cache_path: str = "/data/state/whatsapp_media.json"

    @classmethod
    def from_env(cls) -> "Settings":
        raw_allowed = os.getenv("ALLOWED_SENDER_NUMBERS") or ""
        allowed = frozenset(
            number.strip() for number in raw_allowed.split(",") if number.strip()
        )
        return cls(
            whatsapp_token=os.getenv("WHATSAPP_TOKEN", ""),
            whatsapp_phone_id=os.getenv("WHATSAPP_PHONE_ID", ""),
            verify_token=os.getenv("VERIFY_TOKEN", "my_secret_token"),
            deepseek_api_key=os.getenv("DEEPSEEK_API_KEY", ""),
            backend_api_url=os.getenv("BACKEND_API_URL", "http://backend:3001"),
            backend_api_key=os.getenv("BACKEND_API_KEY", ""),
            allowed_sender_numbers=allowed,
            debug_notification_number=os.getenv("DEBUG_NOTIFICATION_NUMBER"),
            notification_override_number=os.getenv("NOTIFICATION_OVERRIDE_NUMBER"),
            dropi_n8n_url=os.getenv("DROPI_N8N_URL") or cls.dropi_n8n_url,
            n8n_api_key=(os.getenv("N8N_API_KEY") or "").strip(),
            media_dir=os.getenv("MEDIA_DIR") or cls.media_dir,
            media_cache_path=os.getenv("MEDIA_CACHE_PATH") or cls.media_cache_path,
        )
