"""Tests del servicio de notificación masiva desde Google Sheet.

Todo con dobles de prueba: sin Google, sin Meta, sin backend real.
"""

import polars as pl
import pytest

from app.services.sheet_notifier import SheetNotificationService


# ------------------------------ Fakes ------------------------------


class FakeSheetSource:
    COLUMNS = {
        "NOMBRE": pl.String,
        "TELEFONO": pl.String,
        "DIRECCION": pl.String,
        "CIUDAD": pl.String,
        "DEPARTAMENTO": pl.String,
        "PRODUCTO": pl.String,
        "Guia pdf": pl.String,
        "numero_guia": pl.String,
    }

    def __init__(self, office_rows, delivered_rows, pdf_bytes=b"%PDF-fake"):
        self.office = self._frame(office_rows)
        self.delivered = self._frame(delivered_rows)
        self.pdf_bytes = pdf_bytes
        self.downloads = []

    @classmethod
    def _frame(cls, rows):
        if not rows:
            return pl.DataFrame(schema=cls.COLUMNS)
        return pl.DataFrame(rows, schema_overrides=cls.COLUMNS)

    def fetch_sheet(self, document_id, sheet_gid):
        return self.office if sheet_gid == "office-gid" else self.delivered

    def download_file(self, url):
        self.downloads.append(url)
        return self.pdf_bytes


class FakeBackend:
    def __init__(self, existing_guides=(), guide_counts=None, sheet_config=None):
        # existing_guides: guías notificadas 1 vez (atajo); guide_counts: {guia: count}
        self.guide_counts = {n: 1 for n in existing_guides}
        self.guide_counts.update(guide_counts or {})
        self.sheet_config = sheet_config or {
            "document_id": "doc1",
            "office_id": "office-gid",
            "delivered_id": "delivered-gid",
        }
        self.guides_registered = []
        self.guides_notified = []
        self.outgoing = []

    def get_business_sheet(self, business_id):
        return self.sheet_config

    def get_guide(self, number):
        if number in self.guide_counts:
            return {
                "number": number,
                "notification_count": self.guide_counts[number],
                "last_notification_timestamp": getattr(self, "guide_last_notified", {}).get(number),
            }
        return None

    def register_guide(self, number, user_phone, user_name, business_id):
        if number in self.guide_counts:
            return False
        self.guide_counts[number] = 0
        self.guides_registered.append(number)
        return True

    def mark_guide_notified(self, number):
        self.guides_notified.append(number)
        if number in self.guide_counts:
            self.guide_counts[number] += 1

    def register_outgoing_message(self, **kwargs):
        self.outgoing.append(kwargs)

    # Métodos no usados por este servicio
    def fetch_authorized_associates(self): return {}
    def register_incoming_message(self, **kwargs): pass
    def update_message_status(self, *a): pass


class FakeSender:
    def __init__(self):
        self.sent = []
        self._n = 0

    def send_template(self, to_number, template):
        self._n += 1
        self.sent.append((to_number, template.name, template.log_message()))
        return f"wamid.sheet.{self._n}"


class FakeUploader:
    def __init__(self):
        self.uploads = []

    def upload_media(self, data, filename):
        self.uploads.append(filename)
        return f"media-{len(self.uploads)}"


def office_row(nombre, telefono, guia, pdf_url="https://files.test/guia.pdf"):
    return {
        "NOMBRE": nombre,
        "TELEFONO": telefono,
        "DIRECCION": "Oficina Interrapidísimo",
        "CIUDAD": "POPAYAN",
        "DEPARTAMENTO": "CAUCA",
        "PRODUCTO": "1 Collar girasol",
        "Guia pdf": pdf_url,
        "numero_guia": guia,
    }


def delivered_row(guia):
    return {"numero_guia": guia, "TELEFONO": "3000000000", "NOMBRE": "X", "DIRECCION": "Y", "CIUDAD": "Z", "DEPARTAMENTO": "W", "PRODUCTO": "P"}


def build_service(office_rows, delivered_rows, existing_guides=(), guide_counts=None):
    sheet_source = FakeSheetSource(office_rows, delivered_rows)
    backend = FakeBackend(existing_guides, guide_counts)
    sender = FakeSender()
    uploader = FakeUploader()
    service = SheetNotificationService(
        sheet_source=sheet_source,
        backend=backend,
        message_sender=sender,
        media_uploader=uploader,
    )
    return service, sheet_source, backend, sender, uploader


# ------------------------------ Tests ------------------------------


def test_guide_in_both_sheets_does_nothing():
    service, sheet_source, backend, sender, uploader = build_service(
        office_rows=[office_row("Juan", "3117039771", "G1")],
        delivered_rows=[delivered_row("G1")],  # ya reclamó
    )

    report = service.notify_business_sheet(1)

    assert report.total_pending == 0
    assert sender.sent == []
    assert backend.guides_registered == []
    assert sheet_source.downloads == []


def test_pending_guide_already_notified_sends_reminder():
    service, _, backend, sender, uploader = build_service(
        office_rows=[office_row("Juan", "3117039771", "G2")],
        delivered_rows=[],
        existing_guides={"G2"},  # ya fue notificada antes
    )

    report = service.notify_business_sheet(1)

    assert report.reminders == 1
    assert report.new_notifications == 0
    # Solo la plantilla recordatorio, sin PDF ni registro nuevo
    assert [name for _, name, _ in sender.sent] == ["recordatorio"]
    assert sender.sent[0][0] == "573117039771"  # normalizado +57
    assert "Tu pedido te espera" in sender.sent[0][2]
    assert uploader.uploads == []
    assert backend.guides_notified == ["G2"]
    # El recordatorio también queda en el historial del chat
    assert len(backend.outgoing) == 1
    assert backend.outgoing[0]["media_type"] == "text"


def test_pending_guide_never_notified_sends_both_templates():
    service, sheet_source, backend, sender, uploader = build_service(
        office_rows=[office_row("Kevin Yande", "3117039771", "G3")],
        delivered_rows=[],
    )

    report = service.notify_business_sheet(1)

    assert report.new_notifications == 1
    assert report.reminders == 0
    # PDF descargado del sheet y subido a Meta
    assert sheet_source.downloads == ["https://files.test/guia.pdf"]
    assert uploader.uploads == ["Guia_G3.pdf"]
    # Las dos plantillas en orden, con el texto completo guardado
    assert [name for _, name, _ in sender.sent] == ["guia", "mensaje_guia_es"]
    assert sender.sent[0][2] == "Guia de envio de tu pedido"
    assert "Tu pedido G3 con el producto 1 Collar girasol" in sender.sent[1][2]
    assert "POPAYAN, CAUCA" in sender.sent[1][2]
    # Guía registrada y marcada; mensajes en el historial
    assert backend.guides_registered == ["G3"]
    assert backend.guides_notified == ["G3"]
    assert len(backend.outgoing) == 2
    assert backend.outgoing[0]["media_id"] == "media-1"


def test_rows_without_guide_number_are_skipped():
    service, _, backend, sender, _ = build_service(
        office_rows=[office_row("Sin Guia", "3117039771", "")],
        delivered_rows=[],
    )

    report = service.notify_business_sheet(1)

    assert report.total_pending == 0
    assert sender.sent == []


def test_missing_pdf_url_is_an_error():
    service, _, backend, sender, _ = build_service(
        office_rows=[office_row("Juan", "3117039771", "G4", pdf_url="")],
        delivered_rows=[],
    )

    report = service.notify_business_sheet(1)

    assert report.errors == ["G4"]
    assert sender.sent == []
    assert backend.guides_registered == []


def test_business_without_sheet_config_reports_error():
    service, _, backend, sender, _ = build_service([], [])
    backend.sheet_config = None

    report = service.notify_business_sheet(99)

    assert report.errors
    assert sender.sent == []


def test_mixed_scenario_full_flow():
    service, _, backend, sender, _ = build_service(
        office_rows=[
            office_row("Reclamado", "3117039771", "G10"),     # en ambas -> nada
            office_row("Ya Notificado", "3222222222", "G11"),  # -> recordatorio
            office_row("Nuevo", "3333333333", "G12"),          # -> 2 plantillas
            office_row("Incompleto", "", "G13"),               # sin teléfono -> skip
        ],
        delivered_rows=[delivered_row("G10")],
        existing_guides={"G11"},
    )

    report = service.notify_business_sheet(1)

    assert report.total_pending == 3  # G10 queda fuera del anti-join
    assert report.reminders == 1
    assert report.new_notifications == 1
    assert report.skipped == ["G13"]
    templates_sent = [name for _, name, _ in sender.sent]
    assert templates_sent == ["recordatorio", "guia", "mensaje_guia_es"]


# ------------------------------ Override / debug ------------------------------


def test_override_redirects_sending_but_registers_real_recipient():
    service, _, backend, sender, _ = build_service(
        office_rows=[office_row("Nuevo", "3333333333", "G20")],
        delivered_rows=[],
    )
    service._notification_override_number = "573003579384"

    report = service.notify_business_sheet(1)

    assert report.new_notifications == 1
    # El envío físico va al número de pruebas...
    assert all(to == "573003579384" for to, _, _ in sender.sent)
    # ...pero el historial se registra con el destinatario real
    assert all(o["user_phone"] == "573333333333" for o in backend.outgoing)


def test_debug_number_receives_copy():
    service, _, backend, sender, _ = build_service(
        office_rows=[office_row("Ya Notificado", "3222222222", "G21")],
        delivered_rows=[],
        existing_guides={"G21"},
    )
    service._debug_notification_number = "573003579384"

    report = service.notify_business_sheet(1)

    assert report.reminders == 1
    # Una al destinatario real y una copia al número debug
    targets = [to for to, _, _ in sender.sent]
    assert targets == ["573222222222", "573003579384"]


# ------------------------------ Escalación de recordatorios ------------------------------


def test_guide_notified_twice_gets_final_reminder():
    service, _, backend, sender, _ = build_service(
        office_rows=[office_row("Juan", "3117039771", "G30")],
        delivered_rows=[],
        guide_counts={"G30": 2},  # notificada 2 veces
    )

    report = service.notify_business_sheet(1)

    assert report.final_reminders == 1
    assert report.reminders == 0
    assert [name for _, name, _ in sender.sent] == ["recordatorio_final"]
    assert "Reclama tu pedido" in sender.sent[0][2]


def test_guide_at_max_notifications_is_skipped():
    service, _, backend, sender, _ = build_service(
        office_rows=[office_row("Juan", "3117039771", "G31")],
        delivered_rows=[],
        guide_counts={"G31": 3},  # máximo alcanzado
    )

    report = service.notify_business_sheet(1)

    assert report.skipped == ["G31"]
    assert sender.sent == []
    assert backend.guides_notified == []


# ------------------------------ Duplicados y regla de 1/día ------------------------------


def test_duplicate_guide_rows_in_office_send_only_once():
    """La hoja office a veces repite la misma guía en varias filas; sin el
    dedup escalaría recordatorio + recordatorio_final en la misma corrida."""
    service, _, backend, sender, _ = build_service(
        office_rows=[
            office_row("Juan", "3117039771", "G40"),
            office_row("Juan Duplicado", "3117039771", "G40"),  # misma guía
        ],
        delivered_rows=[],
    )

    report = service.notify_business_sheet(1)

    assert report.total_pending == 1  # deduplicada
    assert report.new_notifications == 1
    assert [name for _, name, _ in sender.sent] == ["guia", "mensaje_guia_es"]


def test_guide_already_notified_today_is_skipped():
    from datetime import datetime

    service, _, backend, sender, _ = build_service(
        office_rows=[office_row("Juan", "3117039771", "G41")],
        delivered_rows=[],
        guide_counts={"G41": 1},
    )
    backend.guide_last_notified = {"G41": datetime.now().isoformat()}  # notificada HOY

    report = service.notify_business_sheet(1)

    assert report.skipped == ["G41"]
    assert sender.sent == []


def test_guide_notified_yesterday_gets_reminder():
    from datetime import datetime, timedelta

    service, _, backend, sender, _ = build_service(
        office_rows=[office_row("Juan", "3117039771", "G42")],
        delivered_rows=[],
        guide_counts={"G42": 1},
    )
    yesterday = (datetime.now() - timedelta(days=1)).isoformat()
    backend.guide_last_notified = {"G42": yesterday}

    report = service.notify_business_sheet(1)

    assert report.reminders == 1
    assert [name for _, name, _ in sender.sent] == ["recordatorio"]
