"""Tests del seguimiento por niveles de los pedidos de Dropi
(POST /notify-dropi-tracking).

Todo con dobles de prueba: sin N8N, sin Meta, sin backend real.
"""

import threading
from concurrent.futures import ThreadPoolExecutor
from datetime import date

import pytest
import requests
from fastapi import FastAPI
from fastapi.testclient import TestClient

from app.models.quota import QuotaReservation
from app.api.dependencies import (
    get_backend_client,
    get_dropi_notifier,
    get_dropi_runs,
    get_dropi_tracking_notifier,
    get_dropi_tracking_runs,
)
from app.api.notify_dropi import router as notify_dropi_router
from app.core.phones import PhoneCountry
from app.models.shipping import RecipientInfo
from app.services.backend_client import RustBackendClient
from app.services.dropi_tracking_notifier import TRACKING_LEVELS, DropiTrackingNotificationService
from app.services.run_guard import RunGuard
from app.whatsapp.templates.pedido_camino import PedidoCaminoTemplate
from app.whatsapp.templates.pedido_enviado import PedidoEnviadoTemplate

BUSINESS_ID = 1
FROM = date(2026, 10, 24)
UNTIL = date(2026, 11, 8)
COLOMBIA = PhoneCountry("57", 10)

LEVEL_1_STATUS = "GUIA_GENERADA"
LEVEL_2_STATUS = "EN REPARTO"


# ------------------------------ Fakes ------------------------------


class FakeDropiSource:
    """Simula /ordenes (un estado por consulta, paginado por `start`) y /orden."""

    def __init__(self, orders_by_status=None, details=None, fail_status=None):
        self.orders_by_status = orders_by_status or {}
        self.details = details if details is not None else {}
        self.fail_status = fail_status
        self.order_list_calls = []
        self.order_calls = []

    def fetch_orders(self, business_id, status, from_date, until_date, result_number, start):
        self.order_list_calls.append((status, start))
        if status == self.fail_status:
            raise Exception("N8N timeout")
        page = self.orders_by_status.get(status, [])[start:start + result_number]
        return page, len(page)

    def fetch_order(self, business_id, order_id):
        self.order_calls.append(order_id)
        return self.details.get(order_id)


class FakeBackend:
    """Guarda las guías como el backend: register_guide las crea y
    mark_guide_tracking solo sube el nivel."""

    def __init__(self, guides=None, lookup_fails=False, country=COLOMBIA, without_tracking_level=False,
                 rejects_guides=False, quota_capacity=None, quota_fails=False):
        self.guides = guides or {}
        self.lookup_fails = lookup_fails
        self.rejects_guides = rejects_guides  # el backend responde 400 (datos inválidos)
        self.country = country
        self.without_tracking_level = without_tracking_level
        self.registered = []
        self.tracking_marks = []
        self.notified = []
        self.tracking_messages = []  # mensaje ligado a cada marca de nivel
        self.outgoing = []
        # Cupo de mensajería: None = aprueba todo.
        self.quota_capacity = quota_capacity
        self.quota_fails = quota_fails
        self.quota_requests = []
        self.quota_released = []

    def get_business_country(self, business_id):
        return self.country

    def lookup_guides(self, numbers):
        if self.lookup_fails:
            return None
        found = {n: dict(self.guides[n]) for n in numbers if n in self.guides}
        if self.without_tracking_level:
            for guide_ in found.values():
                guide_.pop("tracking_level", None)
        return found

    def register_guide(self, number, user_phone, user_name, business_id, details=None):
        self.registered.append({"number": number, "user_phone": user_phone, "user_name": user_name,
                                "business_id": business_id, "details": details})
        if number in self.guides or self.rejects_guides:
            return False
        products = (details or {}).get("products") or []
        stored = {k: v for k, v in (details or {}).items() if k != "products"}
        self.guides[number] = guide(number, user_id=user_phone, business_id=business_id,
                                    details=stored, products=products)
        return True

    def mark_guide_tracking(self, number, level, meta_message_id=None):
        self.tracking_marks.append((number, level))
        self.tracking_messages.append(meta_message_id)
        stored = self.guides[number]
        stored["tracking_level"] = max(stored["tracking_level"], level)

    def mark_guide_notified(self, number, meta_message_id=None):
        self.notified.append(number)

    def reserve_quota(self, phones, kind, business_id, ttl_minutes):
        self.quota_requests.append((list(phones), kind, business_id))
        if self.quota_fails:
            return None
        approved = list(phones) if self.quota_capacity is None else list(phones)[: self.quota_capacity]
        if self.quota_capacity is not None:
            self.quota_capacity -= len(approved)
        return QuotaReservation(
            approved_phones=tuple(approved),
            denied_phones=tuple(p for p in phones if p not in approved),
            reservation_id=len(self.quota_requests) if approved else None,
        )

    def release_quota(self, reservation_id):
        self.quota_released.append(reservation_id)

    def register_outgoing_message(self, **kwargs):
        self.outgoing.append(kwargs)


class FakeSender:
    def __init__(self, fail_all=False):
        self.sent = []
        self.fail_all = fail_all

    def send_template(self, to_number, template):
        self.sent.append((to_number, template.name, template.log_message(), template.build_payload(to_number)))
        return None if self.fail_all else f"wamid.tracking.{len(self.sent)}"


def order(order_id, tracking, status=LEVEL_1_STATUS, total=84900, rate_type="CON RECAUDO",
          shipping_company="ENVIA", phone="3105409038"):
    return {
        "order_id": order_id,
        "status": status,
        "customer_name": "Cesar Ochoa",
        "customer_phone": phone,
        "city": "CALI",
        "total": total,
        "currency": "COP",
        "shipping_company": shipping_company,
        "rate_type": rate_type,
        "tracking_code": tracking,
    }


def order_detail(order_id, tracking, address="Cra 5 # 10-20 Barrio Centro", product="CAYENNE PEPPER. CM"):
    return {
        "order_id": order_id,
        "customer": {"name": "Cesar Ochoa", "phone": "3105409038", "document": "",
                     "address": address, "city": "CALI"},
        "items": [{"product_id": 2246234, "product_name": product, "qty": 1,
                   "unit_price": 84900, "subtotal": 84900}],
        "total": 84900,
        "currency": "COP",
        "tracking_code": tracking,
    }


def guide(number, user_id="573105409038", tracking_level=0, count=0, business_id=BUSINESS_ID,
          details=None, products=None, video_sent=False):
    return {
        "video_sent": video_sent,
        "number": number,
        "user_id": user_id,
        "business_id": business_id,
        "notification_count": count,
        "last_notification_timestamp": None,
        "tracking_level": tracking_level,
        "tracking_notified_at": None,
        "details": details or {},
        "products": products or [],
    }


def stored_guide(number, tracking_level=0, count=0, **kwargs):
    """Guía con los datos del pedido ya guardados (no hace falta /orden)."""
    details = {"order_id": "1", "customer_name": "Cesar Ochoa", "delivery_address": "Cra 5 # 10-20",
               "city": "CALI", "shipping_company": "ENVIA", "total": 84900.0}
    products = [{"product_id": 2246234, "product_name": "CAYENNE PEPPER. CM", "quantity": 1}]
    return guide(number, tracking_level=tracking_level, count=count, details=details, products=products, **kwargs)


class FakeVideos:
    """Videos de producto. Por defecto ningún producto tiene video."""

    def __init__(self, media_id=None):
        self.media_id = media_id
        self.calls = []
        self.discarded = []

    def video_for(self, business_id, products):
        self.calls.append((business_id, [p["product_name"] for p in products]))
        return (self.media_id, "CAYENNE.mp4") if self.media_id else None

    def discard(self, media_id):
        self.discarded.append(media_id)


def build_service(source, backend=None, sender=None, override=None, debug=None, videos=None):
    backend = backend or FakeBackend()
    sender = sender or FakeSender()
    service = DropiTrackingNotificationService(
        order_source=source,
        backend=backend,
        message_sender=sender,
        product_videos=videos or FakeVideos(),
        debug_notification_number=debug,
        notification_override_number=override,
    )
    return service, backend, sender


def sent_names(sender):
    return [s[1] for s in sender.sent]


def body_params(sent):
    return [p["text"] for p in sent[3]["template"]["components"][0]["parameters"]]


def run(service, **kwargs):
    return service.notify_tracking_levels(BUSINESS_ID, FROM, UNTIL, **kwargs)


# ------------------------------ Consulta de pedidos ------------------------------


def test_levels_have_the_requested_states_with_dropi_spelling():
    assert TRACKING_LEVELS == {
        # "GUIA GENERADA" (con espacio) es un estado de SUPPLI-EXPRESS: el
        # genérico de Dropi es GUIA_GENERADA.
        1: ("GUIA_GENERADA", "PREPARADO PARA TRANSPORTADORA", "ENTREGADO A TRANSPORTADORA", "ADMITIDA", "RECOLECTADO"),
        2: ("EN TRANSPORTE", "EN REPARTO", "DESPACHADA", "EN DESPACHO", "EN RUTA"),
    }


def test_requests_each_state_of_each_level_separately():
    source = FakeDropiSource()
    service, _, _ = build_service(source)

    report = run(service)

    all_states = [status for statuses in TRACKING_LEVELS.values() for status in statuses]
    assert source.order_list_calls == [(status, 0) for status in all_states]
    assert report.pages == len(all_states)


def test_paginates_each_state_while_pages_come_full():
    orders = [order(str(i), f"G{i}", status="ADMITIDA") for i in range(5)]
    source = FakeDropiSource({"ADMITIDA": orders}, details={str(i): order_detail(str(i), f"G{i}") for i in range(5)})
    service, _, sender = build_service(source)

    report = run(service, result_number=2)

    assert [start for status, start in source.order_list_calls if status == "ADMITIDA"] == [0, 2, 4]
    assert [start for status, start in source.order_list_calls if status == "EN RUTA"] == [0]
    assert report.total_orders == 5
    assert report.notified == {1: 5}
    assert len(sender.sent) == 5


def test_failed_state_query_does_not_stop_the_other_states():
    source = FakeDropiSource({LEVEL_2_STATUS: [order("1", "G1", status=LEVEL_2_STATUS)]},
                             details={"1": order_detail("1", "G1")}, fail_status=LEVEL_1_STATUS)
    service, _, sender = build_service(source)

    report = run(service)

    assert f"ordenes {LEVEL_1_STATUS} start=0" in report.errors
    assert sent_names(sender) == ["pedido_camino_1"]


# ------------------------------ Nivel 1 ------------------------------


def test_new_order_in_level_1_sends_pedido_enviado_and_stores_order():
    source = FakeDropiSource({LEVEL_1_STATUS: [order("96040863", "034059582302")]},
                             details={"96040863": order_detail("96040863", "034059582302")})
    service, backend, sender = build_service(source)

    report = run(service)

    assert report.notified == {1: 1}
    assert source.order_calls == ["96040863"]  # única consulta al MCP
    assert sent_names(sender) == ["pedido_enviado_1"]
    assert body_params(sender.sent[0]) == ["Cesar Ochoa", "034059582302", "CAYENNE PEPPER. CM", "84.900", "ENVIA"]
    assert sender.sent[0][0] == "573105409038"

    details = backend.registered[0]["details"]
    assert details["order_id"] == "96040863"
    assert details["products"][0]["product_name"] == "CAYENNE PEPPER. CM"
    assert backend.tracking_marks == [("034059582302", 1)]
    # Ligada a la plantilla: si Meta reporta que falló, el backend baja el nivel.
    assert backend.tracking_messages == ["wamid.tracking.1"]
    # El nivel 3 (reclame en oficina) sigue sin notificar.
    assert backend.notified == []

    assert len(backend.outgoing) == 1
    assert backend.outgoing[0]["guide_number"] == "034059582302"
    assert backend.outgoing[0]["message"] == sender.sent[0][2]


def test_prepaid_order_shows_nothing_to_pay():
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1", rate_type="SIN RECAUDO")]},
                             details={"1": order_detail("1", "G1")})
    service, _, sender = build_service(source)

    run(service)

    assert "Precio: *$0*" in sender.sent[0][2]


def test_level_1_without_total_is_skipped():
    detail = order_detail("1", "G1")
    detail["total"] = None
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1", total=None)]}, details={"1": detail})
    service, backend, sender = build_service(source)

    report = run(service)

    assert sender.sent == []
    assert report.skipped == ["G1"]
    assert backend.tracking_marks == []


# ------------------------------ Nivel 2 ------------------------------


def test_level_2_after_level_1_uses_stored_order_and_full_address():
    source = FakeDropiSource({LEVEL_2_STATUS: [order("1", "G1", status=LEVEL_2_STATUS)]})
    backend = FakeBackend({"G1": stored_guide("G1", tracking_level=1)})
    service, backend, sender = build_service(source, backend)

    report = run(service)

    assert report.notified == {2: 1}
    assert source.order_calls == []  # datos guardados: sin consultar a Dropi
    assert backend.registered == []
    assert sent_names(sender) == ["pedido_camino_1"]
    assert body_params(sender.sent[0]) == ["Cesar Ochoa", "CAYENNE PEPPER. CM", "G1", "Cra 5 # 10-20, CALI"]
    assert backend.tracking_marks == [("G1", 2)]


def test_level_2_address_does_not_repeat_city():
    detail = order_detail("1", "G1", address="Calle 2 A # 7-28 Barrio La Padilla, Cali")
    source = FakeDropiSource({LEVEL_2_STATUS: [order("1", "G1", status=LEVEL_2_STATUS)]}, details={"1": detail})
    service, _, sender = build_service(source)

    run(service)

    assert "ya está siendo transportado a Calle 2 A # 7-28 Barrio La Padilla, Cali." in sender.sent[0][2]


def test_order_first_seen_in_level_2_skips_level_1_for_good():
    source = FakeDropiSource({LEVEL_2_STATUS: [order("1", "G1", status=LEVEL_2_STATUS)]},
                             details={"1": order_detail("1", "G1")})
    service, backend, sender = build_service(source)

    run(service)
    # El pedido vuelve a un estado del nivel 1 (p. ej. regresa a la terminal
    # de origen): no se le envía la del nivel 1.
    source.orders_by_status = {LEVEL_1_STATUS: [order("1", "G1")]}
    report = run(service)

    assert sent_names(sender) == ["pedido_camino_1"]
    assert report.already_notified == 1
    assert backend.tracking_marks == [("G1", 2)]


# ------------------------------ Una notificación por nivel ------------------------------


def test_job_every_15_minutes_notifies_each_level_once():
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1")]}, details={"1": order_detail("1", "G1")})
    service, backend, sender = build_service(source)

    run(service)
    run(service)  # 15 min después, sigue en el nivel 1
    source.orders_by_status = {LEVEL_2_STATUS: [order("1", "G1", status=LEVEL_2_STATUS)]}
    run(service)
    report = run(service)  # sigue en el nivel 2

    assert sent_names(sender) == ["pedido_enviado_1", "pedido_camino_1"]
    assert source.order_calls == ["1"]  # el detalle se pidió una sola vez
    assert report.already_notified == 1
    assert backend.tracking_marks == [("G1", 1), ("G1", 2)]


def test_order_that_changed_state_between_queries_gets_only_the_highest_level():
    source = FakeDropiSource({
        LEVEL_1_STATUS: [order("1", "G1")],
        LEVEL_2_STATUS: [order("1", "G1", status=LEVEL_2_STATUS)],
    }, details={"1": order_detail("1", "G1")})
    service, backend, sender = build_service(source)

    report = run(service)

    assert report.total_orders == 1
    assert sent_names(sender) == ["pedido_camino_1"]
    assert backend.tracking_marks == [("G1", 2)]


@pytest.mark.parametrize("status", [LEVEL_1_STATUS, LEVEL_2_STATUS])
def test_guide_already_notified_in_office_gets_no_tracking(status):
    # notification_count > 0: ya se le notificó que está en oficina.
    source = FakeDropiSource({status: [order("1", "G1", status=status)]})
    backend = FakeBackend({"G1": stored_guide("G1", count=2)})
    service, backend, sender = build_service(source, backend)

    report = run(service)

    assert sender.sent == []
    assert report.already_notified == 1
    assert backend.tracking_marks == []


def test_failed_send_does_not_raise_level_and_is_retried():
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1")]}, details={"1": order_detail("1", "G1")})
    backend = FakeBackend()
    service, _, _ = build_service(source, backend, sender=FakeSender(fail_all=True))

    report = run(service)

    assert report.errors == ["G1"]
    assert backend.tracking_marks == []
    assert backend.outgoing == []

    # La próxima corrida reintenta con los datos ya guardados.
    service, _, sender = build_service(source, backend)
    report = run(service)

    assert report.notified == {1: 1}
    assert sent_names(sender) == ["pedido_enviado_1"]
    assert source.order_calls == ["1"]


# ------------------------------ Casos borde ------------------------------


def test_guide_of_another_business_is_skipped():
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1")]})
    backend = FakeBackend({"G1": stored_guide("G1", business_id=2)})
    service, backend, sender = build_service(source, backend)

    report = run(service)

    assert sender.sent == []
    assert report.skipped == ["G1"]


def test_orders_in_another_status_or_without_tracking_are_skipped():
    # Si N8N no aplicara el filtro de estado, no se notifica un pedido entregado.
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1", status="ENTREGADO"), order("2", "")]})
    service, backend, sender = build_service(source)

    report = run(service)

    assert sender.sent == []
    assert "G1 en estado ENTREGADO" in report.skipped
    assert "pedido 2 sin guía" in report.skipped


def test_backend_lookup_failure_sends_nothing():
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1")]}, details={"1": order_detail("1", "G1")})
    service, _, sender = build_service(source, FakeBackend(lookup_fails=True))

    report = run(service)

    assert sender.sent == []
    assert source.order_calls == []
    assert report.errors


def test_backend_without_tracking_level_sends_nothing():
    # Bot desplegado antes de migrar el backend: sin el nivel guardado cada
    # corrida repetiría las notificaciones.
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1")]})
    backend = FakeBackend({"G1": stored_guide("G1")}, without_tracking_level=True)
    service, _, sender = build_service(source, backend)

    report = run(service)

    assert sender.sent == []
    assert any("tracking_level" in error for error in report.errors)


def test_unknown_business_country_sends_nothing():
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1")]})
    service, _, sender = build_service(source, FakeBackend(country=None))

    report = run(service)

    assert sender.sent == []
    assert source.order_list_calls == []
    assert report.errors


def test_order_detail_failure_is_an_error():
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1")]}, details={})
    service, backend, sender = build_service(source)

    report = run(service)

    assert report.errors == ["G1"]
    assert sender.sent == []
    assert backend.registered == []


def test_customer_phone_gets_prefix_of_business_country():
    source = FakeDropiSource({"RECOLECTADO": [order("1784924", "FD45203393", status="RECOLECTADO",
                                                    phone="51319585", shipping_company="FORZA")]},
                             details={"1784924": order_detail("1784924", "FD45203393")})
    service, backend, sender = build_service(source, FakeBackend(country=PhoneCountry("502", 8)))

    run(service)

    assert sender.sent[0][0] == "50251319585"
    assert backend.registered[0]["user_phone"] == "50251319585"


def test_override_redirects_sending_but_registers_real_recipient():
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1")]}, details={"1": order_detail("1", "G1")})
    service, backend, sender = build_service(source, override="573009999999", debug="573008888888")

    run(service)

    assert [s[0] for s in sender.sent] == ["573009999999", "573008888888"]
    assert backend.outgoing[0]["user_phone"] == "573105409038"


# ------------------------------ Plantillas ------------------------------


RECIPIENT = RecipientInfo(name="Cesar Ochoa", phone="573105409038", tracking_number="240063856178",
                          delivery_address="Cra 5 # 10-20, CALI", product="Collar Girasol")


def test_pedido_enviado_template():
    template = PedidoEnviadoTemplate(RECIPIENT, "84.900", "INTERRAPIDISIMO")

    payload = template.build_payload("573105409038")

    assert payload["template"]["name"] == "pedido_enviado_1"
    assert [p["text"] for p in payload["template"]["components"][0]["parameters"]] == [
        "Cesar Ochoa", "240063856178", "Collar Girasol", "84.900", "INTERRAPIDISIMO",
    ]
    assert template.log_message() == (
        "Hola Cesar Ochoa, ¡tu pedido ya está listo para envío! 📦\n"
        "\n"
        "Guía: 240063856178\n"
        "Producto: Collar Girasol\n"
        "Precio: *$84.900*\n"
        "Transportadora: INTERRAPIDISIMO\n"
        "\n"
        "La transportadora iniciará el traslado de tu paquete en los próximos días. "
        "¡Mantente atento a cualquier novedad con tu entrega!"
    )


def test_pedido_camino_template():
    template = PedidoCaminoTemplate(RECIPIENT)

    payload = template.build_payload("573105409038")

    assert payload["template"]["name"] == "pedido_camino_1"
    assert [p["text"] for p in payload["template"]["components"][0]["parameters"]] == [
        "Cesar Ochoa", "Collar Girasol", "240063856178", "Cra 5 # 10-20, CALI",
    ]
    assert template.log_message() == (
        "Tu pedido esta en camino.\n"
        "Hola Cesar Ochoa.\n"
        "\n"
        "Tu pedido con el producto Collar Girasol y número de guia 240063856178 , "
        "ya está siendo transportado a Cra 5 # 10-20, CALI.\n"
        "\n"
        "Puedes usar tu número de guía para hacerle seguimiento en la página oficial "
        "de la transportadora. ¡Pronto estará en tus manos!"
    )


# ------------------------------ Cliente del backend ------------------------------


def test_backend_client_marks_tracking_level():
    class Session:
        def __init__(self):
            self.calls = []

        def request(self, method, url, headers=None, timeout=None, **kwargs):
            self.calls.append((method, url, kwargs.get("json")))

            class Response:
                status_code = 200

                def raise_for_status(self):
                    pass

            return Response()

    client = RustBackendClient("http://backend", api_key="k")
    client._token = "token"
    client._session = Session()

    client.mark_guide_tracking("G1", 2)
    client.mark_guide_tracking("G1", 1, "wamid.1")

    assert client._session.calls == [
        ("POST", "http://backend/guides/G1/tracking", {"level": 2, "meta_message_id": None}),
        ("POST", "http://backend/guides/G1/tracking", {"level": 1, "meta_message_id": "wamid.1"}),
    ]


def test_backend_client_links_notification_and_reports_message_errors():
    class Session:
        def __init__(self):
            self.calls = []

        def request(self, method, url, headers=None, timeout=None, **kwargs):
            self.calls.append((method, url, kwargs.get("json")))

            class Response:
                status_code = 201

                def raise_for_status(self):
                    pass

                def json(self):
                    return {"updated": True, "undone": []}

            return Response()

    client = RustBackendClient("http://backend", api_key="k")
    client._token = "token"
    client._session = Session()
    payload = {"id": "wamid.9", "status": "failed", "errors": [{"code": 131048, "title": "Spam Rate limit hit"}]}

    client.mark_guide_notified("G1", "wamid.8")
    client.register_message_error("wamid.9", 131048, "Spam Rate limit hit", payload)

    assert client._session.calls == [
        ("POST", "http://backend/guides/G1/notified", {"meta_message_id": "wamid.8"}),
        ("POST", "http://backend/messages/wamid.9/errors",
         {"error_code": 131048, "error_title": "Spam Rate limit hit", "payload": payload}),
    ]


def test_guide_rejected_by_backend_is_not_notified():
    # Antes se enviaba igual (fail-open), la guía nunca quedaba registrada y
    # la misma plantilla salía en cada corrida (FD45379197, 10 veces el 9 nov).
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "FD45379197")]},
                             details={"1": order_detail("1", "FD45379197")})
    service, backend, sender = build_service(source, FakeBackend(rejects_guides=True))

    for _ in range(2):
        report = run(service)
        assert report.notified == {}
        assert report.skipped == ["FD45379197"]

    assert sender.sent == []
    assert backend.tracking_marks == []


def _client_answering(status_code=None, error=None):
    class Response:
        def __init__(self):
            self.status_code = status_code
            self.text = '{"message": "Validation failed"}'

        def raise_for_status(self):
            if self.status_code >= 400:
                raise requests.HTTPError(f"{self.status_code} Error")

        def json(self):
            return {"created": True}

    class Session:
        def request(self, *args, **kwargs):
            if error:
                raise error
            return Response()

    client = RustBackendClient("http://backend", api_key="k")
    client._token = "token"
    client._session = Session()
    return client


def test_backend_client_does_not_notify_guides_it_cannot_register():
    assert _client_answering(201).register_guide("G1", "488760023", "X", 2) is True
    # Datos inválidos: no notificar (se repetiría en cada corrida).
    assert _client_answering(400).register_guide("G1", "1234", "X", 2) is False
    # Backend caído: tampoco (mejor perder la notificación que notificar duplicado).
    assert _client_answering(500).register_guide("G1", "488760023", "X", 2) is False
    assert _client_answering(error=requests.ConnectionError("down")).register_guide("G1", "488760023", "X", 2) is False


# ------------------------------ Endpoint ------------------------------


class BlockingTrackingNotifier:
    """La corrida del business 3 no termina hasta que se libere `finish`."""

    def __init__(self):
        self.runs = []
        self.started = threading.Event()
        self.finish = threading.Event()

    def notify_tracking_levels(self, *args):
        self.runs.append(args)
        if args[0] == 3:
            self.started.set()
            assert self.finish.wait(5)


class FakeOfficeNotifier:
    def __init__(self):
        self.runs = []

    def notify_office_orders(self, *args):
        self.runs.append(args)


class FakeBusinessBackend:
    def __init__(self, exists):
        self.exists = exists

    def business_exists(self, business_id):
        return self.exists


def endpoint_client(tracking=None, office=None, exists=True):
    app = FastAPI()
    app.include_router(notify_dropi_router)
    tracking = tracking or BlockingTrackingNotifier()
    office = office or FakeOfficeNotifier()
    tracking_runs, office_runs = RunGuard(), RunGuard()
    app.dependency_overrides[get_dropi_tracking_notifier] = lambda: tracking
    app.dependency_overrides[get_dropi_notifier] = lambda: office
    app.dependency_overrides[get_backend_client] = lambda: FakeBusinessBackend(exists)
    app.dependency_overrides[get_dropi_tracking_runs] = lambda: tracking_runs
    app.dependency_overrides[get_dropi_runs] = lambda: office_runs
    return TestClient(app), tracking, office


def test_endpoint_runs_for_business_in_query():
    client, tracking, _ = endpoint_client()

    response = client.post("/notify-dropi-tracking?business_id=5", json={"from": "2026-11-01", "until": "2026-11-08"})

    assert response.status_code == 202
    assert response.json()["business_id"] == 5
    assert tracking.runs == [(5, date(2026, 11, 1), date(2026, 11, 8), 100, 0)]


@pytest.mark.parametrize("query,exists,status", [
    ("", True, 422),
    ("?business_id=0", True, 422),
    ("?business_id=99", False, 404),
    ("?business_id=99", None, 503),
])
def test_endpoint_rejects_invalid_or_unknown_business(query, exists, status):
    client, tracking, _ = endpoint_client(exists=exists)

    assert client.post(f"/notify-dropi-tracking{query}").status_code == status
    assert tracking.runs == []


def test_endpoint_one_run_per_business_without_blocking_office_notification():
    client, tracking, office = endpoint_client()

    with ThreadPoolExecutor(1) as pool:
        first = pool.submit(client.post, "/notify-dropi-tracking?business_id=3")
        assert tracking.started.wait(5)

        second = client.post("/notify-dropi-tracking?business_id=3")
        office_run = client.post("/notify-dropi-mcp?business_id=3")

        tracking.finish.set()
        assert first.result(5).status_code == 202

    assert second.status_code == 409
    assert office_run.status_code == 202  # guardas separadas
    assert [run[0] for run in office.runs] == [3]
    # Terminada la primera, el business 3 se puede volver a correr.
    assert client.post("/notify-dropi-tracking?business_id=3").status_code == 202


# ------------------------------ Cupo de mensajería ------------------------------


def test_tracking_reserves_its_own_quota_and_defers_what_does_not_fit():
    source = FakeDropiSource(
        {LEVEL_1_STATUS: [order("1", "G1", phone="3000000001"), order("2", "G2", phone="3000000002")]},
        details={"1": order_detail("1", "G1"), "2": order_detail("2", "G2")},
    )
    service, backend, sender = build_service(source, FakeBackend(quota_capacity=1))

    report = run(service)

    assert backend.quota_requests == [(["573000000001", "573000000002"], "tracking", BUSINESS_ID)]
    assert report.notified == {1: 1}
    assert report.deferred == ["G2"]
    assert source.order_calls == ["1"]  # sin cupo ni se consulta el detalle a Dropi
    assert backend.tracking_marks == [("G1", 1)]
    assert backend.quota_released == [1]

    # La próxima corrida, con cupo, lo envía.
    backend.quota_capacity = None
    assert run(service).notified == {1: 1}
    assert backend.tracking_marks == [("G1", 1), ("G2", 1)]


def test_tracking_without_quota_service_sends_nothing():
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1")]}, details={"1": order_detail("1", "G1")})
    service, backend, sender = build_service(source, FakeBackend(quota_fails=True))

    report = run(service)

    assert sender.sent == []
    assert backend.tracking_marks == []
    assert "backend: no se pudo reservar cupo de mensajería" in report.errors


# ------------------------------ Video del producto ------------------------------


def test_level_1_sends_the_product_video_before_pedido_enviado():
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1")]}, details={"1": order_detail("1", "G1")})
    videos = FakeVideos(media_id="media-cayenne")
    service, backend, sender = build_service(source, videos=videos)

    report = run(service)

    assert sent_names(sender) == ["video_producto", "pedido_enviado_1"]
    assert videos.calls == [(BUSINESS_ID, ["CAYENNE PEPPER. CM"])]
    assert report.notified == {1: 1}
    assert report.videos == 1
    # El nivel queda ligado a pedido_enviado_1, no al video.
    assert backend.tracking_messages == ["wamid.tracking.2"]
    assert [(m["media_type"], m["media_id"]) for m in backend.outgoing] == [("video", "media-cayenne"), ("text", None)]


def test_level_2_does_not_send_the_video():
    source = FakeDropiSource({LEVEL_2_STATUS: [order("1", "G1", status=LEVEL_2_STATUS)]})
    backend = FakeBackend({"G1": stored_guide("G1")})
    videos = FakeVideos(media_id="media-cayenne")
    service, backend, sender = build_service(source, backend, videos=videos)

    run(service)

    assert sent_names(sender) == ["pedido_camino_1"]
    assert videos.calls == []


def test_video_is_not_repeated_when_level_1_is_retried():
    # El video salió pero pedido_enviado_1 falló: el reintento no repite el video.
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1")]})
    backend = FakeBackend({"G1": stored_guide("G1", video_sent=True)})
    videos = FakeVideos(media_id="media-cayenne")
    service, backend, sender = build_service(source, backend, videos=videos)

    report = run(service)

    assert sent_names(sender) == ["pedido_enviado_1"]
    assert videos.calls == []
    assert report.videos == 0


def test_rejected_video_is_forgotten_and_level_1_still_goes_out():
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1")]}, details={"1": order_detail("1", "G1")})
    videos = FakeVideos(media_id="media-vencido")
    sender = FakeSender()
    original_send = sender.send_template
    sender.send_template = lambda to, template: None if template.name == "video_producto" else original_send(to, template)
    service, backend, _ = build_service(source, sender=sender, videos=videos)

    report = run(service)

    assert videos.discarded == ["media-vencido"]
    assert report.notified == {1: 1}
    assert report.videos == 0
    assert backend.tracking_marks == [("G1", 1)]


def test_failed_pedido_enviado_does_not_raise_the_level_even_if_the_video_went_out():
    source = FakeDropiSource({LEVEL_1_STATUS: [order("1", "G1")]}, details={"1": order_detail("1", "G1")})
    sender = FakeSender()
    original_send = sender.send_template
    sender.send_template = lambda to, template: None if template.name == "pedido_enviado_1" else original_send(to, template)
    service, backend, _ = build_service(source, sender=sender, videos=FakeVideos(media_id="media-cayenne"))

    report = run(service)

    assert report.errors == ["G1"]
    assert backend.tracking_marks == []
    assert [m["media_type"] for m in backend.outgoing] == ["video"]  # el video sí queda en el historial
