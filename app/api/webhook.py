from fastapi import APIRouter, BackgroundTasks, Depends, Request, Response

from app.api.dependencies import get_settings, get_webhook_processor
from app.config import Settings
from app.services.webhook_processor import WhatsAppWebhookProcessor

router = APIRouter()


@router.get("/webhook")
async def verify_webhook(
    request: Request,
    settings: Settings = Depends(get_settings),
) -> Response:
    """Verificación del webhook requerida por Meta al configurarlo."""
    mode = request.query_params.get("hub.mode")
    token = request.query_params.get("hub.verify_token")
    challenge = request.query_params.get("hub.challenge")

    if mode and token:
        if mode == "subscribe" and token == settings.verify_token:
            print("Webhook verified successfully!")
            return Response(content=challenge, status_code=200)
        return Response(content="Forbidden", status_code=403)

    return Response(content="Bad Request", status_code=400)


@router.post("/webhook")
async def receive_webhook(
    request: Request,
    background_tasks: BackgroundTasks,
    processor: WhatsAppWebhookProcessor = Depends(get_webhook_processor),
) -> dict:
    """Recibe los mensajes entrantes de WhatsApp.

    El evento se procesa en segundo plano para responder 200 OK de
    inmediato, como exige la documentación de webhooks de Meta.
    """
    data = await request.json()
    background_tasks.add_task(processor.handle_event, data)
    return {"status": "ok"}
