"""
Composición de dependencias de la aplicación (composition root).

Aquí se construyen las implementaciones concretas y se inyectan en los
servicios que dependen de abstracciones.
"""

from functools import lru_cache

from app.config import Settings
from app.services.pdf_extractor import PdfTextExtractor
from app.services.recipient_extractor import DeepSeekRecipientExtractor
from app.services.shipping_notifier import ShippingNotificationService
from app.services.webhook_processor import WhatsAppWebhookProcessor
from app.whatsapp.client import WhatsAppClient


@lru_cache
def get_settings() -> Settings:
    return Settings.from_env()


@lru_cache
def get_webhook_processor() -> WhatsAppWebhookProcessor:
    settings = get_settings()

    whatsapp_client = WhatsAppClient(
        access_token=settings.whatsapp_token,
        phone_number_id=settings.whatsapp_phone_id,
    )
    notifier = ShippingNotificationService(
        media_repository=whatsapp_client,
        message_sender=whatsapp_client,
        pdf_extractor=PdfTextExtractor(),
        recipient_extractor=DeepSeekRecipientExtractor(
            api_key=settings.deepseek_api_key
        ),
        debug_notification_number=settings.debug_notification_number,
        notification_override_number=settings.notification_override_number,
    )
    return WhatsAppWebhookProcessor(
        notifier=notifier,
        allowed_sender_numbers=settings.allowed_sender_numbers,
    )
