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
│   ├── google_sheets.py     # Descarga de hojas de Google Sheets (polars)
│   ├── sheet_notifier.py    # Notificación masiva desde el Google Sheet
│   ├── dropi_client.py      # Cliente de los webhooks de N8N (MCP de Dropi)
│   ├── dropi_notifier.py    # Notificación de pedidos de Dropi en oficina
│   ├── product_videos.py    # Video de cada producto y caché de su media_id
│   └── associate_directory.py  # Asociados autorizados en RAM (cargados del backend)
└── api/
    ├── dependencies.py      # Composition root (inyección de dependencias)
    ├── notify_sheet.py      # POST /notify-sheet (notificación masiva)
    ├── notify_dropi.py      # POST /notify-dropi-mcp (pedidos de Dropi)
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
- **Mensaje de un usuario final**: `POST /messages/incoming`. Todos los
  business envían desde el mismo número de WhatsApp, así que el backend
  decide a qué chat va el mensaje (un cliente puede comprarle a varios):
  1. Si el cliente citó un mensaje al responder (o tocó un botón de una
     plantilla), Meta manda `context.id` y el bot lo envía como
     `reply_to_meta_message_id`: el mensaje va al business de la
     notificación citada y queda ligado a su guía (`guide_number`, que el
     panel muestra como "Responde a la guía X").
  2. Si no, va al chat del cliente con actividad más reciente (último
     mensaje enviado o recibido): a quien probablemente le está
     respondiendo.
- **Teléfonos sin indicativo** (Dropi, Google Sheet, PDF): se completan
  con el país del business (`countries.phone_prefix` y `phone_length`,
  que el bot lee de `GET /businesses/{id}`).
- **Estados que reporta Meta** (sent/delivered/read):
  `PATCH /messages/{meta_message_id}/status`.

Todos los teléfonos se normalizan sin '+' en ambos servicios antes de
guardarse o compararse (los números de 10 dígitos, como los del Google
Sheet, reciben el prefijo 57).

### Escalación de notificaciones (webhook y Google Sheet por igual)

Una guía se notifica como máximo 3 veces (`guides.notification_count`):

| Veces notificada antes | Plantillas enviadas |
|---|---|
| 0 (nueva) | `guia` + `mensaje_guia_es` |
| 1 | `recordatorio` |
| 2 | `recordatorio_final` |
| 3 o más | nada (máximo alcanzado) |

La decisión vive en `app/services/notification_policy.py`
(`step_for_notification_count`, función pura) y la usan tanto
`ShippingNotificationService` (PDF por WhatsApp) como
`SheetNotificationService` (Google Sheet).

Reglas duras adicionales: una guía nunca recibe más de una notificación
por día, y la corrida del sheet deduplica `numero_guia` antes de procesar
(la hoja office a veces repite filas de la misma guía).

Cada guía registrada guarda su `business_id` (del asociado que la envió
por WhatsApp o del business cuya hoja se procesó).

### Dashboard de notificaciones

`GET /businesses/{id}/stats/notifications?days=N` (JWT, solo el business
dueño) devuelve las notificaciones por día y tipo. La webapp lo muestra en
`/chats/{businessId}/dashboard` (icono de gráfica junto al nombre del
business): barras apiladas por día con selector de 7/30/90 días.

### Notificación masiva desde Google Sheet

`POST /notify-sheet` con `{"business_id": N}` (responde 202 y procesa en
segundo plano). El bot lee la configuración del sheet del business desde
el backend (`GET /businesses/{id}/sheet` → `business_users_sheet`):

1. Descarga las hojas `office_id` (todos los pedidos) y `delivered_id`
   (pedidos ya reclamados) como CSV con polars.
2. Anti-join por `numero_guia`: quedan solo las guías **no reclamadas**.
   Las filas sin `numero_guia` se omiten.
3. Por cada guía pendiente (sin llamar a DeepSeek: la hoja ya trae
   `NOMBRE`, `TELEFONO`, `DIRECCION`, `CIUDAD`, `DEPARTAMENTO`,
   `PRODUCTO`):
   - Si la guía ya fue notificada antes (`GET /guides/{number}`) →
     plantilla **recordatorio**.
   - Si nunca fue notificada → descarga el PDF de la columna
     `Guia pdf`, lo sube a Meta (`POST /{phone_id}/media`) y envía las
     plantillas **guia** + **mensaje_guia_es**, registra la guía y los
     mensajes en el chat.

### Notificación de pedidos de Dropi (MCP vía N8N)

`POST /notify-dropi-mcp?business_id=N` (responde 202 y procesa en
segundo plano). `business_id` es obligatorio: es el business dueño de los
chats e historial de la corrida (404 si no existe) y elige la cuenta de
Dropi que consulta N8N. Solo corre una notificación por business a la
vez: mientras la anterior no termine, otra llamada para el mismo business
responde 409 sin hacer nada (otros business sí corren en paralelo). El
body es opcional:

| Campo | Por defecto |
|---|---|
| `from` | hace 15 días (`YYYY-MM-DD`) |
| `until` | hoy |
| `result_number` | 100 |
| `start` | 0 |

```bash
curl -X POST "localhost:8000/notify-dropi-mcp?business_id=1" \
  -H "Content-Type: application/json" \
  -d '{"from": "2026-10-18", "until": "2026-10-20"}'
```

`DROPI_N8N_URL` apunta a los webhooks de N8N (por defecto
`https://n8n.notiasis.com/webhook/dropi`) y `N8N_API_KEY` es el header
`api-key` que exigen (obligatorio: sin él responde 503). N8N escoge la
credencial de Dropi según el `business_id`: con uno equivocado `/orden`
falla o responde que el pedido no está en la cuenta, y el pedido queda
como error en el reporte. Las guías que ya
pertenecen a otro business se omiten. El reporte de la corrida (nuevas,
cuántas con video, envíos de cada plantilla, omitidas, errores,
consultas a `/orden`) se imprime en los logs del bot al terminar.

1. Pagina `GET {DROPI_N8N_URL}/ordenes?business_id=N` (body JSON con los
   filtros y `"status": "RECLAME EN OFICINA"`): mientras `count == result_number`
   pide la siguiente página con `start += count`.
2. Consulta en lote al backend (`POST /guides/lookup`) qué guías
   (`tracking_code`) ya existen, cuántas veces se notificaron y sus datos
   de pedido guardados.
3. Una notificación por guía **por día, sin tope**: el pedido deja de
   notificarse cuando sale de la oficina (ya no aparece en `/ordenes`).

   | Notificación (`notification_count` actual) | Se envía |
   |---|---|
   | Día 1 (0) | `video_producto` (si el producto tiene video) + `mensaje_guia_es` |
   | Día 2 (1) | `recordatorio` |
   | Día 3 (2) | `recordatorio_final` |
   | Día 4 en adelante (3 o más) | `pedido_oficina` |

   "Día N" es la N-ésima notificación: si un día el pedido no se
   notifica, la escalación sigue donde iba. La guía cuenta como
   notificada solo si salió la plantilla principal del día (el video es
   opcional).

   `mensaje_guia_es` y `pedido_oficina` llevan la dirección completa
   (dirección, ciudad y transportadora si la dirección no la menciona);
   `pedido_oficina` además el valor a pagar (`84.900`; `0` si la tarifa
   es `SIN RECAUDO`).

El MCP de Dropi tiene rate limit, así que el detalle del pedido
(`GET {DROPI_N8N_URL}/orden?order_id=&business_id=N`) solo se consulta cuando la guía
no tiene guardados sus datos: la primera notificación (o una guía vieja
registrada por otro flujo). Esos datos (cliente y productos) se guardan
con la guía (`POST /guides` con `details`) y los días siguientes se leen
del backend; de los productos se guardan id, nombre y cantidad (los
precios que envía N8N se descartan). Ante 429/5xx el cliente reintenta
con espera exponencial.

#### Videos de producto

Los videos se leen del volumen montado en el bot (`MEDIA_HOST_DIR`, por
defecto `/home/debian/vicky-v/media` del servidor):

```
{MEDIA_HOST_DIR}/business/{business_id}/products/*.mp4
```

Para cada producto del pedido se busca, en orden:

1. `{product_id}.mp4` (id de producto de Dropi): amarra el video sin
   depender del nombre.
2. Un archivo cuyas palabras aparezcan **todas** en el nombre del
   producto, sin importar mayúsculas, tildes, signos ni conectores ("de",
   "x", "the"...). Una palabra de 4 letras o más también vale dentro de
   otra (`CLUB` en `CLUBTHE`). Si varios sirven, gana el de más palabras.

| Archivo | Productos que lo usan |
|---|---|
| `ASAD_CLUB_DE_NUIT.mp4` | `ASAD-CLUB DE NUIT`, `COMBO ASAD X CLUB DE NUIT`, `PERFUMES ASAD+CLUBTHE NUITFULL+PERFUMERO` |
| `CAYENNE_PEPPER.mp4` | `CAYENNE PEPPER PRIV` |

Se usa el video del primer producto que tenga uno. Si ninguno tiene, la
primera notificación sale solo con `mensaje_guia_es` (el log lo lista).
WhatsApp acepta MP4 de hasta 16 MB.

Cada video se sube a WhatsApp una sola vez: el `media_id` queda en un
caché (`/data/state/whatsapp_media.json`, volumen `bot-state`) y se
reutiliza en todos los envíos. Meta conserva el media 30 días, así que
se vuelve a subir a los 28 días, si el archivo cambia, o si Meta rechaza
el `media_id`.

Migración: volver a correr `webapp/backend/db.sql` sobre la base
existente (es idempotente) agrega las columnas de pedido a `guides` y la
tabla `guide_products` (y le quita `unit_price` y `subtotal` si se
habían creado).

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

- Los asociados solo acceden a los recursos de **su** business (403 si no):
  el detalle del business, sus chats, estadísticas y multimedia. Todo lo
  demás (crear business y asociados, clientes, guías, registrar mensajes)
  es **solo del bot** (token de api_key).
- Cada mensaje libre enviado renueva el token del asociado otros 15 min
  (la respuesta incluye `renewed_token`).
- El bot lee `BACKEND_API_KEY` del entorno, obtiene su JWT solo, y lo
  renueva automáticamente si recibe un 401.

```bash
cd webapp/backend
cp .env.example .env   # configurar DATABASE_URL y credenciales de Meta
psql $DATABASE_URL -f db.sql
cargo run
cargo test             # no requieren base de datos
```

`db.sql` es idempotente: sobre una base existente agrega lo que falte.
Antes de correrlo en una base vieja:

1. Revisar que no haya guías sin business (`SET NOT NULL` fallaría a
   mitad del script): `SELECT number, user_id FROM guides WHERE business_id IS NULL;`.
   Asignarles su business o borrarlas, y volver a correr el script.
2. Revisar que ningún teléfono de asociado esté repetido (el índice
   único fallaría): `SELECT phone_number FROM business_associates GROUP BY 1 HAVING count(*) > 1;`.
3. Los business existentes quedan en Colombia. Los de otro país se
   cambian a mano: `UPDATE businesses SET country_id = (SELECT id FROM countries WHERE code = 'GT') WHERE id = 2;`.
   Un país nuevo es una fila más en `countries` (`code`, `name`,
   `phone_prefix`, `phone_length`).

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
  - Panel izquierdo con el **nombre del business** en el header, chats
    ordenados: 1) **importantes** (se marcan con el icono `!`/`?` a la
    izquierda del nombre, vía `PATCH /chats/{bid}/{phone}/importance`),
    2) respuesta del usuario más reciente, 3) guía notificada más
    recientemente (`chats.last_guide_notification_at`).
  - **Búsqueda** (icono lupa): por nombre o teléfono. **Filtros** (icono
    embudo): Importantes / Respuestas recientes / Notificaciones
    recientes (últimas 24h). Con búsqueda o filtros activos solo se
    muestran los chats que los cumplen, hasta pulsar
    "Limpiar filtros/búsqueda".
  - El chat seleccionado se ilumina con un gradiente blanco de luna.
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
| GET | `/businesses/{id}` | Detalle con su país (`country`: `code`, `phone_prefix`, `phone_length`) |
| GET | `/businesses/{id}/stats/notifications` | Estadísticas del dashboard |
| GET | `/chats?business_id={id}` | Bandeja ordenada (importantes > respuesta reciente > guía notificada) + `window_open` |
| PATCH | `/chats/{bid}/{user_phone}/importance` | Marcar/desmarcar chat como importante |
| GET | `/chats/{business_id}/{user_phone}/messages` | Historial del chat |
| POST | `/chats/{business_id}/{user_phone}/messages` | Enviar mensaje libre (422 si la ventana de 24h está cerrada) |
| GET | `/messages/media/{media_id}` | Multimedia de Meta en memoria (túnel, sin tocar disco) con `Content-Disposition: inline` para visualizar en el navegador. Un asociado solo descarga la de mensajes de su business (404 si no) |

Solo para el bot (token de api_key; un asociado recibe 403) y la administración:

| Método | Ruta | Descripción |
|---|---|---|
| POST | `/businesses` | Crear empresa: `{"name", "country_id"}` (`SELECT id, code FROM countries`) |
| GET | `/businesses` | Listar empresas |
| POST | `/businesses/{id}/associates` | Crear asociado (password con bcrypt). El teléfono puede ir sin indicativo: se completa con el país del business (400 si no es de ese país). 409 si el teléfono o el usuario ya existen |
| GET | `/businesses/{id}/associates` · `/associates` | Listar asociados |
| GET | `/businesses/{id}/sheet` | Config del Google Sheet del business |
| GET | `/users` · `/users/{phone}` | Clientes finales |
| GET | `/guides?user_phone=` · `/guides/{number}` | Guías registradas / una guía (404 si no existe) |
| GET | `/associates/phones` | Números autorizados (reemplaza `ALLOWED_SENDER_NUMBERS`) |
| POST | `/messages/incoming` | Registrar mensaje entrante del webhook. `reply_to_meta_message_id` opcional: el mensaje que el cliente citó |
| POST | `/messages/outgoing` | Registrar plantilla enviada, con la guía que notifica (`guide_number`) |
| PATCH | `/messages/{meta_message_id}/status` | Actualizar estado (`sent`/`delivered`/`read`) |
| POST | `/guides` | Registrar guía; responde `created: false` si es duplicada (no re-notificar). `details` opcional (cliente y productos del pedido) se guarda aunque la guía ya existiera |
| POST | `/guides/lookup` | `{"numbers": [...]}` (máx. 500) → guías existentes con `notification_count`, `details` y `products` |
| POST | `/guides/{number}/notified` | Marcar cuándo se notificó la guía |

La sesión usa JWT en `localStorage` (15 min, renovado automáticamente al
enviar mensajes libres). Pendiente: refresh tokens y cookies httpOnly para
una sesión más robusta.
