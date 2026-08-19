# NotiAsis

Bot de WhatsApp que recibe guías de envío en PDF, extrae los datos del
destinatario con DeepSeek y notifica al cliente mediante plantillas de la
WhatsApp Cloud API.

## Ejecutar

```bash
uvicorn app.main:app --host 0.0.0.0 --port 8000
```

O con Docker:

```bash
docker compose up --build
```

## Estructura del proyecto

```
app/
├── main.py                  # Punto de entrada: crea la app FastAPI
├── config.py                # Settings: configuración centralizada (variables de entorno)
├── core/
│   └── interfaces.py        # Puertos/abstracciones del dominio (ABC)
├── models/
│   └── shipping.py          # Modelos de dominio (RecipientInfo)
├── whatsapp/
│   ├── client.py            # WhatsAppClient: HTTP contra la Graph API de Meta
│   └── templates/           # Una clase por plantilla de WhatsApp
│       ├── base.py          #   TemplateMessage (contrato base)
│       └── pedido_enviado.py
├── services/
│   ├── pdf_extractor.py     # Extracción de texto de PDFs
│   ├── recipient_extractor.py  # Extracción de datos con DeepSeek
│   ├── shipping_notifier.py # Orquestador del flujo de notificación
│   └── webhook_processor.py # Interpreta los eventos del webhook de Meta
└── api/
    ├── dependencies.py      # Composition root (inyección de dependencias)
    └── webhook.py           # Router FastAPI (GET/POST /webhook)
```

## Agregar una nueva plantilla de WhatsApp

Cada plantilla registrada en Meta Business tiene su propia clase en
`app/whatsapp/templates/`. Para crear una nueva:

1. Crea un archivo nuevo en `app/whatsapp/templates/`, por ejemplo `mi_plantilla.py`.
2. Define una clase que herede de `TemplateMessage`:

```python
from app.whatsapp.templates.base import TemplateMessage

class MiPlantillaTemplate(TemplateMessage):
    TEMPLATE_NAME = "mi_plantilla"  # nombre registrado en Meta

    def __init__(self, dato: str) -> None:
        self._dato = dato

    @property
    def name(self) -> str:
        return self.TEMPLATE_NAME

    def build_components(self) -> list[dict]:
        return [
            {
                "type": "body",
                "parameters": [{"type": "text", "text": self._dato}],
            }
        ]
```

3. Envíala con el cliente:

```python
whatsapp_client.send_template("+573001234567", MiPlantillaTemplate("hola"))
```

No es necesario modificar `WhatsAppClient` ni el resto del código
(principio abierto/cerrado).

## Configuración

Ver `.env.example`. Los números autorizados (`ALLOWED_SENDER_NUMBERS`) y el
número de copia de depuración (`DEBUG_NOTIFICATION_NUMBER`) son configurables
por variables de entorno.
