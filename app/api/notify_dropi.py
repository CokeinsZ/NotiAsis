from datetime import date, timedelta

from fastapi import APIRouter, BackgroundTasks, Depends
from pydantic import BaseModel, ConfigDict, Field, model_validator

from app.api.dependencies import get_dropi_notifier
from app.services.dropi_notifier import DropiNotificationService

router = APIRouter()

DEFAULT_LOOKBACK_DAYS = 15


def _default_from() -> date:
    return date.today() - timedelta(days=DEFAULT_LOOKBACK_DAYS)


class NotifyDropiRequest(BaseModel):
    """Filtros de la consulta a Dropi; todos opcionales."""

    model_config = ConfigDict(populate_by_name=True)

    from_date: date = Field(default_factory=_default_from, alias="from")
    until: date = Field(default_factory=date.today)
    result_number: int = Field(default=100, ge=1)
    start: int = Field(default=0, ge=0)

    @model_validator(mode="after")
    def _check_range(self) -> "NotifyDropiRequest":
        if self.from_date > self.until:
            raise ValueError("'from' no puede ser posterior a 'until'")
        return self


@router.post("/notify-dropi-mcp", status_code=202)
async def notify_dropi_mcp(
    background_tasks: BackgroundTasks,
    body: NotifyDropiRequest | None = None,
    notifier: DropiNotificationService = Depends(get_dropi_notifier),
) -> dict:
    """Notifica los pedidos de Dropi que están en "reclame en oficina".

    Se procesa en segundo plano (paginar los pedidos, consultar el
    detalle de los nuevos y enviar plantillas puede tardar varios minutos).
    """
    filters = body or NotifyDropiRequest()
    background_tasks.add_task(
        notifier.notify_office_orders,
        filters.from_date,
        filters.until,
        filters.result_number,
        filters.start,
    )
    return {
        "status": "processing",
        "from": filters.from_date.isoformat(),
        "until": filters.until.isoformat(),
        "result_number": filters.result_number,
        "start": filters.start,
    }
