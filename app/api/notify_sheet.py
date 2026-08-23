from fastapi import APIRouter, BackgroundTasks, Depends
from pydantic import BaseModel

from app.api.dependencies import get_sheet_notifier
from app.services.sheet_notifier import SheetNotificationService

router = APIRouter()


class NotifySheetRequest(BaseModel):
    business_id: int


@router.post("/notify-sheet", status_code=202)
async def notify_sheet(
    body: NotifySheetRequest,
    background_tasks: BackgroundTasks,
    notifier: SheetNotificationService = Depends(get_sheet_notifier),
) -> dict:
    """Dispara la notificación masiva desde el Google Sheet del business.

    Se procesa en segundo plano (descargar las hojas, cruzar guías y
    enviar plantillas puede tardar varios minutos).
    """
    background_tasks.add_task(notifier.notify_business_sheet, body.business_id)
    return {"status": "processing", "business_id": body.business_id}
