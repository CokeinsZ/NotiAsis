"""Tests de la notificación de pedidos de Dropi (MCP vía N8N).

Todo con dobles de prueba: sin N8N, sin Meta, sin backend real.
"""

from datetime import date, datetime, timedelta

import pytest
from pydantic import ValidationError

from app.api.notify_dropi import NotifyDropiRequest
from app.services.dropi_client import N8nDropiClient
from app.services.dropi_notifier import DropiNotificationService

FROM = date(2026, 10, 6)
UNTIL = date(2026, 10, 21)
YESTERDAY = (datetime.now() - timedelta(days=1)).isoformat(timespec="seconds")
TODAY = datetime.now().isoformat(timespec="seconds")


# ------------------------------ Fakes ------------------------------


class FakeDropiSource:
    """Simula /oficina (paginado por `start`) y /orden."""

    def __init__(self, orders=(), details=None, fail_on_start=None, ignore_start=False):
        self.orders = list(orders)
        self.details = details if details is not None else {}
        self.fail_on_start = fail_on_start
        self.ignore_start = ignore_start
        self.office_calls = []
        self.order_calls = []

    def fetch_office_orders(self, from_date, until_date, result_number, start):
        self.office_calls.append((from_date, until_date, result_number, start))
        if start == self.fail_on_start:
            raise Exception("N8N timeout")
        offset = 0 if self.ignore_start else start
        page = self.orders[offset:offset + result_number]
        return page, len(page)

    def fetch_order(self, order_id):
        self.order_calls.append(order_id)
        return self.details.get(order_id)


class FakeBackend:
    def __init__(self, guides=None, lookup_fails=False):
        # guides: {número: guía con el formato de POST /guides/lookup}
        self.guides = guides or {}
        self.lookup_fails = lookup_fails
        self.lookups = []
        self.registered = []
        self.notified = []
        self.outgoing = []

    def lookup_guides(self, numbers):
        self.lookups.append(list(numbers))
        if self.lookup_fails:
            return None
        return {n: self.guides[n] for n in numbers if n in self.guides}

    def register_guide(self, number, user_phone, user_name, business_id, details=None):
        self.registered.append({"number": number, "user_phone": user_phone, "user_name": user_name,
                                "business_id": business_id, "details": details})
        if number in self.guides:
            return False
        self.guides[number] = guide(number, user_phone, count=0)
        return True

    def mark_guide_notified(self, number):
        self.notified.append(number)

    def register_outgoing_message(self, **kwargs):
        self.outgoing.append(kwargs)


class FakeSender:
    def __init__(self, fail=False):
        self.sent = []
        self.fail = fail

    def send_template(self, to_number, template):
        self.sent.append((to_number, template.name, template.log_message()))
        return None if self.fail else f"wamid.dropi.{len(self.sent)}"


def office_order(order_id, tracking, name="Cesar Ochoa", phone="3105409038"):
    return {
        "order_id": order_id,
        "status": "RECLAME EN OFICINA",
        "customer_name": name,
        "customer_phone": phone,
        "city": "TARAZA",
        "total": 84900,
        "currency": "COP",
        "shipping_company": "INTERRAPIDISIMO",
        "tracking_code": tracking,
    }


def order_detail(order_id, tracking, product="Collar Girasol GIRATORIO + Caja Cuerina"):
    return {
        "order_id": order_id,
        "status": "RECLAME EN OFICINA",
        "customer": {
            "name": "Cesar Ochoa",
            "phone": "3105409038",
            "document": "",
            "address": "Oficina Interrapidisimo",
            "city": "TARAZA",
        },
        "items": [{"product_id": 2298431, "product_name": product, "qty": 1,
                   "unit_price": 24900, "subtotal": 84900}],
        "total": 84900,
        "currency": "COP",
        "tracking_code": tracking,
    }


def guide(number, user_id="573105409038", count=1, last=YESTERDAY, products=None, details=None):
    return {
        "number": number,
        "user_id": user_id,
        "business_id": 1,
        "notification_count": count,
        "last_notification_timestamp": last,
        "details": details or {},
        "products": products or [],
    }


def stored_details():
    return {"customer_name": "Cesar Ochoa", "delivery_address": "Oficina Interrapidisimo", "city": "TARAZA"}


def stored_products():
    return [{"product_id": 2298431, "product_name": "Collar Girasol", "quantity": 1}]


def build_service(source, backend=None, sender=None, override=None, debug=None):
    backend = backend or FakeBackend()
    sender = sender or FakeSender()
    service = DropiNotificationService(
        order_source=source,
        backend=backend,
        message_sender=sender,
        business_id=1,
        debug_notification_number=debug,
        notification_override_number=override,
    )
    return service, backend, sender


# ------------------------------ Paginación ------------------------------


def test_paginates_while_pages_come_full():
    orders = [office_order(str(i), f"G{i}") for i in range(5)]
    source = FakeDropiSource(orders, details={str(i): order_detail(str(i), f"G{i}") for i in range(5)})
    service, backend, _ = build_service(source)

    report = service.notify_office_orders(FROM, UNTIL, result_number=2, start=0)

    # count 2 == result_number -> siguiente página con start += count
    assert [call[3] for call in source.office_calls] == [0, 2, 4]
    assert report.pages == 3
    assert report.total_orders == 5
    assert backend.lookups == [[f"G{i}" for i in range(5)]]  # una sola consulta en lote


def test_last_page_exactly_full_requests_one_more_empty_page():
    orders = [office_order(str(i), f"G{i}") for i in range(4)]
    source = FakeDropiSource(orders, details={str(i): order_detail(str(i), f"G{i}") for i in range(4)})
    service, _, _ = build_service(source)

    service.notify_office_orders(FROM, UNTIL, result_number=2, start=0)

    assert [call[3] for call in source.office_calls] == [0, 2, 4]


def test_pagination_starts_from_requested_start():
    orders = [office_order(str(i), f"G{i}") for i in range(14)]
    source = FakeDropiSource(orders, details={})
    service, _, _ = build_service(source)

    service.notify_office_orders(FROM, UNTIL, result_number=3, start=10)

    assert [call[3] for call in source.office_calls] == [10, 13]
    assert source.office_calls[0][:3] == (FROM, UNTIL, 3)


def test_repeated_pages_stop_pagination():
    # Un webhook que ignora `start` devolvería siempre la misma página llena.
    orders = [office_order("1", "G1"), office_order("2", "G2")]
    source = FakeDropiSource(orders, details={"1": order_detail("1", "G1"), "2": order_detail("2", "G2")},
                             ignore_start=True)
    service, _, sender = build_service(source)

    report = service.notify_office_orders(FROM, UNTIL, result_number=2)

    assert len(source.office_calls) == 2
    assert report.total_orders == 2
    assert len(sender.sent) == 2
    assert any("repetida" in error for error in report.errors)


def test_office_page_failure_processes_orders_already_fetched():
    orders = [office_order(str(i), f"G{i}") for i in range(4)]
    source = FakeDropiSource(orders, details={str(i): order_detail(str(i), f"G{i}") for i in range(4)},
                             fail_on_start=2)
    service, _, sender = build_service(source)

    report = service.notify_office_orders(FROM, UNTIL, result_number=2)

    assert report.total_orders == 2
    assert len(sender.sent) == 2
    assert "oficina start=2" in report.errors


# ------------------------------ Escalación ------------------------------


def test_new_guide_sends_only_mensaje_guia_and_stores_order():
    source = FakeDropiSource([office_order("93632719", "240063856178")],
                             details={"93632719": order_detail("93632719", "240063856178")})
    service, backend, sender = build_service(source)

    report = service.notify_office_orders(FROM, UNTIL)

    assert report.new_notifications == 1
    assert source.order_calls == ["93632719"]  # única consulta al MCP
    assert [s[1] for s in sender.sent] == ["mensaje_guia_es"]  # sin plantilla de PDF
    to_number, _, text = sender.sent[0]
    assert to_number == "573105409038"
    assert "Collar Girasol GIRATORIO + Caja Cuerina" in text
    assert "Oficina Interrapidisimo, TARAZA" in text

    registered = backend.registered[0]
    assert registered["number"] == "240063856178"
    assert registered["business_id"] == 1
    details = registered["details"]
    assert details["order_id"] == "93632719"
    assert details["shipping_company"] == "INTERRAPIDISIMO"
    assert details["products"] == [{"product_id": 2298431, "product_name": "Collar Girasol GIRATORIO + Caja Cuerina",
                                    "quantity": 1, "unit_price": 24900.0, "subtotal": 84900.0}]

    assert backend.outgoing[0]["media_type"] == "text"
    assert backend.outgoing[0]["media_id"] is None
    assert backend.notified == ["240063856178"]


def test_guide_notified_once_gets_reminder_without_calling_dropi():
    source = FakeDropiSource([office_order("1", "G1")])
    backend = FakeBackend({"G1": guide("G1", count=1)})
    service, backend, sender = build_service(source, backend)

    report = service.notify_office_orders(FROM, UNTIL)

    assert report.reminders == 1
    assert source.order_calls == []
    assert [s[1] for s in sender.sent] == ["recordatorio"]
    assert backend.registered == []
    assert backend.notified == ["G1"]


def test_guide_notified_twice_gets_final_reminder_with_stored_product():
    source = FakeDropiSource([office_order("1", "G1")])
    backend = FakeBackend({"G1": guide("G1", count=2, details=stored_details(), products=stored_products())})
    service, backend, sender = build_service(source, backend)

    report = service.notify_office_orders(FROM, UNTIL)

    assert report.final_reminders == 1
    assert source.order_calls == []
    assert sender.sent[0][1] == "recordatorio_final"
    assert "Collar Girasol" in sender.sent[0][2]


def test_legacy_guide_without_products_fetches_order_once_for_final_reminder():
    # Guía registrada por el flujo de PDF/sheet, antes de guardar productos.
    source = FakeDropiSource([office_order("1", "G1")], details={"1": order_detail("1", "G1", product="Perfumero")})
    backend = FakeBackend({"G1": guide("G1", count=2)})
    service, backend, sender = build_service(source, backend)

    report = service.notify_office_orders(FROM, UNTIL)

    assert report.final_reminders == 1
    assert source.order_calls == ["1"]
    assert backend.registered[0]["details"]["products"][0]["product_name"] == "Perfumero"  # se guarda
    assert "Perfumero" in sender.sent[0][2]


@pytest.mark.parametrize("count,last", [(3, YESTERDAY), (5, YESTERDAY), (1, TODAY)])
def test_max_notifications_or_notified_today_sends_nothing(count, last):
    source = FakeDropiSource([office_order("1", "G1")])
    backend = FakeBackend({"G1": guide("G1", count=count, last=last)})
    service, backend, sender = build_service(source, backend)

    report = service.notify_office_orders(FROM, UNTIL)

    assert sender.sent == []
    assert report.skipped == ["G1"]
    assert backend.notified == []


def test_registered_guide_never_notified_is_retried_without_dropi_or_reregistering():
    # Primer envío fallido: la guía existe con count 0 y sus datos guardados.
    source = FakeDropiSource([office_order("1", "G1")])
    backend = FakeBackend({"G1": guide("G1", count=0, last=None, details=stored_details(),
                                       products=stored_products())})
    service, backend, sender = build_service(source, backend)

    report = service.notify_office_orders(FROM, UNTIL)

    assert report.new_notifications == 1
    assert source.order_calls == []
    assert backend.registered == []
    assert [s[1] for s in sender.sent] == ["mensaje_guia_es"]
    assert backend.notified == ["G1"]


def test_registered_guide_without_details_is_completed_and_notified():
    source = FakeDropiSource([office_order("1", "G1")], details={"1": order_detail("1", "G1")})
    backend = FakeBackend({"G1": guide("G1", count=0, last=None)})
    service, backend, sender = build_service(source, backend)

    report = service.notify_office_orders(FROM, UNTIL)

    # register_guide responde False (ya existía) pero no es una carrera:
    # solo se usó para guardar los datos del pedido.
    assert report.new_notifications == 1
    assert backend.registered[0]["details"]["order_id"] == "1"
    assert backend.notified == ["G1"]


# ------------------------------ Casos borde ------------------------------


def test_duplicate_tracking_codes_are_notified_once():
    source = FakeDropiSource([office_order("1", "G1"), office_order("2", "G1")], details={"1": order_detail("1", "G1")})
    service, _, sender = build_service(source)

    report = service.notify_office_orders(FROM, UNTIL)

    assert report.total_orders == 1
    assert len(sender.sent) == 1


def test_orders_without_tracking_code_are_skipped():
    source = FakeDropiSource([office_order("1", ""), office_order("2", "G2")], details={"2": order_detail("2", "G2")})
    service, backend, sender = build_service(source)

    report = service.notify_office_orders(FROM, UNTIL)

    assert backend.lookups == [["G2"]]
    assert len(sender.sent) == 1
    assert "pedido 1 sin guía" in report.skipped


def test_backend_lookup_failure_sends_nothing():
    source = FakeDropiSource([office_order("1", "G1")], details={"1": order_detail("1", "G1")})
    service, _, sender = build_service(source, FakeBackend(lookup_fails=True))

    report = service.notify_office_orders(FROM, UNTIL)

    assert sender.sent == []
    assert source.order_calls == []
    assert report.errors


def test_order_detail_failure_is_an_error():
    source = FakeDropiSource([office_order("1", "G1")], details={})
    service, backend, sender = build_service(source)

    report = service.notify_office_orders(FROM, UNTIL)

    assert report.errors == ["G1"]
    assert sender.sent == []
    assert backend.registered == []


def test_failed_send_does_not_mark_guide_as_notified():
    source = FakeDropiSource([office_order("1", "G1")], details={"1": order_detail("1", "G1")})
    service, backend, _ = build_service(source, sender=FakeSender(fail=True))

    report = service.notify_office_orders(FROM, UNTIL)

    assert report.errors == ["G1"]
    assert backend.registered  # queda registrada con count 0: se reintenta en la próxima corrida
    assert backend.notified == []
    assert backend.outgoing == []


def test_override_redirects_sending_but_registers_real_recipient():
    source = FakeDropiSource([office_order("1", "G1")], details={"1": order_detail("1", "G1")})
    service, backend, sender = build_service(source, override="573009999999", debug="573008888888")

    service.notify_office_orders(FROM, UNTIL)

    assert [s[0] for s in sender.sent] == ["573009999999", "573008888888"]
    assert backend.outgoing[0]["user_phone"] == "573105409038"


def test_product_names_are_single_line_for_meta():
    detail = order_detail("1", "G1", product="Collar\nGirasol\t  +    Caja")
    source = FakeDropiSource([office_order("1", "G1")], details={"1": detail})
    service, backend, sender = build_service(source)

    service.notify_office_orders(FROM, UNTIL)

    assert backend.registered[0]["details"]["products"][0]["product_name"] == "Collar Girasol + Caja"


# ------------------------------ Request del endpoint ------------------------------


def test_request_defaults():
    request = NotifyDropiRequest()
    assert request.from_date == date.today() - timedelta(days=15)
    assert request.until == date.today()
    assert request.result_number == 100
    assert request.start == 0


def test_request_accepts_from_alias():
    request = NotifyDropiRequest.model_validate(
        {"from": "2026-10-10", "until": "2026-10-20", "result_number": 1, "start": 5}
    )
    assert request.from_date == date(2026, 10, 10)
    assert request.result_number == 1
    assert request.start == 5


@pytest.mark.parametrize("payload", [
    {"from": "2026-10-21", "until": "2026-10-20"},
    {"result_number": 0},
    {"start": -1},
])
def test_request_rejects_invalid_filters(payload):
    with pytest.raises(ValidationError):
        NotifyDropiRequest.model_validate(payload)


# ------------------------------ Cliente N8N ------------------------------


class FakeResponse:
    def __init__(self, status_code, payload=None):
        self.status_code = status_code
        self._payload = payload or {}
        self.text = str(payload)

    def json(self):
        return self._payload

    def raise_for_status(self):
        if self.status_code >= 400:
            raise Exception(f"HTTP {self.status_code}")


class FakeHttpSession:
    def __init__(self, responses):
        self.responses = list(responses)
        self.calls = []

    def get(self, url, timeout=None, **kwargs):
        self.calls.append((url, kwargs))
        return self.responses.pop(0)


def test_client_sends_filters_as_json_body_and_reads_count():
    session = FakeHttpSession([FakeResponse(200, {"items": [{"order_id": "1"}], "count": 1})])
    client = N8nDropiClient("https://n8n.test/webhook/dropi/", session=session)

    items, count = client.fetch_office_orders(date(2026, 10, 10), date(2026, 10, 20), 1, 0)

    assert (items, count) == ([{"order_id": "1"}], 1)
    url, kwargs = session.calls[0]
    assert url == "https://n8n.test/webhook/dropi/oficina"
    assert kwargs["json"] == {"from": "2026-10-10", "until": "2026-10-20", "result_number": 1, "start": 0}


def test_client_retries_rate_limit_with_backoff():
    sleeps = []
    session = FakeHttpSession([FakeResponse(429), FakeResponse(200, {"order_id": "93632719"})])
    client = N8nDropiClient("https://n8n.test/webhook/dropi", session=session, sleep=sleeps.append)

    order = client.fetch_order("93632719")

    assert order == {"order_id": "93632719"}
    assert session.calls[0] == ("https://n8n.test/webhook/dropi/orden", {"params": {"order_id": "93632719"}})
    assert sleeps == [2.0]


def test_client_gives_up_after_max_attempts():
    session = FakeHttpSession([FakeResponse(429), FakeResponse(429), FakeResponse(429)])
    client = N8nDropiClient("https://n8n.test/webhook/dropi", session=session, sleep=lambda s: None)

    assert client.fetch_order("1") is None
    assert len(session.calls) == 3

    session = FakeHttpSession([FakeResponse(503)] * 3)
    client = N8nDropiClient("https://n8n.test/webhook/dropi", session=session, sleep=lambda s: None)
    with pytest.raises(Exception):
        client.fetch_office_orders(FROM, UNTIL, 100, 0)
