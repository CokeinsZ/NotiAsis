from datetime import date, timedelta

from fastapi import APIRouter, BackgroundTasks, Depends, HTTPException, Query
from pydantic import BaseModel, ConfigDict, Field, model_validator

from app.api.dependencies import get_backend_client, get_dropi_notifier, get_dropi_runs
from app.core.interfaces import NotificationBackend
from app.services.dropi_notifier import DropiNotificationService
from app.services.run_guard import RunGuard

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
def notify_dropi_mcp(
    background_tasks: BackgroundTasks,
    business_id: int = Query(gt=0, description="Business dueño de los chats e historial"),
    body: NotifyDropiRequest | None = None,
    notifier: DropiNotificationService = Depends(get_dropi_notifier),
    backend: NotificationBackend = Depends(get_backend_client),
    runs: RunGuard = Depends(get_dropi_runs),
) -> dict:
    """Notifica los pedidos de Dropi que están en "reclame en oficina".

    Se procesa en segundo plano (paginar los pedidos, consultar el
    detalle de los nuevos y enviar plantillas puede tardar varios minutos).
    El business se verifica antes: con uno inexistente no se registraría
    nada y cada corrida volvería a enviar la notificación inicial.

    Una sola corrida por business a la vez (409 mientras la anterior no
    termina): dos corridas simultáneas verían las mismas guías sin
    notificar y les enviarían la plantilla dos veces.
    """
    exists = backend.business_exists(business_id)
    if exists is None:
        raise HTTPException(status_code=503, detail="No se pudo verificar el business en el backend")
    if not exists:
        raise HTTPException(status_code=404, detail=f"El business {business_id} no existe")

    if not runs.try_start(business_id):
        raise HTTPException(
            status_code=409,
            detail=f"Ya hay una notificación de Dropi en curso para el business {business_id}",
        )

    filters = body or NotifyDropiRequest()
    background_tasks.add_task(
        _run,
        notifier,
        runs,
        business_id,
        filters.from_date,
        filters.until,
        filters.result_number,
        filters.start,
    )
    return {
        "status": "processing",
        "business_id": business_id,
        "from": filters.from_date.isoformat(),
        "until": filters.until.isoformat(),
        "result_number": filters.result_number,
        "start": filters.start,
    }


def _run(notifier: DropiNotificationService, runs: RunGuard, business_id: int, *filters) -> None:
    """Libera el business al terminar, aunque la corrida falle."""
    try:
        notifier.notify_office_orders(business_id, *filters)
    finally:
        runs.finish(business_id)
