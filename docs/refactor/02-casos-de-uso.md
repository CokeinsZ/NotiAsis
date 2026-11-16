# 02 · Inventario de casos de uso

Todo lo que el sistema hace hoy, servicio por servicio. Cada fila es una
funcionalidad observable en el código, con el lugar donde vive ahora y el
módulo al que va en la [arquitectura objetivo](03-arquitectura-objetivo.md).

**Cómo leerlo**

- **ID**: estable; el plan de ejecución y los commits de la refactorización
  lo citan (`BOT-064`, `BE-031`…).
- **Hoy**: archivo y símbolo actuales, relativo a la raíz del servicio.
- **Destino**: módulo objetivo, relativo a la raíz del servicio. Un mismo
  módulo agrupa los casos de uso que cambian por la misma razón.

Regla de la refactorización: **ningún caso de uso de esta lista puede
cambiar de comportamiento** mientras se mueve. Los que hoy tienen un
defecto llevan la marca ⚠ y se corrigen en un paso aparte, nunca en el
mismo commit que el movimiento.

- [1. Bot](#1-bot-python--app) · [2. Backend](#2-backend-rust--webappbackend) ·
  [3. Agente de IA](#3-agente-de-ia-rust--ai-agents) · [4. Frontend](#4-frontend-nextjs--webappfrontend)

---

## 1. Bot (Python) — `app/`

### 1.1 Entrada de WhatsApp (webhook de Meta)

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BOT-001 | Verificar la suscripción del webhook (`hub.mode`, `hub.verify_token`, `hub.challenge`) | `api/webhook.py` · `verify_webhook` | `modules/webhook/router.py` + `verification.py` |
| BOT-002 | Recibir un evento, responder 200 de inmediato y procesarlo en segundo plano | `api/webhook.py` · `receive_webhook` | `modules/webhook/router.py` |
| BOT-003 | Recorrer `entry → changes → value` del payload de Meta | `services/webhook_processor.py` · `_iter_values` | `modules/webhook/parser.py` |
| BOT-004 | Extraer el nombre del contacto (`contacts[0].profile.name`) | `webhook_processor.py` · `_extract_contact_name` | `modules/webhook/parser.py` |
| BOT-005 | Reenviar al backend los estados `sent` / `delivered` / `read` de un mensaje | `webhook_processor.py` · `_handle_statuses` | `modules/webhook/handlers/status_update.py` |
| BOT-006 | Registrar una entrega fallida (`failed`) con código, título y el estado completo de Meta | `webhook_processor.py` · `_handle_failure` | `modules/webhook/handlers/delivery_failure.py` |
| BOT-007 | Clasificar al remitente: asociado autorizado o cliente final | `webhook_processor.py` · `_handle_message` | `modules/webhook/dispatcher.py` |
| BOT-008 | Detectar una guía en PDF enviada por un asociado (`document` + mime `pdf` + `media_id`) y lanzar su notificación | `webhook_processor.py` · `_handle_associate_message` | `modules/webhook/handlers/associate_document.py` |
| BOT-009 | Ignorar los mensajes de asociados que no son PDF | ídem | ídem |
| BOT-010 | Registrar el mensaje entrante de un cliente (`text`, `document`, `audio`, `image`, `video`) | `webhook_processor.py` · `_handle_user_message` | `modules/webhook/handlers/customer_message.py` |
| BOT-011 | Tratar el toque de un botón de plantilla como respuesta de texto | `webhook_processor.py` · `BUTTON_TYPE` | `modules/webhook/parser.py` |
| BOT-012 | Extraer contenido según el tipo: cuerpo de texto, o `caption` + `media_id` | `webhook_processor.py` · `_extract_content` | `modules/webhook/parser.py` |
| BOT-013 | Propagar el mensaje citado (`context.id`) para que el backend sepa a qué business y guía responde el cliente | `webhook_processor.py:147` | `modules/webhook/handlers/customer_message.py` |
| BOT-014 | Descartar tipos de mensaje no soportados (sticker, ubicación, reacción…) | `webhook_processor.py` · `SUPPORTED_MEDIA_TYPES` | `modules/webhook/parser.py` |
| BOT-015 | Publicar "mensaje de cliente registrado" a los interesados (Observer), solo si quedó en un chat | `webhook_processor.py:151` | `modules/webhook/events.py` |
| BOT-016 | Avisar a ai-agents de cada mensaje de cliente (`POST /agents/customer-service/events`), sin bloquear ni fallar | `services/ai_agents_client.py` | `integrations/ai_agents/client.py` |

### 1.2 Asociados autorizados

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BOT-020 | Cargar del backend el directorio teléfono → business de los asociados | `services/associate_directory.py` · `load` | `modules/associates/directory.py` |
| BOT-021 | Usar `ALLOWED_SENDER_NUMBERS` como respaldo si el backend no responde (sin business) | ídem | ídem |
| BOT-022 | Decidir si un teléfono es de un asociado autorizado | `associate_directory.py` · `is_authorized` | ídem |
| BOT-023 | Resolver el business de un asociado | `associate_directory.py` · `business_id_for` | ídem |
| BOT-024 | ⚠ Refrescar el directorio sin reiniciar (hoy no existe: los cambios de asociados no llegan al bot hasta reiniciarlo) | — | `modules/associates/directory.py` (TTL) |

### 1.3 Notificación de una guía en PDF (enviada por un asociado)

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BOT-030 | Obtener la URL del media y descargar el PDF desde Meta | `services/shipping_notifier.py:71-77` | `modules/guide_pdf/service.py` (puerto `MediaDownloader`) |
| BOT-031 | Extraer el texto del PDF | `services/pdf_extractor.py` | `modules/guide_pdf/pdf_text.py` |
| BOT-032 | Extraer con un LLM nombre, teléfono, guía, dirección y producto (JSON) | `services/recipient_extractor.py` | `modules/guide_pdf/recipient_extraction.py` + `integrations/deepseek/` |
| BOT-033 | Rechazar la guía si falta algún dato del destinatario | `models/shipping.py` · `is_complete` | `modules/guide_pdf/service.py` |
| BOT-034 | Completar el indicativo del teléfono con el país del business del asociado | `shipping_notifier.py:105-109` | `shared/phones.py` |
| BOT-035 | Decidir el paso según el historial de la guía (inicial → recordatorio → final → nada) | `shipping_notifier.py:111-137` | `modules/escalation/` |
| BOT-036 | Registrar la guía nueva; no notificar si el backend la rechaza o ya existía | `shipping_notifier.py:120` | `modules/guide_pdf/service.py` |
| BOT-037 | Notificar sin registrar cuando el asociado solo existe en el respaldo local | `shipping_notifier.py:116-119` | ídem |
| BOT-038 | Enviar `guia` + `mensaje_guia_es`, o `recordatorio`, o `recordatorio_final` | `shipping_notifier.py:124-137` | `modules/guide_pdf/service.py` + `modules/templates/` |
| BOT-039 | Marcar la guía como notificada, ligada al último mensaje enviado | `shipping_notifier.py:144` | `modules/guide_pdf/service.py` |
| BOT-03A | ⚠ El prompt fuerza el indicativo `+57`: una guía de un business de otro país queda con el teléfono mal | `recipient_extractor.py:17` | `modules/guide_pdf/recipient_extraction.py` |

### 1.4 Entrega de plantillas (común a los cuatro flujos)

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BOT-040 | Enviar una lista de plantillas, en orden, a un destinatario | copiado en `shipping_notifier.py:147`, `sheet_notifier.py:181`, `dropi_orders.py:311` | `modules/delivery/dispatcher.py` |
| BOT-041 | Desviar el envío físico a un número de pruebas (`NOTIFICATION_OVERRIDE_NUMBER`) | ídem | `modules/delivery/routing.py` |
| BOT-042 | Enviar además una copia de depuración (`DEBUG_NOTIFICATION_NUMBER`) | ídem | ídem |
| BOT-043 | Registrar cada plantilla enviada en el historial del chat, con su texto renderizado, su media y su guía, siempre con el teléfono real | copiado en los tres | `modules/delivery/dispatcher.py` |

### 1.5 Política de escalación

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BOT-050 | Escalación con tope: 0 → inicial, 1 → recordatorio, 2 → final, ≥3 → nada | `services/notification_policy.py` · `step_for_notification_count` | `modules/escalation/policy.py` |
| BOT-051 | Escalación diaria sin tope: …, ≥3 → recordatorio diario | `notification_policy.py` · `daily_step` | ídem |
| BOT-052 | Regla dura: una guía nunca recibe más de una notificación por día | ambas funciones | ídem (una sola vez) |
| BOT-053 | Interpretar la fecha de última notificación que devuelve el backend | `notification_policy.py` · `_as_date` | ídem |
| BOT-054 | ⚠ El "día" es el del contenedor (UTC), no el del business | `date.today()` | `shared/clock.py` |

### 1.6 Notificación masiva desde Google Sheet

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BOT-060 | Lanzar la corrida de un business (`POST /notify-sheet`), responder 202 | `api/notify_sheet.py` | `modules/sheet_notifications/router.py` |
| BOT-061 | Leer el país del business; sin él no se envía nada | `services/sheet_notifier.py:77` | `modules/sheet_notifications/service.py` |
| BOT-062 | Leer y validar la configuración del sheet (`document_id`, `office_id`, `delivered_id`) | `sheet_notifier.py:82-88` | ídem |
| BOT-063 | Descargar las hojas `office` y `delivered` como CSV (guía y teléfono como texto) | `services/google_sheets.py` · `fetch_sheet` | `integrations/google_sheets/client.py` |
| BOT-064 | Calcular las guías pendientes: en `office` y no en `delivered`, sin filas vacías, sin guías repetidas | `sheet_notifier.py` · `_pending_guides` | `modules/sheet_notifications/pending_guides.py` |
| BOT-065 | Convertir una fila en destinatario (NOMBRE, TELEFONO, DIRECCION + CIUDAD + DEPARTAMENTO, PRODUCTO, numero_guia) | `sheet_notifier.py` · `_build_recipient` | `modules/sheet_notifications/row_mapper.py` |
| BOT-066 | Planear el paso de cada guía; omitir las incompletas y las que no tocan hoy | `sheet_notifier.py` · `_plan_row` | `modules/sheet_notifications/planner.py` |
| BOT-067 | Poner primero los avisos iniciales si el cupo no alcanza | `sheet_notifier.py:104` | ídem |
| BOT-068 | Reservar cupo `office` y dejar para la próxima corrida a quien no quepa (sin descargar su PDF) | `sheet_notifier.py:106-111, :168` | `modules/quota/` |
| BOT-069 | Aviso inicial: descargar el PDF de la columna `Guia pdf`, subirlo a Meta, registrar la guía, enviar `guia` + `mensaje_guia_es`, marcar | `sheet_notifier.py` · `_send_new_notification` | `modules/sheet_notifications/service.py` |
| BOT-070 | Recordatorio y recordatorio final | `sheet_notifier.py` · `_send_reminder` | ídem |
| BOT-071 | Informe de la corrida: nuevas, recordatorios, omitidas, sin cupo, errores | `SheetNotificationReport` | `modules/sheet_notifications/report.py` |
| BOT-072 | ⚠ Sin control de corridas simultáneas ni verificación del business | `api/notify_sheet.py` | `shared/run_guard.py` |

### 1.7 Pedidos de Dropi — común a oficina y seguimiento

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BOT-080 | Validar los filtros de la corrida (`from` ≤ `until`, `result_number` ≥ 1, `start` ≥ 0; por defecto 15 días) | `api/notify_dropi.py` · `NotifyDropiRequest` | `modules/dropi_orders/schemas.py` |
| BOT-081 | Verificar que el business existe (404) y que el backend responde (503) | `notify_dropi.py` · `_start_run` | `modules/dropi_orders/run_launcher.py` |
| BOT-082 | Una corrida por business a la vez (409); liberar al terminar aunque falle | `services/run_guard.py`, `notify_dropi.py` · `_run` | `shared/run_guard.py` |
| BOT-083 | Paginar `/ordenes` por estado: seguir mientras la página llegue llena, tope de 100 páginas, parar si una página solo repite pedidos, procesar lo obtenido si una página falla | `services/dropi_orders.py` · `_fetch_orders` | `modules/dropi_orders/order_pager.py` |
| BOT-084 | Consultar el detalle de un pedido y llevarlo al formato del backend (cliente, productos, total, moneda), recortando a los largos que acepta | `dropi_orders.py` · `_fetch_details` | `modules/dropi_orders/order_details.py` |
| BOT-085 | No consultar a Dropi si la guía ya tiene guardados productos y dirección | `dropi_orders.py` · `_stored_details`, `_has_order_data` | ídem |
| BOT-086 | Resolver el teléfono del cliente (chat existente > `/ordenes` > `/orden`) y completar su indicativo | `dropi_orders.py` · `_phone` | `modules/dropi_orders/customer_phone.py` |
| BOT-087 | Componer la dirección de entrega: dirección + ciudad (+ transportadora), sin repetir lo que ya menciona | `dropi_orders.py` · `_address_with_city`, `_full_address` | `modules/dropi_orders/address.py` |
| BOT-088 | Calcular el valor a pagar (`SIN RECAUDO` = 0; total actual o, si falta, el guardado) | `dropi_orders.py` · `_total_to_pay` | `modules/dropi_orders/payment.py` |
| BOT-089 | Dar formato al dinero por moneda (`$84.900`, `Q1,250.50`), con la moneda del país si el pedido no la trae | `dropi_orders.py` · `_format_money`, `_price_to_pay`, `_amount_to_pay` | `shared/money.py` |
| BOT-090 | Dejar todo texto en una sola línea (Meta rechaza saltos de línea y tabs en parámetros) | `dropi_orders.py` · `_clean` | `shared/text.py` |
| BOT-091 | Omitir las guías que ya pertenecen a otro business | `dropi_notifier.py:162`, `dropi_tracking_notifier.py:187` | `modules/dropi_orders/ownership.py` |
| BOT-092 | Cliente de N8N: header `api-key`, reintentos con espera exponencial ante 429/5xx/red, rechazar pedidos de otra cuenta | `services/dropi_client.py` | `integrations/dropi/client.py` |

### 1.8 Dropi — pedidos en "reclame en oficina"

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BOT-100 | Lanzar la corrida (`POST /notify-dropi-mcp?business_id=`) | `api/notify_dropi.py` · `notify_dropi_mcp` | `modules/dropi_office/router.py` |
| BOT-101 | Traer los pedidos en `RECLAME EN OFICINA`; descartar los que no tienen guía, los repetidos y los de otro estado | `services/dropi_notifier.py` · `_notifiable_orders` | `modules/dropi_office/service.py` |
| BOT-102 | Consultar en lote las guías al backend; sin respuesta no se envía nada | `dropi_notifier.py:104-108` | ídem |
| BOT-103 | Planear cada guía con la escalación diaria y ordenar por prioridad de paso | `dropi_notifier.py` · `_plan`, `STEP_PRIORITY` | `modules/dropi_office/planner.py` |
| BOT-104 | Reservar cupo `office`; lo que no quepa queda para la próxima corrida | `dropi_notifier.py:111` | `modules/quota/` |
| BOT-105 | Día 1: `video_producto` (si aplica) + `mensaje_guia_es` con la dirección completa | `dropi_notifier.py` · `_templates` | `modules/dropi_office/templates_for_step.py` |
| BOT-106 | Día 2: `recordatorio`. Día 3: `recordatorio_final` | ídem | ídem |
| BOT-107 | Día 4 en adelante: `pedido_oficina` con dirección completa y valor a pagar | ídem | ídem |
| BOT-108 | Guardar los datos del pedido con la guía la primera vez que se consultan | `dropi_notifier.py:239-247` | `modules/dropi_office/service.py` |
| BOT-109 | Contar la notificación solo si salió la plantilla principal del día | `dropi_notifier.py:261-269` | ídem |
| BOT-110 | Informe: nuevas, con video, sin video, recordatorios, omitidas, sin cupo, errores, consultas, páginas | `DropiNotificationReport` | `modules/dropi_office/report.py` |

### 1.9 Dropi — seguimiento del envío por niveles

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BOT-120 | Lanzar la corrida (`POST /notify-dropi-tracking?business_id=`), con control de corridas propio | `api/notify_dropi.py` · `notify_dropi_tracking` | `modules/dropi_tracking/router.py` |
| BOT-121 | Mapa nivel → estados de Dropi (nivel 1: listo para envío; nivel 2: en camino) | `services/dropi_tracking_notifier.py` · `TRACKING_LEVELS` | `modules/dropi_tracking/levels.py` |
| BOT-122 | Consultar cada estado por separado; si un pedido aparece dos veces gana el nivel más alto | `dropi_tracking_notifier.py` · `_orders_by_level` | `modules/dropi_tracking/service.py` |
| BOT-123 | Nivel ya notificado de una guía (notificada en oficina = nivel 3) y regla "nunca retrocede" | `dropi_tracking_notifier.py` · `_notified_level`, `_needs_notification` | `modules/dropi_tracking/levels.py` |
| BOT-124 | No enviar nada si el backend no devuelve `tracking_level` | `dropi_tracking_notifier.py:123` | `modules/dropi_tracking/service.py` |
| BOT-125 | Reservar cupo `tracking` (tope propio) | `dropi_tracking_notifier.py:133` | `modules/quota/` |
| BOT-126 | Nivel 1: `video_producto` (si aplica) + `pedido_enviado_2` (precio con símbolo de moneda y transportadora) | `dropi_tracking_notifier.py` · `_template` | `modules/dropi_tracking/templates_for_level.py` |
| BOT-127 | Nivel 2: `pedido_camino_1` (dirección y ciudad) | ídem | ídem |
| BOT-128 | Subir el nivel en el backend ligado al mensaje; no subirlo si el envío falló | `dropi_tracking_notifier.py:287-296` | `modules/dropi_tracking/service.py` |
| BOT-129 | Informe por nivel, ya notificados, videos, omitidos, sin cupo, errores | `DropiTrackingReport` | `modules/dropi_tracking/report.py` |

### 1.10 Videos de producto

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BOT-140 | Encontrar el video por id de producto (`{product_id}.mp4`) | `services/product_videos.py` · `find_video` | `modules/product_videos/matcher.py` |
| BOT-141 | Encontrar el video por nombre: todas las palabras del archivo en el producto, sin conectores, parcial desde 4 letras, gana el más específico | `product_videos.py` · `_words`, `_word_in` | ídem |
| BOT-142 | Usar el video del primer producto del pedido que tenga uno | `find_video` | ídem |
| BOT-143 | Rechazar videos de más de 16 MB o inaccesibles | `product_videos.py` · `_media_id` | `modules/product_videos/library.py` |
| BOT-144 | Subir cada video una sola vez y reutilizar su `media_id` (por número de WhatsApp y archivo real; caduca a los 28 días o si el archivo cambia) | ídem | `modules/product_videos/media_cache.py` |
| BOT-145 | Persistir el caché en disco con escritura atómica; seguir en memoria si el disco falla | `product_videos.py` · `_load`, `_save` | ídem |
| BOT-146 | Olvidar un `media_id` que Meta rechazó | `product_videos.py` · `discard` | ídem |
| BOT-147 | Enviar el video una sola vez por guía (según `video_sent` del backend) | `dropi_orders.py` · `_product_video`, `_video_was_sent` | `modules/product_videos/policy.py` |

### 1.11 Cupo de mensajería (lado del bot)

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BOT-150 | Reservar en una sola llamada el cupo de la corrida, con teléfonos normalizados, sin repetir y en orden de prioridad | `services/quota.py` · `RunQuota.reserve` | `modules/quota/run_quota.py` |
| BOT-151 | Preguntar si un teléfono tiene cupo; pedirlo en el momento si no estaba en el plan | `RunQuota.allows` | ídem |
| BOT-152 | Liberar las reservas al terminar, aunque la corrida falle | `RunQuota.__exit__` | ídem |
| BOT-153 | No enviar nada si el backend no responde | `RunQuota.unavailable` | ídem |
| BOT-154 | Reintentar la reserva si el cupo está bloqueado por otra corrida (503, 3 intentos) | `services/backend_client.py` · `reserve_quota` | `integrations/backend/quota.py` |

### 1.12 Catálogo de plantillas

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BOT-160 | Contrato de plantilla: nombre, idioma, componentes, payload de Meta y metadatos para el historial | `whatsapp/templates/base.py` | `modules/templates/base.py` |
| BOT-161 | `guia`: PDF de la guía como documento | `templates/guia.py` | `modules/templates/guia.py` |
| BOT-162 | `mensaje_guia_es`: nombre, guía, producto, dirección | `templates/mensaje_guia.py` | `modules/templates/mensaje_guia.py` |
| BOT-163 | `recordatorio`: sin variables | `templates/recordatorio.py` | `modules/templates/recordatorio.py` |
| BOT-164 | `recordatorio_final`: nombre, producto | `templates/recordatorio_final.py` | `modules/templates/recordatorio_final.py` |
| BOT-165 | `pedido_oficina`: nombre, producto, guía, dirección, valor | `templates/pedido_oficina.py` | `modules/templates/pedido_oficina.py` |
| BOT-166 | `pedido_enviado_2`: nombre, guía, producto, precio, transportadora | `templates/pedido_enviado.py` | `modules/templates/pedido_enviado.py` |
| BOT-167 | `pedido_camino_1`: nombre, producto, guía, dirección | `templates/pedido_camino.py` | `modules/templates/pedido_camino.py` |
| BOT-168 | `video_producto`: video en el encabezado | `templates/video_producto.py` | `modules/templates/video_producto.py` |
| BOT-169 | Texto renderizado de cada plantilla para el historial del chat | `log_message()` de cada una | cada plantilla |

### 1.13 Integraciones (clientes de sistemas externos)

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BOT-170 | Backend: obtener JWT con la api-key y renovarlo ante un 401 | `services/backend_client.py` · `_login`, `_request` | `integrations/backend/session.py` |
| BOT-171 | Backend: listar asociados autorizados (`GET /associates`) | `fetch_authorized_associates` | `integrations/backend/associates.py` |
| BOT-172 | Backend: registrar guía con sus datos de pedido (`POST /guides`) | `register_guide` | `integrations/backend/guides.py` |
| BOT-173 | Backend: consultar una guía (`GET /guides/{n}`) y varias en lotes de 500 (`POST /guides/lookup`) | `get_guide`, `lookup_guides` | ídem |
| BOT-174 | Backend: marcar guía notificada (`POST /guides/{n}/notified`) y nivel de seguimiento (`POST /guides/{n}/tracking`) | `mark_guide_notified`, `mark_guide_tracking` | ídem |
| BOT-175 | Backend: existencia y país del business (`GET /businesses/{id}`), configuración del sheet (`GET /businesses/{id}/sheet`) | `business_exists`, `get_business_country`, `get_business_sheet` | `integrations/backend/businesses.py` |
| BOT-176 | Backend: registrar mensaje entrante y saliente (`POST /messages/incoming`, `/messages/outgoing`) | `register_incoming_message`, `register_outgoing_message` | `integrations/backend/messages.py` |
| BOT-177 | Backend: actualizar estado y registrar error de un mensaje (`PATCH /messages/{id}/status`, `POST /messages/{id}/errors`) | `update_message_status`, `register_message_error` | ídem |
| BOT-178 | Backend: reservar y liberar cupo (`POST`/`DELETE /quota/reservations`) | `reserve_quota`, `release_quota` | `integrations/backend/quota.py` |
| BOT-180 | Meta: URL de descarga de un media y descarga de su contenido | `whatsapp/client.py` · `get_media_url`, `download_media` | `integrations/whatsapp/media.py` → backend (fase 6) |
| BOT-181 | Meta: subir un archivo (PDF, video) y obtener su `media_id` | `whatsapp/client.py` · `upload_media` | ídem |
| BOT-182 | Meta: enviar una plantilla y obtener el id del mensaje | `whatsapp/client.py` · `send_template` | `integrations/whatsapp/sender.py` → backend (fase 6) |
| BOT-183 | Google Sheets: exportar una hoja como CSV | `services/google_sheets.py` | `integrations/google_sheets/client.py` |
| BOT-184 | Descargar un archivo por URL (el PDF de la guía en la hoja) | `google_sheets.py` · `download_file` | `integrations/file_download/` |
| BOT-185 | DeepSeek: completar un chat con salida JSON | `services/recipient_extractor.py` | `integrations/deepseek/client.py` |

### 1.14 Arranque y configuración

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BOT-190 | Leer la configuración del entorno | `config.py` | `bootstrap/settings.py` |
| BOT-191 | Construir y cablear las dependencias | `api/dependencies.py` | `bootstrap/container.py` + `module.py` de cada módulo |
| BOT-192 | Crear la aplicación y montar los routers | `main.py` | `main.py` |

---

## 2. Backend (Rust) — `webapp/backend/`

Rutas de **Hoy** relativas a `src/`; las de **Destino**, a `src/modules/`
salvo que empiecen por `shared/`, `integrations/` o `bootstrap/`.

### 2.1 Identidad y acceso

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BE-001 | Login de un asociado (usuario y contraseña, business activo) → JWT de 15 min con su business y teléfono | `auth/service.rs` · `login` | `auth/use_cases/login_associate.rs` |
| BE-002 | Login de un servicio con api-key → JWT de 24 h | `auth/service.rs` · `login_with_api_key` | `auth/use_cases/login_service.rs` |
| BE-003 | Validar el JWT de cada petición y dejar sus claims disponibles | `auth/middleware.rs` · `require_auth` | `auth/middleware.rs` |
| BE-004 | Emitir, validar y renovar tokens | `auth/service.rs` · `issue_token`, `validate_token`, `renew_token` | `auth/tokens.rs` |
| BE-005 | Autorizar el acceso a los recursos de un business (el asociado, solo el suyo) | `auth/middleware.rs` · `authorize_business`, `auth/dtos.rs` · `can_access_business` | `auth/extractors.rs` · `BusinessAccess` |
| BE-006 | Reservar un endpoint a los servicios internos | `auth/middleware.rs` · `require_bot` | `auth/extractors.rs` · `ServiceCaller` |
| BE-007 | Renovar la sesión del asociado mientras conversa (token nuevo en cada envío) | `chats/controller.rs:88, :252` | `auth/session.rs` (capa de respuesta, fuera de `messages`) |
| BE-008 | Cambiar la propia contraseña (pide la actual; solo el dueño de la cuenta) | `auth/service.rs` · `change_password` | `associates/use_cases/change_password.rs` |
| BE-009 | Política de contraseñas | `tools/custom_validators.rs` · `validate_password` | `auth/password_policy.rs` |

### 2.2 Empresas, países y asociados

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BE-020 | Crear un business (valida que el país exista) | `businesses/service.rs` · `create_business` | `businesses/use_cases/create.rs` |
| BE-021 | Listar businesses no eliminados | `businesses/service.rs` · `get_businesses` | `businesses/use_cases/list.rs` |
| BE-022 | Detalle de un business con su país (indicativo, largo del número, código) | `businesses/service.rs` · `get_business` | `businesses/use_cases/get.rs` |
| BE-023 | Países y sus reglas de teléfono y zona horaria | `businesses/repository.rs` · `get_country_by_id`, `db.sql` | `businesses/countries.rs` |
| BE-024 | Configuración del Google Sheet de un business | `businesses/service.rs` · `get_business_sheet` | `sheet_configs/` |
| BE-030 | Crear un asociado: teléfono al formato del país, usuario y teléfono únicos, contraseña con bcrypt | `businesses/service.rs` · `create_associate` | `associates/use_cases/create.rs` |
| BE-031 | Listar los asociados de un business | `businesses/service.rs` · `get_associates` | `associates/use_cases/list.rs` |
| BE-032 | Listar los asociados de todos los business activos (lo usa el bot) | `businesses/service.rs` · `get_all_associates` | ídem |
| BE-033 | Listar solo sus teléfonos (`/associates/phones`, sin consumidor) | `businesses/service.rs` · `get_associate_phones` | eliminar en la fase 10 |

### 2.3 Clientes finales

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BE-040 | Crear el cliente si no existe y actualizar su nombre si llega uno no vacío | `users/repository.rs` · `upsert_user` | `customers/use_cases/upsert.rs` |
| BE-041 | Listar y consultar clientes (sin consumidor) | `users/service.rs` | `customers/` (decidir en la fase 10) |

### 2.4 Bandeja de chats

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BE-050 | Bandeja de un business: importantes, después respuesta más reciente, después guía notificada más reciente | `chats/repository.rs` · `get_chats_by_business` | `chats/use_cases/inbox.rs` |
| BE-051 | Indicar en cada chat si la ventana de 24 h está abierta | `chats/service.rs` · `get_chats` | ídem (regla en `messages/window.rs`, vía puerto) |
| BE-052 | Marcar o desmarcar un chat como importante | `chats/service.rs` · `set_chat_importance` | `chats/use_cases/set_importance.rs` |
| BE-053 | Crear el chat si no existe | `chats/repository.rs` · `upsert_chat` | `chats/use_cases/open_chat.rs` |
| BE-054 | Guardar el último mensaje del cliente con su vista previa (`[audio]`, `[imagen] texto`…) | `chats/repository.rs` · `update_last_user_message`, `messages/service.rs` · `message_preview` | `chats/use_cases/record_customer_activity.rs` |
| BE-055 | Elegir el business de un mensaje que no cita ninguno: el chat con actividad más reciente | `chats/repository.rs` · `find_latest_chat_business` | `chats/use_cases/route_customer.rs` |
| BE-056 | ⚠ Reflejar en la bandeja la última guía notificada (hoy en los chats de todos los business del cliente) | `chats/repository.rs` · `touch_guide_notification` | `chats/use_cases/record_guide_notification.rs` |

### 2.5 Mensajes

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BE-060 | Historial de un chat | `messages/service.rs` · `get_chat_messages` | `messages/use_cases/get_history.rs` |
| BE-061 | Registrar un mensaje entrante: cliente, business (explícito > citado > chat reciente), guía y mensaje citados, hora de Meta | `messages/service.rs` · `register_incoming` | `messages/use_cases/register_incoming.rs` |
| BE-062 | Ignorar la cita de un mensaje que no es del mismo cliente | `messages/service.rs:512-518` | ídem |
| BE-063 | Registrar una plantilla enviada por el bot (crea cliente y chat; liga la guía) | `messages/service.rs` · `register_outgoing` | `messages/use_cases/register_outgoing.rs` |
| BE-064 | Idempotencia por id de Meta: un webhook repetido devuelve el mensaje ya guardado | `messages/repository.rs` · `save_message` | `messages/repository.rs` |
| BE-065 | Un saliente cuyo fallo ya llegó nace `failed` | `messages/repository.rs:107-112` | ídem |
| BE-066 | Enviar un texto libre de un asesor dentro de la ventana de 24 h | `messages/service.rs` · `send_free_message` | `messages/use_cases/send_free_text.rs` |
| BE-067 | Enviar un texto libre del agente de IA (queda como `ai`, no cuenta como asesor) | `messages/service.rs` · `send_ai_message` | ídem (el remitente es un parámetro) |
| BE-068 | Enviar multimedia libre: validar tipo, tamaño y pie de foto; subir a Meta; enviar; guardar | `messages/service.rs` · `send_free_media_message` | `messages/use_cases/send_free_media.rs` |
| BE-069 | Leer el formulario multipart del envío (archivo, tipo, pie, voz) y dar nombre por defecto | `chats/controller.rs:147-306` | `messages/dtos.rs` · extractor `MediaUpload` |
| BE-070 | Convertir un audio grabado a nota de voz (OGG/Opus mono, 120 s máximo) | `messages/service.rs` · `FfmpegVoiceTranscoder` | `integrations/ffmpeg/` |
| BE-071 | Reglas de multimedia: tipos y tamaños por medio, pie de 1 024 caracteres | `messages/service.rs` · `validate_media_upload`, `validate_media_caption` | `messages/media_rules.rs` |
| BE-072 | Regla de la ventana de 24 h | `messages/service.rs` · `is_window_open_at` | `messages/window.rs` |
| BE-073 | Actualizar el estado `sent` / `delivered` / `read` (nunca cambia un `read` ni un `failed`) | `messages/service.rs` · `update_status` | `messages/use_cases/update_status.rs` |
| BE-074 | ⚠ El estado no debe retroceder de `delivered` a `sent` | `messages/repository.rs:172` | ídem |
| BE-075 | Registrar un fallo de entrega: guardar el error, dejar el mensaje en `failed`, avisar para deshacer la notificación | `messages/service.rs` · `register_failure`, `messages/repository.rs:185-261` | `messages/use_cases/register_failure.rs` + `notifications/undo/` |
| BE-076 | Túnel de multimedia: descargar de Meta y entregar al navegador; un asociado solo la de su business | `messages/controller.rs` · `get_media` | `media/use_cases/download.rs` |
| BE-077 | Cliente de Meta: enviar texto, subir archivo, enviar multimedia, descargar | `messages/meta_client.rs` | `integrations/whatsapp/` |

### 2.6 Guías

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BE-080 | Registrar una guía; `created: false` si ya existía | `guides/service.rs` · `register_guide` | `guides/use_cases/register.rs` |
| BE-081 | Guardar los datos del pedido aunque la guía ya existiera (los vacíos conservan lo anterior; los productos se reemplazan) | `guides/repository.rs` · `save_details` | `guides/use_cases/save_order_details.rs` |
| BE-082 | Consultar una guía | `guides/service.rs` · `get_guide` | `guides/use_cases/get.rs` |
| BE-083 | Consultar hasta 500 guías con pedido, productos y si ya recibieron el video | `guides/service.rs` · `lookup_guides` | `guides/use_cases/lookup.rs` |
| BE-084 | Saber si una guía ya recibió el video del producto | `guides/repository.rs:214-220` | puerto `VideoHistory` implementado por `messages` |
| BE-085 | Listar guías por teléfono (sin consumidor) | `guides/service.rs` · `get_guides` | decidir en la fase 10 |

### 2.7 Libro de notificaciones de guías

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BE-090 | Contar una notificación de oficina y dejarla en el libro, ligada a su mensaje | `guides/repository.rs` · `mark_notified` | `notifications/use_cases/record_office.rs` |
| BE-091 | Subir el nivel de seguimiento (solo sube) y dejarlo en el libro | `guides/repository.rs` · `raise_tracking_level` | `notifications/use_cases/record_tracking.rs` |
| BE-092 | No contar una notificación cuyo mensaje ya falló | `guides/repository.rs` · `message_can_notify` | `notifications/message_guard.rs` |
| BE-093 | Deshacer una notificación cuando su mensaje falla: borrarla del libro y devolver la guía al paso o nivel anterior | `messages/repository.rs:235-257` | `notifications/use_cases/undo.rs` |
| BE-094 | No deshacer con errores permanentes (número sin WhatsApp) | `tools/message_lock.rs` · `PERMANENT_ERROR_CODES` | `notifications/failure_policy.rs` |
| BE-095 | Ordenar por mensaje el registro, la marca y el fallo (bloqueo en transacción) | `tools/message_lock.rs` · `lock_message` | `notifications/message_guard.rs` |
| BE-096 | Estadísticas por día y tipo para el dashboard (últimos N días o rango) | `guides/repository.rs` · `get_notification_stats` | `notifications/use_cases/daily_stats.rs` |
| BE-097 | ⚠ Las estadísticas agrupan por día UTC, no por el del business | `guides/repository.rs:343` | ídem |

### 2.8 Cupo de mensajería

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BE-100 | Consultar el cupo: límite, usado, reservado, disponible por tipo | `quota/service.rs` · `status` | `quota/use_cases/status.rs` |
| BE-101 | Clasificar cada teléfono: ventana abierta, ya ocupa cupo, reservado por otra corrida, nuevo | `quota/repository.rs` · `PHONES` | `quota/read_model.rs` |
| BE-102 | Asignar en el orden pedido: oficina usa todo el cupo, seguimiento hasta su tope | `quota/dtos.rs` · `allocate` | `quota/allocation.rs` |
| BE-103 | Reservar los aprobados por un tiempo, en una transacción con bloqueo global (503 si no se obtiene en 10 s) | `quota/repository.rs` · `reserve` | `quota/use_cases/reserve.rs` |
| BE-104 | Liberar una reserva (repetible; 404 si no existe) | `quota/repository.rs` · `release` | `quota/use_cases/release.rs` |
| BE-105 | Límites configurables por entorno | `main.rs:164-167` | `bootstrap/config.rs` |

### 2.9 Arranque y transversal

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| BE-110 | Leer la configuración y fallar si falta algo | `main.rs` | `bootstrap/config.rs` |
| BE-111 | Construir repositorios, servicios y rutas | `main.rs` · `build_app`, `state.rs` | `bootstrap/app.rs` + `mod.rs` de cada módulo |
| BE-112 | Respuesta JSON y errores de validación uniformes | `tools/responses.rs` | `shared/response.rs`, `shared/error.rs` |
| BE-113 | Normalizar y validar teléfonos | `tools/phones.rs` | `shared/phone.rs` |
| BE-114 | Esquema de la base de datos | `db.sql` | `migrations/` |

---

## 3. Agente de IA (Rust) — `ai-agents/`

Su estructura ya es la objetivo: la columna **Destino** solo aparece cuando
algo cambia de sitio o de dueño. Rutas relativas a `src/`; `cs/` abrevia
`agents/customer_service/`.

### 3.1 Turno de atención

| ID | Caso de uso | Dónde | Cambio |
|---|---|---|---|
| AG-001 | Recibir el aviso de un mensaje (`POST /agents/customer-service/events`), validar y responder 202 | `cs/controller.rs` | — |
| AG-002 | Correr el turno en segundo plano con su traza | `cs/dispatcher.rs` | — |
| AG-003 | Esperar por si el cliente sigue escribiendo | `cs/steps/wait_for_silence.rs` | — |
| AG-004 | Un turno a la vez por chat | `cs/steps/lock_chat.rs`, `shared/keyed_lock.rs` | — |
| AG-005 | Decidir si responde, antes de llamar al modelo | `cs/steps/check_eligibility.rs`, `cs/eligibility.rs` | — |
| AG-006 | Limitar los turnos simultáneos con el modelo | `cs/steps/reserve_capacity.rs` | — |
| AG-007 | Cargar el contexto: business, país, zona horaria, persona, políticas, guías, fichas de producto, casos anteriores | `cs/steps/load_customer_context.rs`, `cs/repositories/customer_context.rs` | lee por vistas de contrato (fase 7) |
| AG-008 | Transcribir las notas de voz pendientes y guardar el texto | `cs/steps/transcribe_voice_notes.rs` | guarda por la API del backend (fase 7) |
| AG-009 | Cargar el historial con autores, citas y hora local | `cs/steps/load_transcript.rs`, `cs/repositories/transcript.rs` | lee por vistas de contrato |
| AG-010 | Armar el prompt desde `system.md` y `user.md` | `cs/prompts/`, `cs/presentation/` | — |
| AG-011 | Bucle modelo ↔ herramientas con tope de rondas | `agent/runner.rs` | — |
| AG-012 | Interpretar la salida: silencio, comillas, tope de 4 096 caracteres | `cs/reply.rs` | — |
| AG-013 | Volver a evaluar las reglas justo antes de enviar | `cs/steps/deliver_reply.rs` | — |
| AG-014 | Entregar por el backend, o solo registrar en modo borrador | `cs/delivery.rs` | — |

### 3.2 Reglas de elegibilidad (una por archivo en `cs/policy/rules/`)

| ID | Regla | Se calla si… |
|---|---|---|
| AG-020 | `agent_enabled` | el business no tiene el agente activado |
| AG-021 | `allowlist` | hay lista de teléfonos de prueba y el cliente no está |
| AG-022 | `latest_message` | el cliente escribió algo después |
| AG-023 | `service_window` | pasaron 24 h menos 5 min desde su último mensaje |
| AG-024 | `not_yet_answered` | ya respondió ese mensaje |
| AG-025 | `human_takeover` | un asesor escribió hace poco |
| AG-026 | `open_escalation` | escaló el chat y ningún asesor ha respondido |
| AG-027 | `rate_limit` | ya envió el máximo por hora en ese chat |

### 3.3 Herramientas del modelo (una por archivo en `cs/tools/`)

| ID | Herramienta | Qué hace | Cambio |
|---|---|---|---|
| AG-030 | `detalle_guia` | Productos, dirección, valor y avisos de una guía del cliente | — |
| AG-031 | `estado_envio_en_vivo` | Estado actual en Dropi; el id del pedido sale de la base, no del modelo | — |
| AG-032 | `info_producto` | Ficha del catálogo buscada por nombre | — |
| AG-033 | `registrar_novedad` | Guarda "recojo tal día", "ya lo recogí" o "no lo quiero" | tabla propia del agente (fase 7) |
| AG-034 | `escalar_a_humano` | Registra el escalamiento y marca el chat como importante | marca por la API del backend (fase 7) |

### 3.4 Integraciones y base

| ID | Caso de uso | Dónde | Cambio |
|---|---|---|---|
| AG-040 | Token del backend por api-key, en caché, renovado antes de vencer o ante un 401 | `integrations/backend/token.rs` | — |
| AG-041 | Descargar multimedia y enviar el mensaje de IA por el backend | `integrations/backend/client.rs` | — |
| AG-042 | Estado de un pedido en N8N / Dropi | `integrations/dropi/client.rs` | mismo contrato que el bot (`contracts/`) |
| AG-043 | Chat con DeepSeek (thinking, razonamiento devuelto en cada ronda) | `llm/deepseek/` | — |
| AG-044 | Transcripción con Gemini | `transcription/gemini/` | — |
| AG-045 | Reintentos, HTTP JSON, plantillas, locks por clave, texto | `shared/` | — |
| AG-046 | Configuración tipada por secciones | `config/` | — |
| AG-047 | `/health` y middleware de api-key | `http/` | — |

---

## 4. Frontend (Next.js) — `webapp/frontend/`

Rutas relativas a `src/`.

| ID | Caso de uso | Hoy | Destino |
|---|---|---|---|
| FE-001 | Iniciar sesión y entrar al panel del business | `app/login/page.tsx`, `lib/api.ts` · `login` | `features/auth/` |
| FE-002 | Guardar, leer y validar la sesión | `lib/auth.ts` | `features/auth/session.ts` |
| FE-003 | Proteger una página: sin sesión → login; sesión de otro business → su panel | `ChatApp.tsx:39`, `DashboardApp.tsx:121` | `features/auth/useSessionGuard.ts` |
| FE-004 | Cerrar sesión | `ChatApp.tsx` · `handleLogout` | `features/auth/` |
| FE-005 | Cliente HTTP: token, 401 → login, errores | `lib/api.ts` (4 copias) | `shared/http.ts` |
| FE-006 | Proxy hacia el backend | `app/api/backend/[...path]/route.ts` | ídem |
| FE-010 | Bandeja con refresco periódico | `ChatApp.tsx` | `features/inbox/useInbox.ts` |
| FE-011 | Buscar por nombre o teléfono | `ChatApp.tsx:83` | `features/inbox/filters.ts` |
| FE-012 | Filtrar: importantes, respuestas recientes, notificaciones recientes | ídem | ídem |
| FE-013 | Marcar un chat como importante | `ChatListItem.tsx`, `ChatApp.tsx:124` | `features/inbox/` |
| FE-014 | Seleccionar un chat (el primero visible por defecto) | `ChatApp.tsx:115` | `features/inbox/` |
| FE-020 | Historial con refresco periódico y scroll al final | `ChatWindow.tsx` | `features/conversation/useMessages.ts` |
| FE-021 | Burbuja: texto, estado, hora, guía | `MessageBubble.tsx` | `features/conversation/` |
| FE-022 | Ver o reproducir multimedia bajo demanda | `MediaAttachment.tsx` | `features/conversation/media/` |
| FE-023 | Tiempo restante de la ventana de 24 h | `lib/time.ts` · `remainingGrace` | `features/conversation/window.ts` |
| FE-030 | Enviar texto | `MessageInput.tsx`, `lib/api.ts` | `features/composer/` |
| FE-031 | Adjuntar imagen, video o documento con vista previa y validación | `MessageInput.tsx:48-149` | `features/composer/attachments/` |
| FE-032 | Grabar y enviar una nota de voz | `MessageInput.tsx:151-221` | `features/composer/voice/useVoiceRecorder.ts` |
| FE-033 | Deshabilitar el envío con la ventana cerrada | `MessageInput.tsx:225` | `features/composer/` |
| FE-040 | Dashboard: cargar estadísticas por preset o rango | `DashboardApp.tsx:135` | `features/dashboard/useStats.ts` |
| FE-041 | Convertir las filas del backend en series por día | `DashboardApp.tsx:79-101` | `features/dashboard/series.ts` |
| FE-042 | Gráfica de barras apiladas, leyenda y desglose | `DashboardApp.tsx` | `features/dashboard/components/` |
| FE-043 | ⚠ Mostrar quién envió cada mensaje (bot, asesor o IA): el backend lo devuelve y el panel lo descarta | `lib/types.ts:48` | `features/conversation/` |
