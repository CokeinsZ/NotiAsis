import math
import re
from dataclasses import dataclass, field
from datetime import date

from app.core.interfaces import DropiOrderSource, MessageSender, NotificationBackend
from app.models.shipping import RecipientInfo
from app.services.notification_policy import NotificationStep, step_for_notification_count
from app.whatsapp.templates.base import TemplateMessage
from app.whatsapp.templates.mensaje_guia import MensajeGuiaTemplate
from app.whatsapp.templates.recordatorio import RecordatorioTemplate
from app.whatsapp.templates.recordatorio_final import RecordatorioFinalTemplate

# Tope de páginas de /oficina por corrida: protege contra un webhook que
# ignore `start` y devuelva siempre una página llena.
MAX_OFFICE_PAGES = 100

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
    new_notifications: int = 0
    reminders: int = 0
    final_reminders: int = 0
    skipped: list[str] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)


class DropiNotificationService:
    """Notificación de los pedidos de Dropi en "reclame en oficina".

    1. Pagina /oficina hasta traer todos los pedidos del rango de fechas.
    2. Consulta en lote al backend qué guías (tracking_code) ya existen,
       cuántas veces se notificaron y qué datos de pedido tienen guardados.
    3. Aplica la escalación sin PDF:
       0 -> mensaje_guia_es, 1 -> recordatorio, 2 -> recordatorio_final.

    El detalle del pedido (/orden) solo se pide a Dropi cuando faltan
    datos que la plantilla necesita (en la práctica, la primera
    notificación) y se guarda en el backend para los recordatorios.
    """

    def __init__(
        self,
        order_source: DropiOrderSource,
        backend: NotificationBackend,
        message_sender: MessageSender,
        debug_notification_number: str | None = None,
        notification_override_number: str | None = None,
    ) -> None:
        self._order_source = order_source
        self._backend = backend
        self._message_sender = message_sender
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

        orders = self._fetch_office_orders(from_date, until_date, result_number, start, report)
        orders = self._unique_by_tracking(orders, report)
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
                    self._process_order(business_id, order, guides.get(self._tracking(order)), report)

        print(
            f"Dropi notification done for business {business_id}: "
            f"{report.new_notifications} nuevas, {report.reminders} recordatorios, "
            f"{report.final_reminders} recordatorios finales, "
            f"{len(report.skipped)} omitidas, {len(report.errors)} errores, "
            f"{report.order_lookups} consulta(s) de pedido en {report.pages} página(s)"
        )
        return report

    def _fetch_office_orders(
        self,
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

        for _ in range(MAX_OFFICE_PAGES):
            try:
                items, count = self._order_source.fetch_office_orders(
                    from_date, until_date, result_number, offset
                )
            except Exception as e:
                # Se procesan los pedidos ya obtenidos; el resto queda para
                # la próxima corrida.
                print(f"Error fetching Dropi office orders (start={offset}): {e}")
                report.errors.append(f"oficina start={offset}")
                return orders

            report.pages += 1
            new_items = [item for item in items if self._order_key(item) not in seen]
            seen.update(self._order_key(item) for item in items)
            orders.extend(new_items)

            if count < result_number:
                return orders
            if not new_items:
                print(f"WARNING: Dropi office page start={offset} only repeated orders; stopping")
                report.errors.append(f"oficina start={offset}: página repetida")
                return orders
            offset += count

        print(f"WARNING: reached {MAX_OFFICE_PAGES} pages of Dropi office orders; stopping")
        report.errors.append(f"oficina: tope de {MAX_OFFICE_PAGES} páginas")
        return orders

    def _unique_by_tracking(self, orders: list[dict], report: DropiNotificationReport) -> list[dict]:
        """Descarta pedidos sin guía y repetidos: una notificación por guía."""
        unique: dict[str, dict] = {}
        for order in orders:
            tracking = self._tracking(order)
            if not tracking:
                report.skipped.append(f"pedido {order.get('order_id') or '?'} sin guía")
                continue
            unique.setdefault(tracking, order)
        return list(unique.values())

    def _process_order(
        self,
        business_id: int,
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
            step = step_for_notification_count(count, last_notified)

            if step is NotificationStep.STOP:
                report.skipped.append(tracking)
            elif step is NotificationStep.INITIAL:
                self._send_initial(business_id, order, guide, report)
            else:
                self._send_reminder(business_id, order, guide, step, report)
        except Exception as e:
            print(f"Error processing Dropi guide {tracking}: {e}")
            report.errors.append(tracking)

    def _send_initial(
        self,
        business_id: int,
        order: dict,
        guide: dict | None,
        report: DropiNotificationReport,
    ) -> None:
        """mensaje_guia_es. Si la guía ya existe con sus datos (un envío
        anterior falló) no se consulta a Dropi ni se vuelve a registrar."""
        tracking = self._tracking(order)
        details = self._stored_details(guide)
        detail_phone = ""

        fetched = not self._has_initial_data(details)
        if fetched:
            result = self._fetch_details(order, report)
            if result is None:
                report.errors.append(tracking)
                return
            details, detail_phone = result

        recipient = self._build_recipient(order, details, self._phone(order, guide, detail_phone))
        if not recipient.is_complete:
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

        sent = self._deliver(recipient, [MensajeGuiaTemplate(recipient, media_id="")])
        if not sent:
            report.errors.append(tracking)
            return

        self._register_outgoing(business_id, recipient, sent)
        self._backend.mark_guide_notified(tracking)
        report.new_notifications += 1

    def _send_reminder(
        self,
        business_id: int,
        order: dict,
        guide: dict,
        step: NotificationStep,
        report: DropiNotificationReport,
    ) -> None:
        """recordatorio no lleva variables; recordatorio_final necesita el
        producto, que sale de los datos guardados. Solo una guía registrada
        antes de guardar esos datos (flujos de PDF o sheet) consulta el
        pedido a Dropi, una única vez, y lo guarda."""
        tracking = self._tracking(order)
        details = self._stored_details(guide)
        detail_phone = ""

        if step is NotificationStep.FINAL_REMINDER and not details.get("products"):
            result = self._fetch_details(order, report)
            if result is None:
                report.errors.append(tracking)
                return
            details, detail_phone = result
            self._backend.register_guide(
                tracking,
                self._phone(order, guide, detail_phone),
                details.get("customer_name") or "",
                business_id,
                details=details,
            )

        recipient = self._build_recipient(order, details, self._phone(order, guide, detail_phone))
        if step is NotificationStep.REMINDER:
            template: TemplateMessage = RecordatorioTemplate(recipient)
        else:
            if not (recipient.name and recipient.product):
                print(f"Dropi guide {tracking}: missing name/product for recordatorio_final, skipping")
                report.skipped.append(tracking)
                return
            template = RecordatorioFinalTemplate(recipient)

        sent = self._deliver(recipient, [template])
        if not sent:
            report.errors.append(tracking)
            return

        self._register_outgoing(business_id, recipient, sent)
        self._backend.mark_guide_notified(tracking)
        if step is NotificationStep.REMINDER:
            report.reminders += 1
        else:
            report.final_reminders += 1

    def _fetch_details(self, order: dict, report: DropiNotificationReport) -> tuple[dict, str] | None:
        """Consulta /orden y arma los datos del pedido en el formato del
        backend. Retorna (datos, teléfono del cliente) o None si falla."""
        order_id = self._clean(order.get("order_id"), "order_id")
        if not order_id:
            print(f"Dropi guide {self._tracking(order)}: order without order_id")
            return None

        report.order_lookups += 1
        detail = self._order_source.fetch_order(order_id)
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
    def _has_initial_data(details: dict) -> bool:
        return bool(details.get("products") and (details.get("delivery_address") or details.get("city")))

    def _build_recipient(self, order: dict, details: dict, phone: str) -> RecipientInfo:
        address = self._clean(details.get("delivery_address"))
        city = self._clean(details.get("city") or order.get("city"))
        if city and city.lower() not in address.lower():
            address = f"{address}, {city}" if address else city

        products = [p["product_name"] for p in details.get("products") or [] if p.get("product_name")]
        return RecipientInfo(
            name=self._clean(details.get("customer_name") or order.get("customer_name")),
            phone=phone,
            tracking_number=self._tracking(order),
            delivery_address=address,
            product=", ".join(products),
        )

    def _phone(self, order: dict, guide: dict | None, detail_phone: str) -> str:
        """El teléfono con el que ya existe el chat tiene prioridad."""
        phone = (guide or {}).get("user_id") or order.get("customer_phone") or detail_phone
        return re.sub(r"[^\d+]", "", str(phone or ""))

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
