# NotiAsis

Bot de WhatsApp que recibe guías de envío en PDF, extrae los datos del
destinatario con DeepSeek y notifica al cliente mediante plantillas de la
WhatsApp Cloud API.

Por cada guía recibida se envían dos plantillas, en orden:

1. `guia` — el PDF de la guía como documento.
2. `mensaje_guia_es` — nombre, número de guía, producto y dirección.

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
│   ├── interfaces.py        # Puertos/abstracciones del dominio (ABC)
│   └── phones.py            # Normalización de teléfonos (sin '+')
├── models/
│   └── shipping.py          # Modelos de dominio (RecipientInfo)
├── whatsapp/
│   ├── client.py            # WhatsAppClient: HTTP contra la Graph API de Meta
│   └── templates/           # Una clase por plantilla de WhatsApp
│       ├── base.py          #   TemplateMessage (contrato base)
│       ├── guia.py          #   GuiaTemplate (solo el PDF)
│       └── mensaje_guia.py  #   MensajeGuiaTemplate (solo la información)
├── services/
│   ├── pdf_extractor.py     # Extracción de texto de PDFs
│   ├── recipient_extractor.py  # Extracción de datos con DeepSeek
│   ├── shipping_notifier.py # Orquestador del flujo de notificación
│   ├── webhook_processor.py # Interpreta los eventos del webhook de Meta
│   ├── backend_client.py    # Cliente HTTP del backend Rust (NotificationBackend)
│   └── associate_directory.py  # Asociados autorizados en RAM (cargados del backend)
└── api/
    ├── dependencies.py      # Composition root (inyección de dependencias)
    └── webhook.py           # Router FastAPI (GET/POST /webhook)
tests/                       # Tests del bot (pytest, sin servicios externos)
```

## Integración bot (Python) ↔ backend (Rust)

El bot consume la API del backend (`BACKEND_API_URL`):

- **Al iniciar**: carga los `business_associates` autorizados desde
  `GET /associates` en memoria (`AssociateDirectory`). Si el backend no
  responde, usa el respaldo local `ALLOWED_SENDER_NUMBERS`.
- **Guía recibida (PDF de un asociado)**: tras extraer los datos,
  `POST /guides` decide si notificar (`created: false` = guía duplicada,
  no se vuelve a notificar). Tras enviar las plantillas registra cada
  mensaje con `POST /messages/outgoing` (crea el chat si no existe) y
  marca la guía con `POST /guides/{number}/notified`. Cada plantilla
  guarda en el historial su **texto completo renderizado** (el que Meta
  muestra al cliente) — si una plantilla cambia en Meta Business,
  actualiza su `log_message()` en `app/whatsapp/templates/`.
- **Mensaje de un usuario final**: `POST /messages/incoming`.
- **Estados que reporta Meta** (sent/delivered/read):
  `PATCH /messages/{meta_message_id}/status`.

Todos los teléfonos se normalizan sin '+' en ambos servicios antes de
guardarse o compararse.

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

Si la plantilla debe enviarse en el flujo de notificación de guías,
agrégala a `DEFAULT_TEMPLATE_FACTORIES` en
`app/services/shipping_notifier.py`.

## Configuración

Ver `.env.example`. Los números autorizados (`ALLOWED_SENDER_NUMBERS`), el
número de copia de depuración (`DEBUG_NOTIFICATION_NUMBER`) y el desvío de
notificaciones para pruebas (`NOTIFICATION_OVERRIDE_NUMBER`) son
configurables por variables de entorno.

## Backend web (Rust + Axum + PostgreSQL)

En `webapp/backend/`. API REST para la interfaz web (bandeja de chats,
historial de mensajes y envío de mensajes libres dentro de la ventana de
24h de Meta) y para el bot (registro de mensajes, estados y guías).

### Autenticación (JWT)

Todo el backend requiere `Authorization: Bearer <token>`, excepto `/auth/*`:

| Método | Ruta | Descripción |
|---|---|---|
| POST | `/auth/login` | Asociado (usuario + contraseña) → JWT de **15 min** con `business_id` y `phone_number` |
| POST | `/auth/api-key` | Bot (api_key de la tabla `api_keys`) → JWT de **24h**, acceso global |
| PATCH | `/associates/{id}/password` | Cambio de contraseña: pide la actual; solo el dueño de la cuenta (match con `phone_number` del JWT) |

- Los asociados solo acceden a los recursos de **su** business (403 si no).
- Cada mensaje libre enviado renueva el token del asociado otros 15 min
  (la respuesta incluye `renewed_token`).
- El bot lee `BACKEND_API_KEY` del entorno, obtiene su JWT solo, y lo
  renueva automáticamente si recibe un 401.

```bash
cd webapp/backend
cp .env.example .env   # configurar DATABASE_URL y credenciales de Meta
psql $DATABASE_URL -f db.sql
cargo run
cargo test             # 42 tests, no requieren base de datos
```

O todo junto con Docker (la DB puede ser externa, ej. Neon):

```bash
docker compose up --build   # bot (:8000) + backend (:3000) + frontend (:3001)
```

## Frontend web (Next.js + React + Tailwind)

En `webapp/frontend/`. Interfaz de conversaciones sobre fondo negro con
acentos azul claro de luna:

- **`/login`** — acceso con usuario y contraseña del asociado.
- **`/chats/{businessId}`** — bandeja de conversaciones (requiere JWT del
  business correspondiente; sin token redirige a `/login`):
  - Panel izquierdo: chats ordenados por el último mensaje del usuario
    (más reciente arriba), cada uno con nombre, **tiempo de gracia
    restante** de la ventana de 24h y el último mensaje del usuario.
    El chat seleccionado se ilumina con un gradiente blanco de luna.
  - Panel derecho: historial del chat con burbujas, estados
    (✓ / ✓✓ / ✓✓ azul) y envío de mensajes libres (deshabilitado cuando
    la ventana de 24h está cerrada; el backend responde 422).
  - Multimedia (PDFs, imágenes, audios): se carga bajo demanda como
    **blob en la memoria del navegador** — el backend hace de túnel hacia
    Meta (`GET /messages/media/{id}`) sin guardar nada en disco, y el
    navegador visualiza/reproduce el archivo (visor de PDF, `<img>`,
    `<audio>`) liberando la memoria al salir del chat.

```bash
cd webapp/frontend
npm ci
npm run dev            # http://localhost:3000 (o el puerto libre)
```

La URL del backend se configura con `BACKEND_URL` (por defecto
`http://localhost:3000`). El navegador nunca habla directo con el
backend: pasa por el proxy `/api/backend/*` (route handler en runtime),
así no hay CORS ni se expone la URL interna.

### Estructura

Cada módulo sigue la organización de `src-EJEMPLO`:
`dtos.rs` (modelos y validación) · `repository.rs` (trait + impl PostgreSQL)
· `service.rs` (trait + lógica de negocio) · `controller.rs` (rutas HTTP).

```
src/
├── main.rs        # build_app: composition root (pool, servicios, rutas)
├── state.rs       # Estados inyectados en los handlers
├── tools/         # responses y validadores compartidos
├── businesses/    # Empresas y asociados (quiénes pueden enviar guías)
├── users/         # Clientes finales (destinatarios de las guías)
├── chats/         # Bandeja de entrada + mensajes del chat + envío libre
├── messages/      # Registro incoming/outgoing, estados, cliente de Meta
└── guides/        # Registro y deduplicación de guías
```

### Endpoints

Para la webapp:

| Método | Ruta | Descripción |
|---|---|---|
| POST | `/businesses` | Crear empresa |
| GET | `/businesses` · `/businesses/{id}` | Listar / detalle |
| POST | `/businesses/{id}/associates` | Crear asociado (password con bcrypt) |
| GET | `/businesses/{id}/associates` | Listar asociados |
| GET | `/users` · `/users/{phone}` | Clientes finales |
| GET | `/chats?business_id={id}` | Bandeja: último mensaje + `window_open` |
| GET | `/chats/{business_id}/{user_phone}/messages` | Historial del chat |
| POST | `/chats/{business_id}/{user_phone}/messages` | Enviar mensaje libre (422 si la ventana de 24h está cerrada) |
| GET | `/messages/media/{media_id}` | Multimedia de Meta en memoria (túnel, sin tocar disco) con `Content-Disposition: inline` para visualizar en el navegador |
| GET | `/guides?user_phone=` | Guías registradas |

Para el bot (Python):

| Método | Ruta | Descripción |
|---|---|---|
| GET | `/associates/phones` | Números autorizados (reemplaza `ALLOWED_SENDER_NUMBERS`) |
| POST | `/messages/incoming` | Registrar mensaje entrante del webhook |
| POST | `/messages/outgoing` | Registrar plantilla enviada |
| PATCH | `/messages/{meta_message_id}/status` | Actualizar estado (`sent`/`delivered`/`read`) |
| POST | `/guides` | Registrar guía; responde `created: false` si es duplicada (no re-notificar) |
| POST | `/guides/{number}/notified` | Marcar cuándo se notificó la guía |

La sesión usa JWT en `localStorage` (15 min, renovado automáticamente al
enviar mensajes libres). Pendiente: refresh tokens y cookies httpOnly para
una sesión más robusta.
