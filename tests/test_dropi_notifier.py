"""Tests de la notificación de pedidos de Dropi (MCP vía N8N).

Todo con dobles de prueba: sin N8N, sin Meta, sin backend real.
"""

from datetime import date, datetime, timedelta

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from pydantic import ValidationError

from app.api.dependencies import get_backend_client, get_dropi_notifier
from app.api.notify_dropi import NotifyDropiRequest, router as notify_dropi_router
from app.models.shipping import RecipientInfo
from app.services.dropi_client import N8nDropiClient
from app.services.dropi_notifier import OFFICE_STATUS, DropiNotificationService
from app.services.notification_policy import NotificationStep, daily_step
from app.whatsapp.templates.pedido_oficina import PedidoOficinaTemplate
from app.whatsapp.templates.video_producto import VideoProductoTemplate

BUSINESS_ID = 1
FROM = date(2026, 10, 6)
UNTIL = date(2026, 10, 21)
YESTERDAY = (datetime.now() - timedelta(days=1)).isoformat(timespec="seconds")
TODAY = datetime.now().isoformat(timespec="seconds")


# ------------------------------ Fakes ------------------------------


class FakeDropiSource:
    """Simula /ordenes (paginado por `start`) y /orden."""

    def __init__(self, orders=(), details=None, fail_on_start=None, ignore_start=False):
        self.orders = list(orders)
        self.details = details if details is not None else {}
        self.fail_on_start = fail_on_start
        self.ignore_start = ignore_start
        self.order_list_calls = []
        self.order_calls = []

    def fetch_orders(self, status, from_date, until_date, result_number, start):
        self.order_list_calls.append((status, from_date, until_date, result_number, start))
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
        self.guides[number] = guide(number, user_phone, count=0, business_id=business_id)
        return True

    def mark_guide_notified(self, number):
        self.notified.append(number)

    def register_outgoing_message(self, **kwargs):
        self.outgoing.append(kwargs)


class FakeSender:
    def __init__(self, fail=(), fail_all=False):
        self.sent = []
        self.fail = set(fail)  # nombres de plantilla que Meta rechaza
        self.fail_all = fail_all

    def send_template(self, to_number, template):
        self.sent.append((to_number, template.name, template.log_message()))
        if self.fail_all or template.name in self.fail:
            return None
        return f"wamid.dropi.{len(self.sent)}"


class FakeVideos:
    def __init__(self, media_id="media-video-1"):
        self.media_id = media_id
        self.calls = []
        self.discarded = []

    def video_for(self, business_id, products):
        self.calls.append((business_id, [p["product_name"] for p in products]))
        return (self.media_id, "ASAD_CLUB_DE_NUIT.mp4") if self.media_id else None

    def discard(self, media_id):
        self.discarded.append(media_id)


def office_order(order_id, tracking, name="Cesar Ochoa", phone="3105409038", total=84900,
                 rate_type="CON RECAUDO", status=OFFICE_STATUS, shipping_company="INTERRAPIDISIMO"):
    return {
        "order_id": order_id,
        "status": status,
        "customer_name": name,
        "customer_phone": phone,
        "city": "TARAZA",
        "total": total,
        "currency": "COP",
        "shipping_company": shipping_company,
        "rate_type": rate_type,
        "tracking_code": tracking,
    }


def order_detail(order_id, tracking, product="Collar Girasol GIRATORIO + Caja Cuerina",
                 address="Oficina Interrapidisimo"):
    return {
        "order_id": order_id,
        "status": OFFICE_STATUS,
        "customer": {
            "name": "Cesar Ochoa",
            "phone": "3105409038",
            "document": "",
            "address": address,
            "city": "TARAZA",
        },
        "items": [{"product_id": 2298431, "product_name": product, "qty": 1,
                   "unit_price": 24900, "subtotal": 84900}],
        "total": 84900,
        "currency": "COP",
        "tracking_code": tracking,
    }


def guide(number, user_id="573105409038", count=1, last=YESTERDAY, products=None, details=None,
          business_id=BUSINESS_ID):
    return {
        "number": number,
        "user_id": user_id,
        "business_id": business_id,
        "notification_count": count,
        "last_notification_timestamp": last,
        "details": details or {},
        "products": products or [],
    }


def stored_details(total=84900.0):
    return {"customer_name": "Cesar Ochoa", "delivery_address": "Oficina Interrapidisimo", "city": "TARAZA",
            "total": total}


def stored_products():
    return [{"product_id": 2298431, "product_name": "Collar Girasol", "quantity": 1}]


def build_service(source, backend=None, sender=None, videos=None, override=None, debug=None):
    backend = backend or FakeBackend()
    sender = sender or FakeSender()
    videos = videos or FakeVideos()
    service = DropiNotificationService(
        order_source=source,
        backend=backend,
        message_sender=sender,
        product_videos=videos,
        debug_notification_number=debug,
        notification_override_number=override,
    )
    return service, backend, sender


def sent_names(sender):
    return [s[1] for s in sender.sent]


# ------------------------------ Paginación ------------------------------


def test_paginates_while_pages_come_full():
    orders = [office_order(str(i), f"G{i}") for i in range(5)]
    source = FakeDropiSource(orders, details={str(i): order_detail(str(i), f"G{i}") for i in range(5)})
    service, backend, _ = build_service(source)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL, result_number=2, start=0)

    # count 2 == result_number -> siguiente página con start += count
    assert [call[4] for call in source.order_list_calls] == [0, 2, 4]
    assert report.pages == 3
    assert report.total_orders == 5
    assert backend.lookups == [[f"G{i}" for i in range(5)]]  # una sola consulta en lote


def test_requests_only_office_status():
    source = FakeDropiSource([])
    service, _, _ = build_service(source)

    service.notify_office_orders(BUSINESS_ID, FROM, UNTIL, result_number=3)

    assert source.order_list_calls == [("RECLAME EN OFICINA", FROM, UNTIL, 3, 0)]


def test_last_page_exactly_full_requests_one_more_empty_page():
    orders = [office_order(str(i), f"G{i}") for i in range(4)]
    source = FakeDropiSource(orders, details={str(i): order_detail(str(i), f"G{i}") for i in range(4)})
    service, _, _ = build_service(source)

    service.notify_office_orders(BUSINESS_ID, FROM, UNTIL, result_number=2, start=0)

    assert [call[4] for call in source.order_list_calls] == [0, 2, 4]


def test_pagination_starts_from_requested_start():
    orders = [office_order(str(i), f"G{i}") for i in range(14)]
    source = FakeDropiSource(orders, details={})
    service, _, _ = build_service(source)

    service.notify_office_orders(BUSINESS_ID, FROM, UNTIL, result_number=3, start=10)

    assert [call[4] for call in source.order_list_calls] == [10, 13]


def test_repeated_pages_stop_pagination():
    # Un webhook que ignora `start` devolvería siempre la misma página llena.
    orders = [office_order("1", "G1"), office_order("2", "G2")]
    source = FakeDropiSource(orders, details={"1": order_detail("1", "G1"), "2": order_detail("2", "G2")},
                             ignore_start=True)
    service, _, _ = build_service(source)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL, result_number=2)

    assert len(source.order_list_calls) == 2
    assert report.total_orders == 2
    assert report.new_notifications == 2
    assert any("repetida" in error for error in report.errors)


def test_orders_page_failure_processes_orders_already_fetched():
    orders = [office_order(str(i), f"G{i}") for i in range(4)]
    source = FakeDropiSource(orders, details={str(i): order_detail(str(i), f"G{i}") for i in range(4)},
                             fail_on_start=2)
    service, _, _ = build_service(source)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL, result_number=2)

    assert report.total_orders == 2
    assert report.new_notifications == 2
    assert "ordenes start=2" in report.errors


# ------------------------------ Escalación ------------------------------


def test_new_guide_sends_video_then_pedido_oficina_and_stores_order():
    source = FakeDropiSource([office_order("93632719", "240063856178")],
                             details={"93632719": order_detail("93632719", "240063856178")})
    videos = FakeVideos(media_id="media-asad")
    service, backend, sender = build_service(source, videos=videos)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert report.new_notifications == 1
    assert report.videos == 1
    assert source.order_calls == ["93632719"]  # única consulta al MCP
    assert sent_names(sender) == ["video_producto", "pedido_oficina"]
    assert videos.calls == [(BUSINESS_ID, ["Collar Girasol GIRATORIO + Caja Cuerina"])]

    to_number, _, text = sender.sent[1]
    assert to_number == "573105409038"
    assert "tu pedido de Collar Girasol GIRATORIO + Caja Cuerina ya está en la oficina" in text
    assert "El  número de guia es: 240063856178" in text
    assert "En  la oficina de: Oficina Interrapidisimo, TARAZA" in text
    assert "Y debes pagar: *$84.900*" in text

    details = backend.registered[0]["details"]
    assert details["order_id"] == "93632719"
    assert details["products"] == [{"product_id": 2298431, "product_name": "Collar Girasol GIRATORIO + Caja Cuerina",
                                    "quantity": 1}]  # los precios de N8N no se guardan

    # Historial: el video con su media_id y luego el texto.
    assert [(m["media_type"], m["media_id"]) for m in backend.outgoing] == [("video", "media-asad"), ("text", None)]
    assert backend.notified == ["240063856178"]


def test_new_guide_without_product_video_sends_only_pedido_oficina():
    source = FakeDropiSource([office_order("1", "G1")], details={"1": order_detail("1", "G1")})
    service, backend, sender = build_service(source, videos=FakeVideos(media_id=None))

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert sent_names(sender) == ["pedido_oficina"]
    assert report.new_notifications == 1
    assert report.videos == 0
    assert report.without_video == ["G1"]
    assert backend.notified == ["G1"]


@pytest.mark.parametrize("count", [1, 2, 3, 10])
def test_following_days_send_only_pedido_oficina_without_limit(count):
    # Sin tope: mientras el pedido siga en oficina se notifica cada día.
    source = FakeDropiSource([office_order("1", "G1")])
    backend = FakeBackend({"G1": guide("G1", count=count, details=stored_details(), products=stored_products())})
    videos = FakeVideos()
    service, backend, sender = build_service(source, backend, videos=videos)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert sent_names(sender) == ["pedido_oficina"]
    assert "Collar Girasol" in sender.sent[0][2]
    assert report.reminders == 1
    assert source.order_calls == []  # datos guardados: sin consultar a Dropi
    assert videos.calls == []
    assert backend.registered == []
    assert backend.notified == ["G1"]


@pytest.mark.parametrize("count", [0, 1, 7])
def test_guide_notified_today_sends_nothing(count):
    source = FakeDropiSource([office_order("1", "G1")])
    backend = FakeBackend({"G1": guide("G1", count=count, last=TODAY, details=stored_details(),
                                       products=stored_products())})
    service, backend, sender = build_service(source, backend)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert sender.sent == []
    assert report.skipped == ["G1"]
    assert backend.notified == []


def test_guide_without_stored_order_fetches_it_once_and_saves_it():
    # Guía registrada por el flujo de PDF/sheet, sin datos del pedido.
    source = FakeDropiSource([office_order("1", "G1")], details={"1": order_detail("1", "G1", product="Perfumero")})
    backend = FakeBackend({"G1": guide("G1", count=2)})
    service, backend, sender = build_service(source, backend)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert report.reminders == 1
    assert source.order_calls == ["1"]
    assert backend.registered[0]["details"]["products"][0]["product_name"] == "Perfumero"  # se guarda
    assert sent_names(sender) == ["pedido_oficina"]
    assert "Perfumero" in sender.sent[0][2]


def test_registered_guide_never_notified_is_retried_without_dropi_or_reregistering():
    # Primer envío fallido: la guía existe con count 0 y sus datos guardados.
    source = FakeDropiSource([office_order("1", "G1")])
    backend = FakeBackend({"G1": guide("G1", count=0, last=None, details=stored_details(),
                                       products=stored_products())})
    service, backend, sender = build_service(source, backend)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert report.new_notifications == 1
    assert source.order_calls == []
    assert backend.registered == []
    assert sent_names(sender) == ["video_producto", "pedido_oficina"]
    assert backend.notified == ["G1"]


def test_registered_guide_without_details_is_completed_and_notified():
    source = FakeDropiSource([office_order("1", "G1")], details={"1": order_detail("1", "G1")})
    backend = FakeBackend({"G1": guide("G1", count=0, last=None)})
    service, backend, sender = build_service(source, backend)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    # register_guide responde False (ya existía) pero no es una carrera:
    # solo se usó para guardar los datos del pedido.
    assert report.new_notifications == 1
    assert backend.registered[0]["details"]["order_id"] == "1"
    assert backend.notified == ["G1"]


def test_rejected_video_still_notifies_and_forgets_media_id():
    source = FakeDropiSource([office_order("1", "G1")], details={"1": order_detail("1", "G1")})
    videos = FakeVideos(media_id="media-vencido")
    service, backend, _ = build_service(source, sender=FakeSender(fail={"video_producto"}), videos=videos)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert videos.discarded == ["media-vencido"]  # se vuelve a subir la próxima vez
    assert report.new_notifications == 1
    assert report.without_video == ["G1"]
    assert backend.notified == ["G1"]
    assert [m["media_type"] for m in backend.outgoing] == ["text"]


def test_failed_pedido_oficina_does_not_mark_guide_as_notified():
    source = FakeDropiSource([office_order("1", "G1")], details={"1": order_detail("1", "G1")})
    service, backend, _ = build_service(source, sender=FakeSender(fail={"pedido_oficina"}))

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert report.errors == ["G1"]
    assert backend.registered  # queda registrada con count 0: se reintenta en la próxima corrida
    assert backend.notified == []
    assert [m["media_type"] for m in backend.outgoing] == ["video"]  # el video sí salió


# ------------------------------ Datos de la plantilla ------------------------------


def test_prepaid_order_has_nothing_to_pay():
    source = FakeDropiSource([office_order("1", "G1", rate_type="SIN RECAUDO")], details={"1": order_detail("1", "G1")})
    service, _, sender = build_service(source)

    service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert "Y debes pagar: *$0*" in sender.sent[-1][2]


def test_amount_falls_back_to_stored_total():
    source = FakeDropiSource([office_order("1", "G1", total=None)])
    backend = FakeBackend({"G1": guide("G1", details=stored_details(total=1234567.0), products=stored_products())})
    service, _, sender = build_service(source, backend)

    service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert "Y debes pagar: *$1.234.567*" in sender.sent[0][2]


def test_order_without_total_is_skipped():
    source = FakeDropiSource([office_order("1", "G1", total=None)])
    backend = FakeBackend({"G1": guide("G1", details=stored_details(total=None), products=stored_products())})
    service, backend, sender = build_service(source, backend)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert sender.sent == []
    assert report.skipped == ["G1"]


def test_full_address_adds_carrier_when_address_does_not_mention_it():
    detail = order_detail("1", "G1", address="Cra 5 # 10-20 Local 3")
    source = FakeDropiSource([office_order("1", "G1", shipping_company="ENVIA")], details={"1": detail})
    service, _, sender = build_service(source)

    service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert "En  la oficina de: ENVIA - Cra 5 # 10-20 Local 3, TARAZA" in sender.sent[-1][2]


def test_full_address_does_not_repeat_carrier_with_different_spelling():
    detail = order_detail("1", "G1", address="Oficina Inter Rapidísimo centro")
    source = FakeDropiSource([office_order("1", "G1", shipping_company="INTERRAPIDISIMO")], details={"1": detail})
    service, _, sender = build_service(source)

    service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert "En  la oficina de: Oficina Inter Rapidísimo centro, TARAZA" in sender.sent[-1][2]


def test_product_names_are_single_line_for_meta():
    detail = order_detail("1", "G1", product="Collar\nGirasol\t  +    Caja")
    source = FakeDropiSource([office_order("1", "G1")], details={"1": detail})
    service, backend, _ = build_service(source)

    service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert backend.registered[0]["details"]["products"][0]["product_name"] == "Collar Girasol + Caja"


# ------------------------------ Business ------------------------------


def test_run_uses_requested_business_for_guides_history_and_videos():
    source = FakeDropiSource([office_order("1", "G1")], details={"1": order_detail("1", "G1")})
    videos = FakeVideos()
    service, backend, _ = build_service(source, videos=videos)

    service.notify_office_orders(7, FROM, UNTIL)

    assert backend.registered[0]["business_id"] == 7
    assert {m["business_id"] for m in backend.outgoing} == {7}
    assert videos.calls[0][0] == 7


def test_guide_of_another_business_is_skipped():
    source = FakeDropiSource([office_order("1", "G1")])
    backend = FakeBackend({"G1": guide("G1", count=1, business_id=2)})
    service, backend, sender = build_service(source, backend)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert sender.sent == []
    assert report.skipped == ["G1"]
    assert backend.notified == []


# ------------------------------ Casos borde ------------------------------


def test_duplicate_tracking_codes_are_notified_once():
    source = FakeDropiSource([office_order("1", "G1"), office_order("2", "G1")], details={"1": order_detail("1", "G1")})
    service, _, _ = build_service(source)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert report.total_orders == 1
    assert report.new_notifications == 1


def test_orders_without_tracking_code_are_skipped():
    source = FakeDropiSource([office_order("1", ""), office_order("2", "G2")], details={"2": order_detail("2", "G2")})
    service, backend, _ = build_service(source)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert backend.lookups == [["G2"]]
    assert report.new_notifications == 1
    assert "pedido 1 sin guía" in report.skipped


def test_orders_in_another_status_are_skipped():
    # Si N8N no aplicara el filtro de estado, no se notifica un pedido entregado.
    source = FakeDropiSource([office_order("1", "G1", status="ENTREGADO"), office_order("2", "G2")],
                             details={"2": order_detail("2", "G2")})
    service, backend, _ = build_service(source)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert backend.lookups == [["G2"]]
    assert "G1 en estado ENTREGADO" in report.skipped


def test_backend_lookup_failure_sends_nothing():
    source = FakeDropiSource([office_order("1", "G1")], details={"1": order_detail("1", "G1")})
    service, _, sender = build_service(source, FakeBackend(lookup_fails=True))

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert sender.sent == []
    assert source.order_calls == []
    assert report.errors


def test_order_detail_failure_is_an_error():
    source = FakeDropiSource([office_order("1", "G1")], details={})
    service, backend, sender = build_service(source)

    report = service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert report.errors == ["G1"]
    assert sender.sent == []
    assert backend.registered == []


def test_override_redirects_sending_but_registers_real_recipient():
    source = FakeDropiSource([office_order("1", "G1")], details={"1": order_detail("1", "G1")})
    service, backend, sender = build_service(source, override="573009999999", debug="573008888888")

    service.notify_office_orders(BUSINESS_ID, FROM, UNTIL)

    assert [s[0] for s in sender.sent] == ["573009999999"] * 2 + ["573008888888"] * 2
    assert {m["user_phone"] for m in backend.outgoing} == {"573105409038"}


# ------------------------------ Política diaria ------------------------------


@pytest.mark.parametrize("count,last,expected", [
    (0, None, NotificationStep.INITIAL),
    (1, "2026-11-03T20:00:00", NotificationStep.REMINDER),
    (25, "2026-11-01T08:00:00", NotificationStep.REMINDER),  # sin tope
    (0, "2026-11-04T07:00:00", NotificationStep.STOP),  # ya se notificó hoy
    (4, "2026-11-04T23:59:59", NotificationStep.STOP),
])
def test_daily_step(count, last, expected):
    assert daily_step(count, last, today=date(2026, 11, 4)) is expected


# ------------------------------ Plantillas ------------------------------


def test_video_producto_sends_video_by_media_id_in_header():
    template = VideoProductoTemplate("media-123")

    payload = template.build_payload("573105409038")

    assert payload["template"]["name"] == "video_producto"
    assert payload["template"]["components"] == [
        {"type": "header", "parameters": [{"type": "video", "video": {"id": "media-123"}}]}
    ]
    assert (template.media_type(), template.log_media_id()) == ("video", "media-123")


def test_pedido_oficina_body_parameters_in_template_order():
    recipient = RecipientInfo(name="Cesar Ochoa", phone="573105409038", tracking_number="240063856178",
                              delivery_address="Oficina Interrapidisimo, TARAZA", product="Collar Girasol")

    payload = PedidoOficinaTemplate(recipient, "84.900").build_payload("573105409038")

    assert payload["template"]["name"] == "pedido_oficina"
    body = payload["template"]["components"][0]
    assert body["type"] == "body"
    assert [p["text"] for p in body["parameters"]] == [
        "Cesar Ochoa", "Collar Girasol", "240063856178", "Oficina Interrapidisimo, TARAZA", "84.900",
    ]


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


class FakeNotifier:
    def __init__(self):
        self.runs = []

    def notify_office_orders(self, *args):
        self.runs.append(args)


class FakeBusinessBackend:
    def __init__(self, exists):
        self.exists = exists

    def business_exists(self, business_id):
        return self.exists


def endpoint_client(exists=True):
    app = FastAPI()
    app.include_router(notify_dropi_router)
    notifier = FakeNotifier()
    app.dependency_overrides[get_dropi_notifier] = lambda: notifier
    app.dependency_overrides[get_backend_client] = lambda: FakeBusinessBackend(exists)
    return TestClient(app), notifier


def test_endpoint_runs_for_business_in_query():
    client, notifier = endpoint_client()

    response = client.post("/notify-dropi-mcp?business_id=3", json={"from": "2026-10-18", "until": "2026-10-20"})

    assert response.status_code == 202
    assert response.json()["business_id"] == 3
    assert notifier.runs == [(3, date(2026, 10, 18), date(2026, 10, 20), 100, 0)]


@pytest.mark.parametrize("query", ["", "?business_id=0", "?business_id=abc"])
def test_endpoint_requires_valid_business_id(query):
    client, notifier = endpoint_client()

    assert client.post(f"/notify-dropi-mcp{query}").status_code == 422
    assert notifier.runs == []


@pytest.mark.parametrize("exists,status", [(False, 404), (None, 503)])
def test_endpoint_rejects_unknown_business_or_unreachable_backend(exists, status):
    client, notifier = endpoint_client(exists=exists)

    assert client.post("/notify-dropi-mcp?business_id=99").status_code == status
    assert notifier.runs == []


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
        self.headers = {}

    def get(self, url, timeout=None, **kwargs):
        self.calls.append((url, kwargs))
        return self.responses.pop(0)


def test_client_sends_api_key_and_filters_with_status():
    session = FakeHttpSession([FakeResponse(200, {"items": [{"order_id": "1"}], "count": 1})])
    client = N8nDropiClient("https://n8n.test/webhook/dropi/", api_key="secreto", session=session)

    items, count = client.fetch_orders("RECLAME EN OFICINA", date(2026, 10, 10), date(2026, 10, 20), 1, 0)

    assert (items, count) == ([{"order_id": "1"}], 1)
    assert session.headers["api-key"] == "secreto"
    url, kwargs = session.calls[0]
    assert url == "https://n8n.test/webhook/dropi/ordenes"
    assert kwargs["json"] == {"from": "2026-10-10", "until": "2026-10-20", "result_number": 1, "start": 0,
                              "status": "RECLAME EN OFICINA"}


def test_client_retries_rate_limit_with_backoff():
    sleeps = []
    session = FakeHttpSession([FakeResponse(429), FakeResponse(200, {"order_id": "93632719"})])
    client = N8nDropiClient("https://n8n.test/webhook/dropi", api_key="k", session=session, sleep=sleeps.append)

    order = client.fetch_order("93632719")

    assert order == {"order_id": "93632719"}
    assert session.calls[0] == ("https://n8n.test/webhook/dropi/orden", {"params": {"order_id": "93632719"}})
    assert sleeps == [2.0]


def test_client_does_not_retry_rejected_api_key():
    session = FakeHttpSession([FakeResponse(403)])
    client = N8nDropiClient("https://n8n.test/webhook/dropi", api_key="mala", session=session, sleep=lambda s: None)

    assert client.fetch_order("1") is None
    assert len(session.calls) == 1


def test_client_gives_up_after_max_attempts():
    session = FakeHttpSession([FakeResponse(429), FakeResponse(429), FakeResponse(429)])
    client = N8nDropiClient("https://n8n.test/webhook/dropi", api_key="k", session=session, sleep=lambda s: None)

    assert client.fetch_order("1") is None
    assert len(session.calls) == 3

    session = FakeHttpSession([FakeResponse(503)] * 3)
    client = N8nDropiClient("https://n8n.test/webhook/dropi", api_key="k", session=session, sleep=lambda s: None)
    with pytest.raises(Exception):
        client.fetch_orders(OFFICE_STATUS, FROM, UNTIL, 100, 0)
