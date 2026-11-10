"""Base compartida de las notificaciones de pedidos de Dropi (MCP vía N8N).

- POST /notify-dropi-mcp: pedidos en "reclame en oficina"
  (`DropiNotificationService`, app/services/dropi_notifier.py).
- POST /notify-dropi-tracking: seguimiento del envío por niveles
  (`DropiTrackingNotificationService`, app/services/dropi_tracking_notifier.py).

Ambas paginan /ordenes por estado, piden el detalle del pedido (/orden,
con rate limit) solo cuando la guía no lo tiene guardado en el backend, y
envían plantillas registrándolas en el historial del chat. Antes de enviar,
cada corrida reserva el cupo de mensajería de sus clientes (RunQuota).
"""

import math
import re
import unicodedata
from dataclasses import dataclass, field
from datetime import date

from app.core.interfaces import DropiOrderSource, MessageSender, NotificationBackend, ProductVideoSource
from app.core.phones import PhoneCountry, to_international
from app.models.shipping import RecipientInfo
from app.services.quota import RunQuota
from app.whatsapp.templates.base import TemplateMessage
from app.whatsapp.templates.video_producto import VideoProductoTemplate

# Pedido ya pagado: la transportadora no cobra nada al entregarlo.
PREPAID_RATE_TYPE = "SIN RECAUDO"

# Tope de páginas de /ordenes por estado y corrida: protege contra un
# webhook que ignore `start` y devuelva siempre una página llena.
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
class DropiRunReport:
    """Lo común del resumen de una corrida de notificación de Dropi."""

    total_orders: int = 0
    pages: int = 0
    # Consultas a /orden: el MCP de Dropi tiene rate limit.
    order_lookups: int = 0
    skipped: list[str] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)
    # Sin cupo de mensajería hoy: no se enviaron ni se marcaron, la próxima
    # corrida los vuelve a intentar.
    deferred: list[str] = field(default_factory=list)


class DropiOrderNotifier:
    """Consulta de pedidos de Dropi y envío de sus plantillas, compartidos
    por las notificaciones de Dropi."""

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

    # ---------------- Pedidos de Dropi ----------------

    def _fetch_orders(
        self,
        business_id: int,
        status: str,
        from_date: date,
        until_date: date,
        result_number: int,
        start: int,
        report: DropiRunReport,
    ) -> list[dict]:
        """Pedidos en `status` (/ordenes acepta un solo estado por consulta).
        Mientras una página llegue llena (count == result_number) quedan
        más pedidos: se pide la siguiente con start += count."""
        orders: list[dict] = []
        seen: set[str] = set()
        offset = start

        for _ in range(MAX_ORDER_PAGES):
            try:
                items, count = self._order_source.fetch_orders(
                    business_id, status, from_date, until_date, result_number, offset
                )
            except Exception as e:
                # Se procesan los pedidos ya obtenidos; el resto queda para
                # la próxima corrida.
                print(f"Error fetching Dropi orders {status!r} (start={offset}): {e}")
                report.errors.append(f"ordenes {status} start={offset}")
                return orders

            report.pages += 1
            new_items = [item for item in items if self._order_key(item) not in seen]
            seen.update(self._order_key(item) for item in items)
            orders.extend(new_items)

            if count < result_number:
                return orders
            if not new_items:
                print(f"WARNING: Dropi orders {status!r} page start={offset} only repeated orders; stopping")
                report.errors.append(f"ordenes {status} start={offset}: página repetida")
                return orders
            offset += count

        print(f"WARNING: reached {MAX_ORDER_PAGES} pages of Dropi orders {status!r}; stopping")
        report.errors.append(f"ordenes {status}: tope de {MAX_ORDER_PAGES} páginas")
        return orders

    def _fetch_details(
        self, business_id: int, order: dict, report: DropiRunReport
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

    # ---------------- Datos de las plantillas ----------------

    def _build_recipient(self, order: dict, details: dict, phone: str, address: str) -> RecipientInfo:
        products = [p["product_name"] for p in details.get("products") or [] if p.get("product_name")]
        return RecipientInfo(
            name=self._clean(details.get("customer_name") or order.get("customer_name")),
            phone=phone,
            tracking_number=self._tracking(order),
            delivery_address=address,
            product=", ".join(products),
        )

    def _address_with_city(self, order: dict, details: dict) -> str:
        """Dirección de entrega y ciudad, sin repetir la ciudad si la
        dirección ya la menciona: "Cra 5 # 10-20, CALI"."""
        address = self._clean(details.get("delivery_address"))
        city = self._clean(details.get("city") or order.get("city"))
        if city and _compact(city) not in _compact(address):
            address = f"{address}, {city}" if address else city
        return address

    def _full_address(self, order: dict, details: dict) -> str:
        """Dirección, ciudad y transportadora, sin repetir lo que la
        dirección ya menciona: "Oficina Interrapidisimo, TARAZA" o
        "ENVIA - Cra 5 # 10-20, CALI"."""
        address = self._address_with_city(order, details)
        carrier = self._carrier(order, details)
        if carrier and _compact(carrier) not in _compact(address):
            address = f"{carrier} - {address}" if address else carrier
        return address

    def _carrier(self, order: dict, details: dict) -> str:
        return self._clean(details.get("shipping_company") or order.get("shipping_company"))

    def _amount_to_pay(self, order: dict, details: dict) -> str | None:
        """Valor a cobrar al entregar con separador de miles ("84.900").
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

    # ---------------- Envío ----------------

    # ---------------- Video del producto ----------------

    @staticmethod
    def _video_already_sent(guide: dict | None) -> bool:
        """El video del producto va una sola vez por guía: en el nivel 1 del
        seguimiento o, si no lo recibió ahí, en el aviso inicial en oficina.
        `video_sent` lo calcula el backend (un video saliente de la guía que
        no falló: si falló, se vuelve a enviar)."""
        return bool(guide and guide.get("video_sent"))

    def _product_video(self, business_id: int, guide: dict | None, details: dict) -> tuple[str, str] | None:
        """(media_id, archivo) del video a enviar, o None si el producto no
        tiene video o la guía ya lo recibió."""
        if self._video_already_sent(guide):
            return None
        return self._product_videos.video_for(business_id, details.get("products") or [])

    def _video_was_sent(self, video: tuple[str, str] | None, sent_names: set[str]) -> bool:
        """Si el video salió. Si Meta lo rechazó al enviarlo (p. ej. el
        media_id se borró antes de vencer), se olvida para volver a subirlo."""
        video_sent = VideoProductoTemplate.TEMPLATE_NAME in sent_names
        if video and not video_sent:
            self._product_videos.discard(video[0])
        return video_sent

    # ---------------- Cupo de mensajería ----------------

    @staticmethod
    def _reserve_quota(quota: RunQuota, phones: list[str], report: DropiRunReport) -> None:
        """Una sola reserva con los teléfonos de la corrida, en orden de
        prioridad."""
        quota.reserve(phones)
        if quota.unavailable:
            report.errors.append("backend: no se pudo reservar cupo de mensajería")

    @staticmethod
    def _within_quota(quota: RunQuota, phone: str, tracking: str, report: DropiRunReport) -> bool:
        """Sin cupo no se envía ni se marca nada: la próxima corrida lo
        reintenta. Sin teléfono no hay cupo que pedir (la guía se omite
        después por datos incompletos)."""
        if not phone or quota.allows(phone):
            return True
        report.deferred.append(tracking)
        return False

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

    # ---------------- Utilidades ----------------

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
