"""Seguimiento del envío de los pedidos de Dropi por niveles
(POST /notify-dropi-tracking, llamado por un job cada 15 minutos).

| Nivel | Estados de Dropi                       | Se envía                     |
|-------|----------------------------------------|------------------------------|
| 1     | TRACKING_LEVELS[1] (listo para envío)  | pedido_enviado_1             |
| 2     | TRACKING_LEVELS[2] (en camino)         | pedido_camino_1              |
| 3     | RECLAME EN OFICINA                     | lo maneja /notify-dropi-mcp  |

Cada guía recibe una sola notificación por nivel y los niveles nunca
retroceden: el backend guarda el nivel más alto notificado
(`guides.tracking_level`). Un pedido que se ve por primera vez en el
nivel 2 recibe solo la del nivel 2, y uno que ya se notificó en oficina
(`notification_count > 0`) no recibe ninguna de estas.
"""

from dataclasses import dataclass, field
from datetime import date

from app.core.phones import PhoneCountry
from app.models.shipping import RecipientInfo
from app.services.dropi_orders import DropiOrderNotifier, DropiRunReport
from app.whatsapp.templates.base import TemplateMessage
from app.whatsapp.templates.pedido_camino import PedidoCaminoTemplate
from app.whatsapp.templates.pedido_enviado import PedidoEnviadoTemplate

# Estados de Dropi de cada nivel, escritos exactamente como en Dropi:
# /ordenes filtra por un solo estado y distingue mayúsculas, guiones y
# espacios ("GUIA_GENERADA" con guion bajo; "GUIA GENERADA" no trae nada).
TRACKING_LEVELS: dict[int, tuple[str, ...]] = {
    1: (
        "GUIA_GENERADA",
        "PREPARADO PARA TRANSPORTADORA",
        "ENTREGADO A TRANSPORTADORA",
        "ADMITIDA",
        "RECOLECTADO",
        "MERCANCIA RECOGIDA",
    ),
    2: (
        "EN TRANSPORTE",
        "EN REPARTO",
        "DESPACHADA",
        "EN DESPACHO",
        "EN RUTA",
        "EN TERMINAL DESTINO",
        "EN BODEGA DESTINO",
        "BODEGA DESTINO",
        "EN TRANSITO",
        "EN DISTRIBUCION"
    ),
}

# Nivel de los pedidos en "reclame en oficina" (los notifica
# /notify-dropi-mcp, con su propia escalación diaria).
OFFICE_LEVEL = 3


@dataclass
class DropiTrackingReport(DropiRunReport):
    """Resumen de una corrida de seguimiento de pedidos de Dropi."""

    # Notificaciones enviadas por nivel: {1: 4, 2: 10}.
    notified: dict[int, int] = field(default_factory=dict)
    # Pedidos cuyo nivel ya estaba notificado (lo normal entre corridas).
    already_notified: int = 0


class DropiTrackingNotificationService(DropiOrderNotifier):
    """Notifica a cada cliente el avance de su pedido de Dropi antes de
    que llegue a la oficina:

    1. Por cada estado de TRACKING_LEVELS pagina /ordenes (un estado por
       consulta) hasta traer todos los pedidos del rango de fechas.
    2. Consulta en lote al backend qué guías ya existen, su nivel ya
       notificado y sus datos de pedido guardados.
    3. Envía la plantilla del nivel a las guías que aún no lo tienen
       notificado y sube su nivel en el backend.

    El detalle del pedido (/orden) solo se pide a Dropi cuando la guía no
    lo tiene guardado (la primera vez que se ve el pedido) y se guarda en
    el backend: el nivel 2 y la notificación en oficina ya no lo piden.
    """

    def notify_tracking_levels(
        self,
        business_id: int,
        from_date: date,
        until_date: date,
        result_number: int = 100,
        start: int = 0,
    ) -> DropiTrackingReport:
        report = DropiTrackingReport()

        # Los teléfonos de Dropi llegan sin indicativo: sin el país del
        # business no se sabe cuál ponerles.
        country = self._backend.get_business_country(business_id)
        if country is None:
            report.errors.append("backend: no se pudo consultar el país del business")
            print(f"Dropi tracking aborted for business {business_id}: country unavailable")
            return report

        orders = self._orders_by_level(business_id, from_date, until_date, result_number, start, report)
        report.total_orders = len(orders)
        print(f"Dropi tracking {from_date}..{until_date}: {len(orders)} pedido(s) en seguimiento")

        if orders:
            guides = self._backend.lookup_guides(list(orders))
            if guides is None:
                # Sin saber qué nivel tiene notificado cada guía se volverían
                # a enviar todas: mejor no enviar nada.
                report.errors.append("backend: no se pudieron consultar las guías")
            elif any("tracking_level" not in guide for guide in guides.values()):
                # Backend sin desplegar o sin migrar (db.sql): no guardaría el
                # nivel y cada corrida repetiría las notificaciones.
                report.errors.append("backend: las guías no traen tracking_level (¿falta migrar db.sql?)")
            else:
                for tracking, (level, order) in orders.items():
                    self._process_order(business_id, country, order, level, guides.get(tracking), report)

        notified = ", ".join(f"nivel {level}: {report.notified.get(level, 0)}" for level in TRACKING_LEVELS)
        print(
            f"Dropi tracking done for business {business_id}: {notified}, "
            f"{report.already_notified} ya notificados, "
            f"{len(report.skipped)} omitidos, {len(report.errors)} errores, "
            f"{report.order_lookups} consulta(s) de pedido en {report.pages} página(s)"
        )
        if report.errors:
            print(f"Dropi tracking: errores: {', '.join(report.errors)}")
        return report

    def _orders_by_level(
        self,
        business_id: int,
        from_date: date,
        until_date: date,
        result_number: int,
        start: int,
        report: DropiTrackingReport,
    ) -> dict[str, tuple[int, dict]]:
        """{guía: (nivel, pedido)} de todos los estados de seguimiento. Un
        pedido que cambió de estado entre dos consultas aparece dos veces:
        cuenta el nivel más alto."""
        found: dict[str, tuple[int, dict]] = {}
        for level, statuses in TRACKING_LEVELS.items():
            for status in statuses:
                orders = self._fetch_orders(business_id, status, from_date, until_date, result_number, start, report)
                for order in orders:
                    tracking = self._tracking(order)
                    if not tracking:
                        report.skipped.append(f"pedido {order.get('order_id') or '?'} sin guía")
                        continue
                    # Si N8N no aplicara el filtro no se notifica un pedido
                    # en otro estado (p. ej. entregado).
                    order_status = self._clean(order.get("status"))
                    if order_status and order_status.upper() != status.upper():
                        report.skipped.append(f"{tracking} en estado {order_status}")
                        continue
                    if tracking not in found or level > found[tracking][0]:
                        found[tracking] = (level, order)
        return found

    def _process_order(
        self,
        business_id: int,
        country: PhoneCountry,
        order: dict,
        level: int,
        guide: dict | None,
        report: DropiTrackingReport,
    ) -> None:
        tracking = self._tracking(order)
        if guide and guide.get("business_id") != business_id:
            # La guía ya está en los chats de otro business: notificarla
            # aquí mezclaría su historial con el de este.
            print(f"Dropi guide {tracking}: belongs to business {guide.get('business_id')}, skipping")
            report.skipped.append(tracking)
            return

        if level <= self._notified_level(guide):
            report.already_notified += 1
            return

        try:
            self._notify(business_id, country, order, level, guide, report)
        except Exception as e:
            print(f"Error processing Dropi guide {tracking}: {e}")
            report.errors.append(tracking)

    @staticmethod
    def _notified_level(guide: dict | None) -> int:
        """Nivel más alto ya notificado a la guía. Una guía ya notificada
        en oficina (notification_count > 0, por Dropi, PDF o Google Sheet)
        está en el nivel 3: no se le vuelve a decir que va en camino."""
        if not guide:
            return 0
        if (guide.get("notification_count") or 0) > 0:
            return OFFICE_LEVEL
        return int(guide.get("tracking_level") or 0)

    def _notify(
        self,
        business_id: int,
        country: PhoneCountry,
        order: dict,
        level: int,
        guide: dict | None,
        report: DropiTrackingReport,
    ) -> None:
        """Envía la plantilla del nivel. Si la guía no tiene guardados los
        datos del pedido se consultan a Dropi una única vez y se guardan
        (también los usa /notify-dropi-mcp cuando el pedido llegue a la
        oficina)."""
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
        recipient = self._build_recipient(order, details, phone, self._address_with_city(order, details))
        template = self._template(level, recipient, order, details)
        if template is None:
            print(f"Dropi guide {tracking}: incomplete order data for level {level}, skipping")
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

        sent = self._deliver(recipient, [template])
        self._register_outgoing(business_id, recipient, sent)
        if not sent:
            # Sin subir el nivel: se reintenta en la próxima corrida.
            report.errors.append(tracking)
            return

        # Si Meta reporta después que la plantilla falló, el backend baja
        # el nivel y la próxima corrida la vuelve a enviar.
        self._backend.mark_guide_tracking(tracking, level, sent[0][1])
        report.notified[level] = report.notified.get(level, 0) + 1

    def _template(
        self, level: int, recipient: RecipientInfo, order: dict, details: dict
    ) -> TemplateMessage | None:
        """Plantilla del nivel, o None si falta un dato que lleva."""
        if not (recipient.name and recipient.phone and recipient.tracking_number and recipient.product):
            return None
        if level == 1:
            amount = self._amount_to_pay(order, details)
            carrier = self._carrier(order, details)
            if amount is None or not carrier:
                return None
            return PedidoEnviadoTemplate(recipient, amount, carrier)
        if level == 2:
            if not recipient.delivery_address:
                return None
            return PedidoCaminoTemplate(recipient)
        raise ValueError(f"Nivel de seguimiento sin plantilla: {level}")
