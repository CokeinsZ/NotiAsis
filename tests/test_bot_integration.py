"""Tests del flujo del bot y su integración con el backend.

Todo se prueba con dobles de prueba: sin Meta, sin DeepSeek y sin
base de datos.
"""

from app.core.phones import normalize_phone
from app.models.shipping import RecipientInfo
from app.services.associate_directory import AssociateDirectory
from app.services.shipping_notifier import ShippingNotificationService
from app.services.webhook_processor import WhatsAppWebhookProcessor
from app.whatsapp.client import WhatsAppClient

# ------------------------------ Fakes ------------------------------


class FakeBackend:
    def __init__(self, guides_created: bool = True):
        self.guides_created = guides_created
        self.guides_registered = []
        self.guides_notified = []
        self.incoming = []
        self.outgoing = []
        self.status_updates = []
        self.associates = {}

    def fetch_authorized_associates(self):
        return self.associates

    def register_guide(self, number, user_phone, user_name):
        self.guides_registered.append((number, user_phone, user_name))
        return self.guides_created

    def mark_guide_notified(self, number):
        self.guides_notified.append(number)

    def register_incoming_message(self, **kwargs):
        self.incoming.append(kwargs)

    def register_outgoing_message(self, **kwargs):
        self.outgoing.append(kwargs)

    def update_message_status(self, meta_message_id, status):
        self.status_updates.append((meta_message_id, status))


class FakeMediaRepository:
    def get_media_url(self, media_id):
        return "http://fake-url"

    def download_media(self, media_url):
        return b"%PDF-fake"


class FakeSender:
    def __init__(self):
        self.sent = []
        self._counter = 0

    def send_template(self, to_number, template):
        self._counter += 1
        meta_id = f"wamid.fake.{self._counter}"
        self.sent.append((to_number, template.name, meta_id))
        return meta_id


class FakePdfExtractor:
    def extract_text(self, data):
        return "texto de la guia"


class FakeRecipientExtractor:
    def __init__(self, recipient=None):
        self._recipient = recipient or RecipientInfo(
            name="Juan Perez",
            phone="+573001234567",
            tracking_number="GUIA123",
            delivery_address="Calle 1 # 2-3",
            product="Collar Girasol",
        )

    def extract(self, text):
        return self._recipient


def build_notifier(backend, sender, directory=None, recipient=None):
    return ShippingNotificationService(
        media_repository=FakeMediaRepository(),
        message_sender=sender,
        pdf_extractor=FakePdfExtractor(),
        recipient_extractor=FakeRecipientExtractor(recipient),
        backend=backend,
        associate_directory=directory or AssociateDirectory({"573003579384": 1}),
        debug_notification_number=None,
        notification_override_number=None,
    )


# ------------------------------ normalize_phone ------------------------------


def test_normalize_phone():
    assert normalize_phone("+573003579384") == "573003579384"
    assert normalize_phone("573003579384") == "573003579384"
    assert normalize_phone(" 573003579384 ") == "573003579384"


# ------------------------------ AssociateDirectory ------------------------------


def test_directory_loads_from_backend():
    backend = FakeBackend()
    backend.associates = {"573003579384": 1, "+573205363052": 2}
    directory = AssociateDirectory.load(backend, fallback_numbers={"579999"})

    assert directory.is_authorized("573003579384")
    assert directory.is_authorized("+573003579384")  # con o sin '+'
    assert directory.is_authorized("573205363052")
    assert not directory.is_authorized("571112223333")
    assert directory.business_id_for("573003579384") == 1
    assert directory.business_id_for("+573205363052") == 2


def test_directory_falls_back_to_local_list():
    backend = FakeBackend()  # sin asociados
    directory = AssociateDirectory.load(backend, fallback_numbers={"573003579384", "+573126866924"})

    assert directory.is_authorized("573003579384")
    assert directory.is_authorized("573126866924")
    assert directory.business_id_for("573003579384") is None


# ------------------------------ ShippingNotificationService ------------------------------


def test_new_guide_sends_templates_and_registers_everything():
    backend = FakeBackend(guides_created=True)
    sender = FakeSender()
    notifier = build_notifier(backend, sender)

    result = notifier.notify_pdf_guide("MEDIA123", associate_phone="573003579384")

    assert result is True
    # Guía registrada con el teléfono normalizado (sin '+')
    assert backend.guides_registered == [("GUIA123", "573001234567", "Juan Perez")]
    # Dos plantillas enviadas al destinatario real (normalizado), en orden
    assert [(to, name) for to, name, _ in sender.sent] == [
        ("573001234567", "guia"),
        ("573001234567", "mensaje_guia_es"),
    ]
    # Los dos mensajes registrados en el backend con el business del asociado
    assert len(backend.outgoing) == 2
    assert all(o["business_id"] == 1 for o in backend.outgoing)
    assert all(o["user_phone"] == "573001234567" for o in backend.outgoing)
    assert backend.outgoing[0]["media_type"] == "document"
    assert backend.outgoing[0]["media_id"] == "MEDIA123"
    assert backend.outgoing[1]["media_type"] == "text"
    assert "Collar Girasol" in backend.outgoing[1]["message"]
    # meta_message_ids reales propagados
    assert backend.outgoing[0]["meta_message_id"] == "wamid.fake.1"
    # Guía marcada como notificada
    assert backend.guides_notified == ["GUIA123"]


def test_duplicate_guide_is_not_notified_again():
    backend = FakeBackend(guides_created=False)  # guía ya existía
    sender = FakeSender()
    notifier = build_notifier(backend, sender)

    result = notifier.notify_pdf_guide("MEDIA123", associate_phone="573003579384")

    assert result is False
    assert sender.sent == []          # no se envió nada
    assert backend.outgoing == []     # no se registró nada
    assert backend.guides_notified == []


def test_override_redirects_sending_but_registers_real_recipient():
    backend = FakeBackend(guides_created=True)
    sender = FakeSender()
    notifier = build_notifier(backend, sender)
    notifier._notification_override_number = "573003579384"

    notifier.notify_pdf_guide("MEDIA123", associate_phone="573003579384")

    # El envío físico va al número de pruebas...
    assert all(to == "573003579384" for to, _, _ in sender.sent)
    # ...pero el historial se registra con el destinatario real
    assert all(o["user_phone"] == "573001234567" for o in backend.outgoing)


def test_incomplete_recipient_is_not_notified():
    backend = FakeBackend()
    sender = FakeSender()
    incomplete = RecipientInfo(name="Juan", phone="+573001234567")  # sin guía ni producto
    notifier = build_notifier(backend, sender, recipient=incomplete)

    assert notifier.notify_pdf_guide("MEDIA123", associate_phone="573003579384") is False
    assert backend.guides_registered == []
    assert sender.sent == []


# ------------------------------ WhatsAppWebhookProcessor ------------------------------


def build_processor(backend, notifier=None, directory=None):
    return WhatsAppWebhookProcessor(
        notifier=notifier or build_notifier(backend, FakeSender()),
        associate_directory=directory or AssociateDirectory({"573003579384": 1}),
        backend=backend,
    )


def test_status_updates_are_forwarded_to_backend():
    backend = FakeBackend()
    processor = build_processor(backend)

    # Payload real de Meta (jsonp.json, evento de estados)
    processor.handle_event({
        "entry": [{"changes": [{"value": {
            "statuses": [
                {"id": "wamid.abc", "status": "delivered", "timestamp": "1787200430"},
                {"id": "wamid.def", "status": "read", "timestamp": "1787200450"},
                {"id": "wamid.ghi", "status": "failed", "timestamp": "1787200460"},
            ],
        }}]}]
    })

    # 'failed' no está soportado por message_status_enum: se ignora
    assert backend.status_updates == [("wamid.abc", "delivered"), ("wamid.def", "read")]


def test_user_text_message_is_registered_as_incoming():
    backend = FakeBackend()
    processor = build_processor(backend)

    # Payload real de Meta (jsonp.json, mensaje de texto de usuario)
    processor.handle_event({
        "entry": [{"changes": [{"value": {
            "contacts": [{"profile": {"name": "Stiven Carvajal"}, "wa_id": "573009998887"}],
            "messages": [{
                "from": "573009998887",
                "id": "wamid.usermsg1",
                "timestamp": "1787200338",
                "type": "text",
                "text": {"body": "Hola"},
            }],
        }}]}]
    })

    assert backend.incoming == [{
        "user_phone": "573009998887",
        "user_name": "Stiven Carvajal",
        "meta_message_id": "wamid.usermsg1",
        "media_type": "text",
        "message": "Hola",
        "media_id": None,
        "timestamp": 1787200338,
    }]


def test_associate_pdf_triggers_guide_flow():
    backend = FakeBackend(guides_created=True)
    sender = FakeSender()
    notifier = build_notifier(backend, sender)
    processor = build_processor(backend, notifier=notifier)

    # Payload real de Meta (jsonp.json, documento PDF de un asociado)
    processor.handle_event({
        "entry": [{"changes": [{"value": {
            "contacts": [{"profile": {"name": "Stiven Carvajal"}, "wa_id": "573003579384"}],
            "messages": [{
                "from": "573003579384",
                "id": "wamid.pdf1",
                "timestamp": "1786869890",
                "type": "document",
                "document": {
                    "filename": "Guia_de_envio.pdf",
                    "mime_type": "application/pdf",
                    "id": "1543426957519806",
                },
            }],
        }}]}]
    })

    # El flujo completo se ejecutó (con fakes de Meta/DeepSeek)
    assert backend.guides_registered == [("GUIA123", "573001234567", "Juan Perez")]
    assert len(sender.sent) == 2
    # El PDF del asociado NO se registra como mensaje incoming del chat
    assert backend.incoming == []


def test_associate_non_pdf_message_is_ignored():
    backend = FakeBackend()
    processor = build_processor(backend)

    processor.handle_event({
        "entry": [{"changes": [{"value": {
            "messages": [{
                "from": "573003579384",
                "id": "wamid.assoc1",
                "timestamp": "1787200338",
                "type": "text",
                "text": {"body": "Ya envié la guía"},
            }],
        }}]}]
    })

    assert backend.incoming == []


def test_unsupported_user_message_type_is_ignored():
    backend = FakeBackend()
    processor = build_processor(backend)

    processor.handle_event({
        "entry": [{"changes": [{"value": {
            "messages": [{
                "from": "573009998887",
                "id": "wamid.sticker1",
                "timestamp": "1787200338",
                "type": "sticker",
                "sticker": {"id": "xyz"},
            }],
        }}]}]
    })

    assert backend.incoming == []


# ------------------------------ WhatsAppClient ------------------------------


class FakeResponse:
    def __init__(self, status_code, payload):
        self.status_code = status_code
        self._payload = payload
        self.text = str(payload)

    def json(self):
        return self._payload


class FakeSession:
    def __init__(self, response):
        self.response = response
        self.headers = {}
        self.posts = []

    def post(self, url, json, timeout):
        self.posts.append((url, json))
        return self.response

    def get(self, url, timeout, headers=None):
        return self.response


def test_send_template_returns_meta_message_id():
    session = FakeSession(FakeResponse(200, {"messages": [{"id": "wamid.REAL123"}]}))
    client = WhatsAppClient("token", "phone_id", session=session)

    from app.whatsapp.templates.guia import GuiaTemplate
    recipient = RecipientInfo(name="Juan", phone="573001234567")
    meta_id = client.send_template("573001234567", GuiaTemplate(recipient, "MEDIA1"))

    assert meta_id == "wamid.REAL123"


def test_send_template_error_returns_none():
    session = FakeSession(FakeResponse(400, {"error": "bad request"}))
    client = WhatsAppClient("token", "phone_id", session=session)

    from app.whatsapp.templates.guia import GuiaTemplate
    recipient = RecipientInfo(name="Juan", phone="573001234567")
    assert client.send_template("573001234567", GuiaTemplate(recipient, "MEDIA1")) is None


# ------------------------------ BackendClient auth ------------------------------


class FakeAuthSession:
    """Simula el backend: login con api_key y 401 cuando el token venció."""

    def __init__(self):
        self.headers = {}
        self.logins = 0
        self.requests = []
        self.valid_token = "token-v1"
        self.expire_first_token = False

    class _Resp:
        def __init__(self, status_code, payload):
            self.status_code = status_code
            self._payload = payload
            self.text = str(payload)

        def json(self):
            return self._payload

        def raise_for_status(self):
            if self.status_code >= 400:
                raise Exception(f"HTTP {self.status_code}")

    def post(self, url, json=None, timeout=None, headers=None):
        return self._handle("POST", url, json, headers)

    def get(self, url, timeout=None, headers=None):
        return self._handle("GET", url, None, headers)

    def patch(self, url, json=None, timeout=None, headers=None):
        return self._handle("PATCH", url, json, headers)

    def request(self, method, url, headers=None, timeout=None, **kwargs):
        return self._handle(method, url, kwargs.get("json"), headers)

    def _handle(self, method, url, json, headers):
        if url.endswith("/auth/api-key"):
            self.logins += 1
            if json.get("api_key") == "key-valida":
                return self._Resp(200, {"token": self.valid_token, "expires_in": 86400})
            return self._Resp(401, {"message": "Invalid api key"})

        self.requests.append((method, url, headers))
        token = (headers or {}).get("Authorization", "").removeprefix("Bearer ")
        if token != self.valid_token:
            return self._Resp(401, {"message": "Invalid or expired token"})
        if url.endswith("/associates"):
            return self._Resp(200, {"associates": [{"phone_number": "573003579384", "business_id": 1}]})
        return self._Resp(200, {"created": True})


def test_client_logs_in_and_sends_bearer_token():
    from app.services.backend_client import RustBackendClient

    client = RustBackendClient("http://fake", api_key="key-valida")
    session = FakeAuthSession()
    client._session = session

    associates = client.fetch_authorized_associates()

    assert session.logins == 1
    assert associates == {"573003579384": 1}
    assert session.requests[0][2]["Authorization"] == "Bearer token-v1"


def test_client_relogs_on_401_and_retries_once():
    from app.services.backend_client import RustBackendClient

    client = RustBackendClient("http://fake", api_key="key-valida")
    session = FakeAuthSession()
    client._session = session

    client.fetch_authorized_associates()   # login + request con token-v1
    session.valid_token = "token-v2"       # el servidor rota el token esperado

    client.fetch_authorized_associates()   # 401 con v1 → re-login (v2) → retry 200

    assert session.logins == 2
    assert session.requests[-1][2]["Authorization"] == "Bearer token-v2"


def test_client_survives_backend_down():
    from app.services.backend_client import RustBackendClient

    client = RustBackendClient("http://fake", api_key="key-valida")

    class DownSession:
        headers = {}

        def post(self, *a, **k):
            raise Exception("connection refused")

        def request(self, *a, **k):
            raise Exception("connection refused")

    client._session = DownSession()

    assert client.fetch_authorized_associates() == {}       # fail-open
    assert client.register_guide("G1", "57300", "Juan")     # no lanza


# ------------------------------ Texto completo de plantillas ------------------------------


def test_guia_template_full_text():
    from app.whatsapp.templates.guia import GuiaTemplate

    recipient = RecipientInfo(name="YEINER MARRUGO PÉREZ", phone="573001234567")
    template = GuiaTemplate(recipient, "MEDIA1")

    assert template.log_message() == "Guia de envio de tu pedido"
    assert template.media_type() == "document"
    assert template.log_media_id() == "MEDIA1"


def test_mensaje_guia_template_full_text():
    from app.whatsapp.templates.mensaje_guia import MensajeGuiaTemplate

    recipient = RecipientInfo(
        name="YEINER MARRUGO PÉREZ",
        phone="573001234567",
        tracking_number="240058393784",
        delivery_address="OFICINA DE INTERRAPIDÍSIMO (CLL REAL DEL COCO SEC LA CRUZ) TURBANA\\BOLI\\COL",
        product="Collar Girasol",
    )
    template = MensajeGuiaTemplate(recipient, "MEDIA1")

    assert template.log_message() == (
        "Es hora de recoger tu pedido\n"
        "Hola YEINER MARRUGO PÉREZ,\n"
        "\n"
        "Tu pedido 240058393784 con el producto Collar Girasol, ya está listo "
        "para recoger en OFICINA DE INTERRAPIDÍSIMO (CLL REAL DEL COCO SEC LA CRUZ) "
        "TURBANA\\BOLI\\COL.\n"
        "\n"
        "Por favor reclamar lo antes posible para evitar devoluciones por parte "
        "de la empresa transportadora.\n"
        "\n"
        "¡Disfruta!"
    )
