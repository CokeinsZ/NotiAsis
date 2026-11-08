import math
import re
import unicodedata
from dataclasses import dataclass, field
from datetime import date

from app.core.interfaces import DropiOrderSource, MessageSender, NotificationBackend, ProductVideoSource
from app.core.phones import PhoneCountry, to_international
from app.models.shipping import RecipientInfo
from app.services.notification_policy import NotificationStep, daily_step
from app.whatsapp.templates.base import TemplateMessage
from app.whatsapp.templates.mensaje_guia import MensajeGuiaTemplate
from app.whatsapp.templates.pedido_oficina import PedidoOficinaTemplate
from app.whatsapp.templates.recordatorio import RecordatorioTemplate
from app.whatsapp.templates.recordatorio_final import RecordatorioFinalTemplate
from app.whatsapp.templates.video_producto import VideoProductoTemplate

# Estado de Dropi que se notifica (filtro de /ordenes).
OFFICE_STATUS = "RECLAME EN OFICINA"

# Pedido ya pagado: la transportadora no cobra nada al entregarlo.
PREPAID_RATE_TYPE = "SIN RECAUDO"

# Tope de páginas de /ordenes por corrida: protege contra un webhook que
# ignore `start` y devuelva siempre una página llena.
MAX_ORDER_PAGES = 100

# Longitudes máximas que acepta el backend (POST /guides): un valor más
# largo haría fallar el registro de la guía completa.
FIELD_LIMITS = {
    "order_id": 32,
    "customer_name": 255,
    "customer_document": 32,
    "delivery_address": 1000,
    "city": 128,
    "shipping_company": 128,
    "currency": 8,
    "product_name": 512,
}


@dataclass
class DropiNotificationReport:
    """Resumen de una corrida de notificación de pedidos de Dropi."""

    total_orders: int = 0
    pages: int = 0
    # Consultas a /orden: el MCP de Dropi tiene rate limit.
    order_lookups: int = 0
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
    skipped: list[str] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)


class DropiNotificationService:
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
        self._order_source = order_source
        self._backend = backend
        self._message_sender = message_sender
        self._product_videos = product_videos
        self._debug_notification_number = debug_notification_number
        self._notification_override_number = notification_override_number

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

        orders = self._fetch_orders(business_id, from_date, until_date, result_number, start, report)
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

    def _fetch_orders(
        self,
        business_id: int,
        from_date: date,
        until_date: date,
        result_number: int,
        start: int,
        report: DropiNotificationReport,
    ) -> list[dict]:
        """Mientras una página llegue llena (count == result_number) quedan
        más pedidos: se pide la siguiente con start += count."""
        orders: list[dict] = []
        seen: set[str] = set()
        offset = start

        for _ in range(MAX_ORDER_PAGES):
            try:
                items, count = self._order_source.fetch_orders(
                    business_id, OFFICE_STATUS, from_date, until_date, result_number, offset
                )
            except Exception as e:
                # Se procesan los pedidos ya obtenidos; el resto queda para
                # la próxima corrida.
                print(f"Error fetching Dropi orders (start={offset}): {e}")
                report.errors.append(f"ordenes start={offset}")
                return orders

            report.pages += 1
            new_items = [item for item in items if self._order_key(item) not in seen]
            seen.update(self._order_key(item) for item in items)
            orders.extend(new_items)

            if count < result_number:
                return orders
            if not new_items:
                print(f"WARNING: Dropi orders page start={offset} only repeated orders; stopping")
                report.errors.append(f"ordenes start={offset}: página repetida")
                return orders
            offset += count

        print(f"WARNING: reached {MAX_ORDER_PAGES} pages of Dropi orders; stopping")
        report.errors.append(f"ordenes: tope de {MAX_ORDER_PAGES} páginas")
        return orders

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

        recipient = self._build_recipient(order, details, self._phone(order, guide, detail_phone, country))
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

    def _fetch_details(
        self, business_id: int, order: dict, report: DropiNotificationReport
    ) -> tuple[dict, str] | None:
        """Consulta /orden y arma los datos del pedido en el formato del
        backend. Retorna (datos, teléfono del cliente) o None si falla."""
        order_id = self._clean(order.get("order_id"), "order_id")
        if not order_id:
            print(f"Dropi guide {self._tracking(order)}: order without order_id")
            return None

        report.order_lookups += 1
        detail = self._order_source.fetch_order(business_id, order_id)
        if not detail:
            return None

        customer = detail.get("customer") or {}
        products = []
        for item in detail.get("items") or []:
            name = self._clean(item.get("product_name"), "product_name")
            if name:
                products.append({
                    "product_id": _as_int(item.get("product_id")),
                    "product_name": name,
                    "quantity": _as_int(item.get("qty")) or 1,
                })

        details = {
            "order_id": order_id,
            "customer_name": self._clean(customer.get("name") or order.get("customer_name"), "customer_name") or None,
            "customer_document": self._clean(customer.get("document"), "customer_document") or None,
            "delivery_address": self._clean(customer.get("address"), "delivery_address") or None,
            "city": self._clean(customer.get("city") or order.get("city"), "city") or None,
            "shipping_company": self._clean(order.get("shipping_company"), "shipping_company") or None,
            "total": _as_float(detail.get("total", order.get("total"))),
            "currency": self._clean(detail.get("currency") or order.get("currency"), "currency") or None,
            "products": products,
        }
        return details, self._clean(customer.get("phone"))

    @staticmethod
    def _stored_details(guide: dict | None) -> dict:
        """Datos del pedido guardados en el backend (mismo formato que
        los que se le envían en register_guide)."""
        if not guide:
            return {}
        return {**(guide.get("details") or {}), "products": guide.get("products") or []}

    @staticmethod
    def _has_order_data(details: dict) -> bool:
        return bool(details.get("products") and (details.get("delivery_address") or details.get("city")))

    def _build_recipient(self, order: dict, details: dict, phone: str) -> RecipientInfo:
        products = [p["product_name"] for p in details.get("products") or [] if p.get("product_name")]
        return RecipientInfo(
            name=self._clean(details.get("customer_name") or order.get("customer_name")),
            phone=phone,
            tracking_number=self._tracking(order),
            delivery_address=self._full_address(order, details),
            product=", ".join(products),
        )

    def _full_address(self, order: dict, details: dict) -> str:
        """Dirección, ciudad y transportadora, sin repetir lo que la
        dirección ya menciona: "Oficina Interrapidisimo, TARAZA" o
        "ENVIA - Cra 5 # 10-20, CALI"."""
        address = self._clean(details.get("delivery_address"))
        city = self._clean(details.get("city") or order.get("city"))
        carrier = self._clean(details.get("shipping_company") or order.get("shipping_company"))
        if city and _compact(city) not in _compact(address):
            address = f"{address}, {city}" if address else city
        if carrier and _compact(carrier) not in _compact(address):
            address = f"{carrier} - {address}" if address else carrier
        return address

    def _amount_to_pay(self, order: dict, details: dict) -> str | None:
        """Valor a cobrar en la oficina con separador de miles ("84.900").
        El total de /ordenes es el más reciente; si no viene, el guardado."""
        if self._clean(order.get("rate_type")).upper() == PREPAID_RATE_TYPE:
            return "0"
        total = _as_float(order.get("total"))
        if total is None:
            total = _as_float(details.get("total"))
        if total is None:
            return None
        return f"{round(total):,}".replace(",", ".")

    def _phone(self, order: dict, guide: dict | None, detail_phone: str, country: PhoneCountry) -> str:
        """El teléfono con el que ya existe el chat tiene prioridad. Los de
        Dropi llegan sin indicativo: se les pone el del país del business."""
        phone = (guide or {}).get("user_id") or order.get("customer_phone") or detail_phone
        return to_international(phone, country)

    def _deliver(
        self,
        recipient: RecipientInfo,
        templates: list[TemplateMessage],
    ) -> list[tuple[TemplateMessage, str]]:
        """Envía las plantillas en orden y retorna las enviadas con sus meta_ids.

        Con NOTIFICATION_OVERRIDE_NUMBER configurado, el envío físico se
        desvía a ese número (pruebas); el historial siempre se registra
        con el teléfono real del destinatario. La copia de depuración
        (DEBUG_NOTIFICATION_NUMBER) se envía además si está configurada.
        """
        target_number = self._notification_override_number or recipient.phone
        if target_number != recipient.phone:
            print(f"[OVERRIDE] {recipient.tracking_number}: envío desviado a {target_number}")

        sent: list[tuple[TemplateMessage, str]] = []
        for template in templates:
            meta_message_id = self._message_sender.send_template(target_number, template)
            if meta_message_id:
                sent.append((template, meta_message_id))

        debug_number = self._debug_notification_number
        if debug_number and debug_number != target_number:
            for template in templates:
                self._message_sender.send_template(debug_number, template)

        return sent

    def _register_outgoing(
        self,
        business_id: int,
        recipient: RecipientInfo,
        sent: list[tuple[TemplateMessage, str]],
    ) -> None:
        for template, meta_message_id in sent:
            self._backend.register_outgoing_message(
                business_id=business_id,
                user_phone=recipient.phone,
                user_name=recipient.name,
                meta_message_id=meta_message_id,
                media_type=template.media_type(),
                message=template.log_message(),
                media_id=template.log_media_id(),
                guide_number=recipient.tracking_number,
            )

    @classmethod
    def _tracking(cls, order: dict) -> str:
        return cls._clean(order.get("tracking_code"))

    @classmethod
    def _order_key(cls, order: dict) -> str:
        return cls._clean(order.get("order_id")) or cls._tracking(order) or str(id(order))

    @staticmethod
    def _clean(value, limit_key: str | None = None) -> str:
        """Texto en una sola línea: Meta rechaza parámetros de plantilla
        con saltos de línea, tabs o más de 4 espacios seguidos."""
        text = re.sub(r"\s+", " ", str(value or "")).strip()
        if limit_key:
            text = text[:FIELD_LIMITS[limit_key]]
        return text


def _compact(text: str) -> str:
    """Solo letras y números, sin tildes ni mayúsculas: para comparar
    "Interrapidísimo" con "INTER RAPIDISIMO"."""
    plain = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode().lower()
    return re.sub(r"[^a-z0-9]", "", plain)


def _as_int(value) -> int | None:
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def _as_float(value) -> float | None:
    try:
        number = float(value)
    except (TypeError, ValueError):
        return None
    # NaN/inf no son JSON válido para el backend.
    return number if math.isfinite(number) else None
