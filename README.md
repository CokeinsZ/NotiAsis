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
│   ├── dropi_orders.py      # Base común de las notificaciones de Dropi
│   ├── dropi_notifier.py    # Notificación de pedidos de Dropi en oficina
│   ├── dropi_tracking_notifier.py  # Seguimiento del envío de Dropi por niveles
│   ├── product_videos.py    # Video de cada producto y caché de su media_id
│   └── associate_directory.py  # Asociados autorizados en RAM (cargados del backend)
└── api/
    ├── dependencies.py      # Composition root (inyección de dependencias)
    ├── notify_sheet.py      # POST /notify-sheet (notificación masiva)
    ├── notify_dropi.py      # POST /notify-dropi-mcp y /notify-dropi-tracking (Dropi)
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
  que el bot lee de `GET /businesses/{id}`). Un teléfono con otra
  longitud (mal digitado) queda tal cual. El backend acepta de 8 a 15
  dígitos en guías, mensajes y asociados; si aun así rechaza una guía
  (4xx), el bot no la notifica, porque sin la guía registrada la
  notificación no se puede marcar y se repetiría en cada corrida.
- **Estados que reporta Meta** (sent/delivered/read):
  `PATCH /messages/{meta_message_id}/status`.
- **Mensajes que Meta acepta pero no entrega** (webhook con estado
  `failed`, ej. 131048 *Spam Rate limit hit* al pasar el límite diario):
  `POST /messages/{meta_message_id}/errors` con el estado completo de
  Meta. El backend guarda el error en `messages_errors`, deja el mensaje
  en `failed` (en el chat sale "No entregado") y, si era el mensaje de
  una notificación de guía, la borra de `guide_notifications` y devuelve
  la guía al paso de oficina o nivel de seguimiento anterior. No se
  reintenta en el momento: la próxima corrida del job la vuelve a
  enviar. Excepción: el error 131026 (número sin WhatsApp o que no
  puede recibir) no deshace la notificación, porque reenviarla fallaría
  en cada corrida. Por eso el bot marca cada notificación con el `meta_message_id`
  de su mensaje principal (`mensaje_guia_es` en la inicial; el video
  solo no la deshace). El aviso de Meta llega entre 1 y 20 segundos
  después del envío y puede ganarle a la marca del bot: un bloqueo por
  mensaje en Postgres hace que el mensaje nazca `failed` y la marca no
  avance si el error llegó primero.

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

Cada fila trae `kind` y `notification_count` (tabla `guide_notifications`):

| `kind` | `notification_count` | En el dashboard |
|---|---|---|
| `tracking` | 1 | Pedido enviado (`pedido_enviado_2`) |
| `tracking` | 2 | Pedido en camino (`pedido_camino_1`) |
| `office` | 1 | Oficina inicial |
| `office` | 2 | Recordatorio |
| `office` | 3 | Recordatorio final |
| `office` | 4+ | Seguimiento diario (`pedido_oficina`) |

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
   | Día 1 (0) | `video_producto` (si el producto tiene video y la guía no lo recibió en el seguimiento) + `mensaje_guia_es` |
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

### Seguimiento del envío de pedidos de Dropi (por niveles)

`POST /notify-dropi-tracking?business_id=N` notifica el avance del pedido
**antes** de que llegue a la oficina. Pensado para un job cada 15 minutos.
Mismas reglas que `/notify-dropi-mcp`: responde 202 y procesa en segundo
plano, el `business_id` es obligatorio (404 si no existe), el body
opcional es el mismo (`from` por defecto hace 15 días, `until`,
`result_number`, `start`) y responde 409 mientras haya otra corrida de
seguimiento del mismo business. Tiene su propio control de corridas: no
bloquea a `/notify-dropi-mcp` ni queda bloqueado por él.

```bash
curl -X POST "localhost:8000/notify-dropi-tracking?business_id=1"
```

| Nivel | Estados de Dropi | Plantilla |
|---|---|---|
| 1 | `GUIA_GENERADA`, `PREPARADO PARA TRANSPORTADORA`, `ENTREGADO A TRANSPORTADORA`, `ADMITIDA`, `RECOLECTADO` | `video_producto` (si el producto tiene video) + `pedido_enviado_2` (nombre, guía, producto, precio con el símbolo de su moneda —`$84.900`, `Q229`—, transportadora) |
| 2 | `EN TRANSPORTE`, `EN REPARTO`, `DESPACHADA`, `EN DESPACHO`, `EN RUTA` | `pedido_camino_1` (nombre, producto, guía, dirección y ciudad de entrega) |
| 3 | `RECLAME EN OFICINA` | lo maneja `/notify-dropi-mcp` |

El video del producto va **una sola vez por guía**: con `pedido_enviado_2`
en el nivel 1 o, si la guía no pasó por ese nivel, con el aviso inicial en
oficina. El backend lo sabe por el historial (`video_sent` en
`/guides/lookup`: un video saliente de la guía que no falló; si Meta lo
rechazó, se vuelve a enviar). Un video rechazado no deshace el nivel: este
va ligado a `pedido_enviado_2`. Los dos endpoints comparten la misma
biblioteca de videos (un solo caché de `media_id`).

Los estados viven en `TRACKING_LEVELS`
(`app/services/dropi_tracking_notifier.py`) y se escriben exactamente como
en Dropi: el genérico es `GUIA_GENERADA` (con guion bajo); `GUIA GENERADA`
es un estado de SUPPLI-EXPRESS y no trae pedidos de las demás.

1. `/ordenes` acepta un solo estado por consulta: por cada estado de cada
   nivel se pagina igual que en `/notify-dropi-mcp` (mientras
   `count == result_number`, `start += count`). Un pedido que cambió de
   estado entre dos consultas cuenta en su nivel más alto.
2. Consulta en lote al backend (`POST /guides/lookup`) el nivel ya
   notificado de cada guía (`tracking_level`) y sus datos de pedido.
3. **Una notificación por nivel por guía, y los niveles nunca
   retroceden**: se envía solo si el nivel del pedido es mayor que el ya
   notificado. Un pedido que se ve por primera vez en el nivel 2 recibe
   solo `pedido_camino_1`; uno que vuelve a un estado del nivel 1 no
   recibe nada; uno ya notificado en oficina (`notification_count > 0`,
   por Dropi, PDF o Google Sheet) no recibe ninguna de estas.
4. Tras enviar la plantilla, `POST /guides/{number}/tracking` sube el
   nivel. Si el envío falla el nivel no sube y se reintenta en la próxima
   corrida. No toca `notification_count`: la notificación en oficina
   empieza igual por `mensaje_guia_es`. Cada subida de nivel queda en
   `guide_notifications` (`kind = 'tracking'`) y sale en el dashboard
   como "Pedido enviado" / "Pedido en camino".

El detalle del pedido (`/orden`) se pide una sola vez, la primera vez
que se ve el pedido, y se guarda con la guía: el nivel 2 y la
notificación en oficina (`/notify-dropi-mcp`) ya no lo vuelven a pedir.
El valor a pagar es `0` si la tarifa es `SIN RECAUDO`.

La primera corrida notifica todos los pedidos que ya estén en el nivel 1
o 2 dentro del rango de fechas; para empezar con menos, enviar un `from`
reciente.

Despliegue, en este orden: 1) correr `webapp/backend/db.sql` (agrega
`tracking_level` y `tracking_notified_at` a `guides`, `kind` y
`meta_message_id` a `guide_notifications`, el estado `failed` a
`message_status_enum` y la tabla `messages_errors`), 2) desplegar el
backend y el frontend (el backend anterior no sabe leer mensajes en
`failed`), 3) desplegar el bot. Las tablas `message_quota_reservations` y
`message_quota_reservation_phones` y los índices del cupo también salen de
`db.sql`; el bot nuevo necesita el backend nuevo (sin `/quota` no envía). Si el backend no devuelve `tracking_level`, la
corrida no envía nada (registra el error en los logs) para no repetir
notificaciones.

### Cupo de mensajes (límite de mensajería de Meta)

Meta limita cuántos clientes **únicos** pueden recibir plantillas en una
**ventana móvil de 24 h** (250 sin verificar el negocio; la cuenta es del
número de WhatsApp, así que los business la comparten). Los clientes que
nos escribieron en las últimas 24 h (ventana de atención abierta) no
consumen cupo. Pasarse da errores como 131048 *Spam Rate limit hit*.

Antes de enviar, cada corrida automática del bot (oficina de Dropi,
seguimiento de Dropi y Google Sheet) reserva el cupo de sus clientes en
una sola llamada, en orden de prioridad:

```
POST /quota/reservations  {"phones": ["573001112233", ...], "kind": "office", "business_id": 1, "ttl_minutes": 60}
→ {"approved": false, "approved_phones": [...], "denied_phones": [...], "new_clients": 40,
   "reservation_id": 12, "expires_at": "...", "available": 23, "limit": 230, "tracking_limit": 80,
   "used": 190, "reserved": 17, "tracking_used": 45, "tracking_reserved": 0}
DELETE /quota/reservations/12   (al terminar la corrida)
GET /quota                      (cupo actual, sin reservar)
```

- El backend clasifica cada teléfono: **ya ocupa cupo** (le escribimos en
  las últimas 24 h, mensaje no fallido y fuera de su ventana de atención),
  **ventana abierta** (nos escribió en las últimas 24 h y sigue abierta
  hasta que vence la reserva), **reservado por otra corrida**, o **nuevo**.
  Los tres primeros se aprueban siempre; los nuevos, en el orden pedido
  mientras quepan (la aprobación puede ser parcial).
- **Oficina tiene prioridad**: puede usar todo el cupo (`MESSAGING_LIMIT`,
  230 por defecto). El seguimiento además tiene un tope propio dentro del
  cupo (`MESSAGING_TRACKING_LIMIT`, 80 por defecto): clientes con una
  notificación de seguimiento en las últimas 24 h. Así el seguimiento de la
  mañana nunca se gasta el cupo de la notificación de oficina.
- La reserva guarda los **teléfonos**, no un número: un cliente cuenta una
  sola vez aunque esté en dos corridas a la vez o ya tenga su mensaje
  registrado (desde ahí cuenta por el mensaje). Al terminar la corrida se
  libera; si el bot se cae, vence en `ttl_minutes` (el bot usa 60).
- Atómico: la situación de todos los teléfonos y los totales salen de una
  sola consulta, y calcular y reservar pasan en una transacción con un
  bloqueo global (`pg_advisory_xact_lock`): dos corridas simultáneas nunca
  ven el mismo cupo libre. Si el bloqueo no se obtiene en 10 s, responde
  503 y el bot reintenta (3 intentos).
- En el bot (`RunQuota`, `app/services/quota.py`): cada corrida planea a
  quién le escribe, reserva, envía solo a los aprobados y libera al final.
  Los que no caben quedan en `deferred` del reporte ("sin cupo"): no se
  envían ni se marcan, así que la próxima corrida los reintenta. Sin cupo
  tampoco se consulta el detalle del pedido a Dropi ni se descarga el PDF.
  Prioridad dentro de una corrida de oficina: primero los avisos iniciales,
  después los recordatorios. Si el backend no responde, no se envía.
- El PDF que manda un asociado por WhatsApp no pide cupo: es un envío
  manual a un solo cliente y no tiene una corrida que lo reintente (igual
  cuenta en `used` una vez registrado).

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
| PATCH | `/messages/{meta_message_id}/status` | Actualizar estado (`sent`/`delivered`/`read`; nunca cambia un `failed`) |
| POST | `/messages/{meta_message_id}/errors` | `{"error_code", "error_title", "payload"}`: Meta reportó que el mensaje falló. Lo deja en `failed`, guarda el error y deshace la notificación de guía ligada a él (`undone`). Idempotente |
| POST | `/guides` | Registrar guía; responde `created: false` si es duplicada (no re-notificar). `details` opcional (cliente y productos del pedido) se guarda aunque la guía ya existiera |
| POST | `/guides/lookup` | `{"numbers": [...]}` (máx. 500) → guías existentes con `notification_count`, `details`, `products` y `video_sent` (ya recibió el video del producto) |
| GET | `/quota` | Cupo de clientes nuevos en 24 h: `limit`, `tracking_limit`, `used`, `reserved`, `tracking_used`, `tracking_reserved`, `available_office`, `available_tracking` |
| POST | `/quota/reservations` | `{"phones", "kind": "office"\|"tracking", "business_id"?, "ttl_minutes"?}`: aprueba (y reserva) los teléfonos que caben en el cupo (`approved_phones`/`denied_phones`, en el orden pedido). 503 si el cupo está bloqueado por otra reserva |
| DELETE | `/quota/reservations/{id}` | Liberar la reserva al terminar el envío (`released: false` si ya estaba liberada; 404 si no existe) |
| POST | `/guides/{number}/notified` | Contar una notificación de oficina. Cuerpo opcional `{"meta_message_id"}`: el mensaje principal (`updated: false` si Meta ya lo reportó como fallido) |
| POST | `/guides/{number}/tracking` | `{"level": 1\|2, "meta_message_id"}`: nivel de seguimiento del envío notificado. Solo sube (`updated: false` si ya tenía ese nivel o uno mayor, o si el mensaje ya falló); 404 si la guía no existe |

La sesión usa JWT en `localStorage` (15 min, renovado automáticamente al
enviar mensajes libres). Pendiente: refresh tokens y cookies httpOnly para
una sesión más robusta.
