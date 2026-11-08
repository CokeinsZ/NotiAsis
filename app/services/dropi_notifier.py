from dataclasses import dataclass, field
from datetime import date

from app.core.interfaces import DropiOrderSource, MessageSender, NotificationBackend, ProductVideoSource
from app.core.phones import PhoneCountry
from app.models.shipping import RecipientInfo
from app.services.dropi_orders import DropiOrderNotifier, DropiRunReport
from app.services.notification_policy import NotificationStep, daily_step
from app.whatsapp.templates.base import TemplateMessage
from app.whatsapp.templates.mensaje_guia import MensajeGuiaTemplate
from app.whatsapp.templates.pedido_oficina import PedidoOficinaTemplate
from app.whatsapp.templates.recordatorio import RecordatorioTemplate
from app.whatsapp.templates.recordatorio_final import RecordatorioFinalTemplate
from app.whatsapp.templates.video_producto import VideoProductoTemplate

# Estado de Dropi que se notifica (filtro de /ordenes).
OFFICE_STATUS = "RECLAME EN OFICINA"


@dataclass
class DropiNotificationReport(DropiRunReport):
    """Resumen de una corrida de notificación de pedidos de Dropi en oficina."""

    # Día 1 (video_producto + mensaje_guia).
    new_notifications: int = 0
    # Día 2 (recordatorio).
    reminders: int = 0
    # Día 3 (recordatorio_final).
    final_reminders: int = 0
    # Día 4 en adelante (pedido_oficina).
    daily_reminders: int = 0
    videos: int = 0
    # Primeras notificaciones que salieron sin video.
    without_video: list[str] = field(default_factory=list)


class DropiNotificationService(DropiOrderNotifier):
    """Notificación diaria de los pedidos de Dropi en "reclame en oficina".

    1. Pagina /ordenes (estado OFFICE_STATUS) hasta traer todos los
       pedidos del rango de fechas.
    2. Consulta en lote al backend qué guías (tracking_code) ya existen,
       cuántas veces se notificaron y qué datos de pedido tienen guardados.
    3. Una notificación por guía por día, sin tope: el pedido deja de
       notificarse cuando sale de la oficina (ya no aparece en /ordenes).
       - Día 1: video_producto (si el producto tiene video) + mensaje_guia.
       - Día 2: recordatorio.
       - Día 3: recordatorio_final.
       - Día 4 en adelante: pedido_oficina.

    El detalle del pedido (/orden) solo se pide a Dropi cuando faltan
    datos que la plantilla necesita (en la práctica, la primera
    notificación) y se guarda en el backend para los días siguientes.
    """

    def __init__(
        self,
        order_source: DropiOrderSource,
        backend: NotificationBackend,
        message_sender: MessageSender,
        product_videos: ProductVideoSource,
        debug_notification_number: str | None = None,
        notification_override_number: str | None = None,
    ) -> None:
        super().__init__(
            order_source=order_source,
            backend=backend,
            message_sender=message_sender,
            debug_notification_number=debug_notification_number,
            notification_override_number=notification_override_number,
        )
        self._product_videos = product_videos

    def notify_office_orders(
        self,
        business_id: int,
        from_date: date,
        until_date: date,
        result_number: int = 100,
        start: int = 0,
    ) -> DropiNotificationReport:
        report = DropiNotificationReport()  #Objeto para acumular el resumen de la ejecución

        # Los teléfonos de Dropi llegan sin indicativo: sin el país del
        # business no se sabe cuál ponerles.
        country = self._backend.get_business_country(business_id)
        if country is None:
            report.errors.append("backend: no se pudo consultar el país del business")
            print(f"Dropi notification aborted for business {business_id}: country unavailable")
            return report

        orders = self._fetch_orders(business_id, OFFICE_STATUS, from_date, until_date, result_number, start, report)
        orders = self._notifiable_orders(orders, report)
        report.total_orders = len(orders)
        print(f"Dropi {from_date}..{until_date}: {len(orders)} pedido(s) en oficina")

        if orders:
            guides = self._backend.lookup_guides([self._tracking(order) for order in orders])
            if guides is None:
                # Sin saber cuántas veces se notificó cada guía se re-enviaría
                # la notificación inicial a todos: mejor no enviar nada.
                report.errors.append("backend: no se pudieron consultar las guías")
            else:
                for order in orders:
                    self._process_order(business_id, country, order, guides.get(self._tracking(order)), report)

        print(
            f"Dropi notification done for business {business_id}: "
            f"{report.new_notifications} nuevas ({report.videos} con video), "
            f"{report.reminders} recordatorios, {report.final_reminders} recordatorios finales, "
            f"{report.daily_reminders} pedido_oficina, "
            f"{len(report.skipped)} omitidas, {len(report.errors)} errores, "
            f"{report.order_lookups} consulta(s) de pedido en {report.pages} página(s)"
        )
        if report.without_video:
            print(f"Dropi: primera notificación sin video: {', '.join(report.without_video)}")
        if report.errors:
            print(f"Dropi: errores: {', '.join(report.errors)}")
        return report

    def _notifiable_orders(self, orders: list[dict], report: DropiNotificationReport) -> list[dict]:
        """Descarta pedidos sin guía, repetidos (una notificación por guía)
        y los que vengan en otro estado (si N8N no aplicara el filtro)."""
        unique: dict[str, dict] = {}
        for order in orders:
            tracking = self._tracking(order)
            if not tracking:
                report.skipped.append(f"pedido {order.get('order_id') or '?'} sin guía")
                continue
            status = self._clean(order.get("status"))
            if status and status.upper() != OFFICE_STATUS:
                report.skipped.append(f"{tracking} en estado {status}")
                continue
            unique.setdefault(tracking, order)
        return list(unique.values())

    def _process_order(
        self,
        business_id: int,
        country: PhoneCountry,
        order: dict,
        guide: dict | None,
        report: DropiNotificationReport,
    ) -> None:
        tracking = self._tracking(order)
        if guide and guide.get("business_id") != business_id:
            # La guía ya está en los chats de otro business: notificarla
            # aquí mezclaría su historial con el de este.
            print(f"Dropi guide {tracking}: belongs to business {guide.get('business_id')}, skipping")
            report.skipped.append(tracking)
            return

        try:
            count = (guide.get("notification_count") or 0) if guide else 0
            last_notified = guide.get("last_notification_timestamp") if guide else None
            step = daily_step(count, last_notified)

            if step is NotificationStep.STOP:
                report.skipped.append(tracking)
            else:
                self._notify(business_id, country, order, guide, step, report)
        except Exception as e:
            print(f"Error processing Dropi guide {tracking}: {e}")
            report.errors.append(tracking)

    def _notify(
        self,
        business_id: int,
        country: PhoneCountry,
        order: dict,
        guide: dict | None,
        step: NotificationStep,
        report: DropiNotificationReport,
    ) -> None:
        """Envía las plantillas del paso (ver _templates). Si la guía no
        tiene guardados los datos del pedido (es nueva, o la registró otro
        flujo) se consultan a Dropi una única vez y se guardan. Si ya los
        tiene (un envío anterior falló) no se consulta a Dropi ni se
        vuelve a registrar."""
        tracking = self._tracking(order)
        details = self._stored_details(guide)
        detail_phone = ""

        fetched = not self._has_order_data(details)
        if fetched:
            result = self._fetch_details(business_id, order, report)
            if result is None:
                report.errors.append(tracking)
                return
            details, detail_phone = result

        phone = self._phone(order, guide, detail_phone, country)
        recipient = self._build_recipient(order, details, phone, self._full_address(order, details))
        amount = self._amount_to_pay(order, details)
        # El valor a pagar solo va en pedido_oficina.
        if not recipient.is_complete or (amount is None and step is NotificationStep.DAILY_REMINDER):
            print(f"Dropi guide {tracking}: incomplete order data, skipping")
            report.skipped.append(tracking)
            return

        if fetched:
            created = self._backend.register_guide(
                tracking, recipient.phone, recipient.name, business_id, details=details
            )
            if guide is None and not created:
                # Carrera rara: otra corrida la registró entre la consulta y ahora.
                report.skipped.append(tracking)
                return

        video = None
        if step is NotificationStep.INITIAL:
            video = self._product_videos.video_for(business_id, details.get("products") or [])
        templates = self._templates(step, recipient, amount, video)

        sent = self._deliver(recipient, templates)
        self._register_outgoing(business_id, recipient, sent)
        sent_names = {template.name for template, _ in sent}

        video_sent = VideoProductoTemplate.TEMPLATE_NAME in sent_names
        if video and not video_sent:
            # Meta rechazó el media_id (p. ej. se borró antes de vencer):
            # se vuelve a subir en la próxima primera notificación.
            self._product_videos.discard(video[0])

        # La notificación del día es la última plantilla: sin ella la guía
        # no cuenta como notificada y se reintenta en la próxima corrida.
        if templates[-1].name not in sent_names:
            report.errors.append(tracking)
            return

        self._backend.mark_guide_notified(tracking)
        if step is NotificationStep.REMINDER:
            report.reminders += 1
        elif step is NotificationStep.FINAL_REMINDER:
            report.final_reminders += 1
        elif step is NotificationStep.DAILY_REMINDER:
            report.daily_reminders += 1
        else:
            report.new_notifications += 1
            if video_sent:
                report.videos += 1
            else:
                report.without_video.append(tracking)

    @staticmethod
    def _templates(
        step: NotificationStep,
        recipient: RecipientInfo,
        amount: str | None,
        video: tuple[str, str] | None,
    ) -> list[TemplateMessage]:
        """Plantillas del paso, en orden de envío."""
        if step is NotificationStep.INITIAL:
            intro = [VideoProductoTemplate(video[0])] if video else []
            return intro + [MensajeGuiaTemplate(recipient)]
        if step is NotificationStep.REMINDER:
            return [RecordatorioTemplate(recipient)]
        if step is NotificationStep.FINAL_REMINDER:
            return [RecordatorioFinalTemplate(recipient)]
        return [PedidoOficinaTemplate(recipient, amount)]


