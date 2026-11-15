"""
Composición de dependencias de la aplicación (composition root).

Aquí se construyen las implementaciones concretas y se inyectan en los
servicios que dependen de abstracciones.
"""

from functools import lru_cache

from fastapi import HTTPException

from app.config import Settings
from app.services.associate_directory import AssociateDirectory
from app.services.backend_client import RustBackendClient
from app.services.dropi_client import N8nDropiClient
from app.services.dropi_notifier import DropiNotificationService
from app.services.dropi_tracking_notifier import DropiTrackingNotificationService
from app.services.google_sheets import GoogleSheetClient
from app.services.pdf_extractor import PdfTextExtractor
from app.services.product_videos import ProductVideoLibrary
from app.services.recipient_extractor import DeepSeekRecipientExtractor
from app.services.run_guard import RunGuard
from app.services.sheet_notifier import SheetNotificationService
from app.services.shipping_notifier import ShippingNotificationService
from app.services.webhook_processor import WhatsAppWebhookProcessor
from app.whatsapp.client import WhatsAppClient


@lru_cache
def get_settings() -> Settings:
    return Settings.from_env()


@lru_cache
def get_backend_client() -> RustBackendClient:
    settings = get_settings()
    return RustBackendClient(
        base_url=settings.backend_api_url,
        api_key=settings.backend_api_key,
    )


@lru_cache
def get_associate_directory() -> AssociateDirectory:
    """Se carga una sola vez al iniciar la aplicación (primer uso)."""
    settings = get_settings()
    return AssociateDirectory.load(
        backend=get_backend_client(),
        fallback_numbers=settings.allowed_sender_numbers,
    )


@lru_cache
def get_sheet_notifier() -> SheetNotificationService:
    settings = get_settings()
    whatsapp_client = WhatsAppClient(
        access_token=settings.whatsapp_token,
        phone_number_id=settings.whatsapp_phone_id,
    )
    return SheetNotificationService(
        sheet_source=GoogleSheetClient(),
        backend=get_backend_client(),
        message_sender=whatsapp_client,
        media_uploader=whatsapp_client,
        debug_notification_number=settings.debug_notification_number,
        notification_override_number=settings.notification_override_number,
    )


def _dropi_order_source(settings: Settings) -> N8nDropiClient:
    if not settings.n8n_api_key:
        # Los webhooks de N8N rechazan las peticiones sin api-key.
        raise HTTPException(status_code=503, detail="N8N_API_KEY no está configurado")
    return N8nDropiClient(base_url=settings.dropi_n8n_url, api_key=settings.n8n_api_key)


@lru_cache
def get_product_videos() -> ProductVideoLibrary:
    """Una sola biblioteca de videos para /notify-dropi-mcp y
    /notify-dropi-tracking: su lock y su caché de media_ids son por
    instancia, y con dos las corridas simultáneas subirían el mismo video
    dos veces y se pisarían el archivo de caché."""
    settings = get_settings()
    return ProductVideoLibrary(
        media_dir=settings.media_dir,
        # Timeout amplio: subir un video (hasta 16 MB) tarda más que un envío.
        uploader=WhatsAppClient(
            access_token=settings.whatsapp_token,
            phone_number_id=settings.whatsapp_phone_id,
            timeout=120,
        ),
        cache_path=settings.media_cache_path,
        namespace=settings.whatsapp_phone_id,
    )


@lru_cache
def get_dropi_notifier() -> DropiNotificationService:
    settings = get_settings()
    return DropiNotificationService(
        order_source=_dropi_order_source(settings),
        backend=get_backend_client(),
        message_sender=WhatsAppClient(
            access_token=settings.whatsapp_token,
            phone_number_id=settings.whatsapp_phone_id,
        ),
        product_videos=get_product_videos(),
        debug_notification_number=settings.debug_notification_number,
        notification_override_number=settings.notification_override_number,
    )


@lru_cache
def get_dropi_runs() -> RunGuard:
    """Corridas de /notify-dropi-mcp en curso, por business."""
    return RunGuard()


@lru_cache
def get_dropi_tracking_notifier() -> DropiTrackingNotificationService:
    settings = get_settings()
    return DropiTrackingNotificationService(
        order_source=_dropi_order_source(settings),
        backend=get_backend_client(),
        message_sender=WhatsAppClient(
            access_token=settings.whatsapp_token,
            phone_number_id=settings.whatsapp_phone_id,
        ),
        product_videos=get_product_videos(),
        debug_notification_number=settings.debug_notification_number,
        notification_override_number=settings.notification_override_number,
    )


@lru_cache
def get_dropi_tracking_runs() -> RunGuard:
    """Corridas de /notify-dropi-tracking en curso, por business. Aparte de
    las de /notify-dropi-mcp: el job de cada 15 minutos no bloquea (ni
    queda bloqueado por) la notificación de pedidos en oficina."""
    return RunGuard()


@lru_cache
def get_webhook_processor() -> WhatsAppWebhookProcessor:
    settings = get_settings()
    backend = get_backend_client()
    directory = get_associate_directory()

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
        backend=backend,
        associate_directory=directory,
        debug_notification_number=settings.debug_notification_number,
        notification_override_number=settings.notification_override_number,
    )
    return WhatsAppWebhookProcessor(
        notifier=notifier,
        associate_directory=directory,
        backend=backend,
    )
