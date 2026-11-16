# 01 · Diagnóstico del estado actual

Qué hace hoy cada servicio, archivo por archivo, para qué existe y qué le
falla. Es la base de la que salen el [inventario de casos de uso](02-casos-de-uso.md),
la [arquitectura objetivo](03-arquitectura-objetivo.md) y el
[plan de ejecución](04-plan-de-ejecucion.md).

Las referencias `archivo:línea` son del commit `bd82342` (rama `refactor`).
Los hallazgos de seguridad no están aquí: van en un plan aparte que no se
versiona (el repositorio es público).

- [1. Bot (Python · FastAPI)](#1-bot-python--fastapi--app)
- [2. Backend (Rust · Axum)](#2-backend-rust--axum--webappbackend)
- [3. Agente de IA (Rust · Axum)](#3-agente-de-ia-rust--axum--ai-agents)
- [4. Frontend (Next.js)](#4-frontend-nextjs--webappfrontend)
- [5. Infraestructura y repositorio](#5-infraestructura-y-repositorio)
- [6. Problemas transversales](#6-problemas-transversales)

**En una frase por servicio**

| Servicio | Tamaño | Estado |
|---|---|---|
| Bot | 4 512 líneas, 30 archivos, 178 tests | El más desordenado: `services/` mezcla seis tipos de cosas y la lógica de envío está copiada tres veces. |
| Backend | 9 364 líneas (≈3 600 son tests dentro de los archivos), 145 tests | Bien dividido en carpetas, mal dividido por dentro: archivos de 1 300 líneas, errores como texto y módulos que escriben en tablas de otros. |
| ai-agents | 7 575 líneas, 117 archivos (mediana: 53 líneas), 111 tests | El modelo a seguir. Solo le sobran dependencias hacia fuera: lee y escribe tablas del backend por SQL. |
| Frontend | 2 370 líneas, 18 archivos, sin tests | Componentes que hacen de todo y un cliente HTTP con la misma lógica repetida cuatro veces. |

---

## 1. Bot (Python · FastAPI) — `app/`

**Rol actual.** Tres cosas distintas en un mismo proceso:

1. **Entrada de WhatsApp**: recibe el webhook de Meta y reparte los eventos
   (estados, fallos, mensajes de clientes, PDFs de asociados).
2. **Cuatro flujos de notificación** que envían plantillas: guía en PDF de un
   asociado, Google Sheet, Dropi "reclame en oficina" y Dropi seguimiento por
   niveles.
3. **Cliente de seis sistemas externos**: Meta Graph API, backend de Rust,
   N8N/Dropi, Google Sheets, DeepSeek y ai-agents.

### 1.1 Mapa de archivos

| Archivo | Líneas | Qué hace y para qué | Cómo mejorar |
|---|---:|---|---|
| `main.py` | 16 | Crea la app FastAPI y monta 3 routers. | Sin `lifespan` (el directorio de asociados se carga en el primer request), sin `/health`, sin configuración de logging. Montar los routers desde el módulo de cada funcionalidad. |
| `config.py` | 58 | `Settings` inmutable leído de variables de entorno. | `load_dotenv()` como efecto secundario al importar. Todo tiene valor por defecto, también lo obligatorio: un despliegue mal configurado arranca igual y falla tarde, en el primer envío. Mezcla `os.getenv(x, d)` y `os.getenv(x) or d`. Pasar a `pydantic-settings` con validación al arrancar y settings por módulo. |
| `core/interfaces.py` | 273 | Los 10 puertos (ABC) del bot en un solo archivo. | `NotificationBackend` (`:118-263`) tiene 14 métodos de 6 recursos distintos: viola segregación de interfaces. `SheetSource.download_file` (`:63`) mete "descargar cualquier archivo" dentro de "leer hojas". Cada puerto debe vivir junto al módulo que lo consume. |
| `core/phones.py` | 42 | `normalize_phone` (quita `+`, pone `57` a los de 10 dígitos), `PhoneCountry`, `to_international`. | Dos estrategias que se pisan: la vieja solo-Colombia (`:15`) y la nueva por país. Duplica `webapp/backend/src/tools/phones.rs`. Una sola regla, por país. |
| `models/shipping.py` | 35 | `RecipientInfo`: destinatario de una guía, normaliza el teléfono al construirse. | Es el único modelo tipado; el resto del bot mueve `dict`. Hereda la regla solo-Colombia. |
| `models/quota.py` | 19 | Constantes `OFFICE`/`TRACKING` y `QuotaReservation`. | Constantes sueltas en vez de `Enum`. Debe vivir en el módulo de cupo. |
| `api/dependencies.py` | 179 | Composition root: construye todo con `lru_cache`. | Crea **5 instancias** de `WhatsAppClient` (`:58, :89, :105, :127, :158`). Un proveedor lanza `HTTPException` (`:75`): mezcla HTTP con cableado. El directorio de asociados se carga una vez y nunca se refresca. |
| `api/webhook.py` | 42 | `GET /webhook` (verificación de Meta) y `POST /webhook` (recibe eventos, procesa en segundo plano). | Lee los query params a mano y registra con `print`. |
| `api/notify_sheet.py` | 26 | `POST /notify-sheet`: lanza la corrida del Google Sheet. | No verifica que el business exista ni impide **dos corridas a la vez** (Dropi sí). `async def` mientras los de Dropi son `def`. |
| `api/notify_dropi.py` | 141 | `POST /notify-dropi-mcp` y `/notify-dropi-tracking`: validan filtros, verifican business, una corrida por business. | DTO, validación y control de corridas mezclados con el router. Es el mejor de los tres routers: usarlo de modelo. |
| `services/webhook_processor.py` | 163 | Interpreta el payload de Meta y dispara: estados → backend, fallos → backend, PDF de asociado → flujo de guía, mensaje de cliente → backend + listeners. | Mezcla parseo del payload, enrutamiento y efectos. `print(data)` (`:44`) como registro de cada evento. Separar parser (puro) + un handler por tipo de evento. |
| `services/shipping_notifier.py` | 187 | Flujo "guía en PDF": descarga → texto → LLM → escalación → plantillas → registro. | Contiene su propia copia de "enviar + desviar + copia debug + registrar" (`:147-187`). Depende de la clase concreta `AssociateDirectory`. |
| `services/sheet_notifier.py` | 316 | Flujo "Google Sheet": anti-join office/delivered → plan → cupo → envío. | Mezcla lectura de hojas, mapeo de filas, plan, cupo, envío y reporte. `_deliver` y `_register_outgoing` (`:181-226`) son copia literal de los de Dropi. Una guía registrada cuyo envío falló queda omitida para siempre (`:273`). |
| `services/dropi_orders.py` | 409 | Clase base de los dos flujos de Dropi: paginación, detalle del pedido, dirección, dinero, teléfono, video, cupo, envío. | Reutilización por **herencia**: una clase base de 20 métodos sin relación entre sí. `_deliver`/`_register_outgoing` (`:311-356`) duplicados. Trabaja con `dict` y claves mágicas. |
| `services/dropi_notifier.py` | 300 | Flujo "reclame en oficina": escalación diaria sin tope. | `_notify` (`:197-281`) repite ~80 % de `dropi_tracking_notifier._notify`. |
| `services/dropi_tracking_notifier.py` | 314 | Flujo "seguimiento por niveles": nivel 1 y 2, nunca retrocede. | Ídem. `TRACKING_LEVELS` (estados de Dropi) está en el código: cambiar un estado exige desplegar. |
| `services/notification_policy.py` | 101 | Escalación pura: `step_for_notification_count` (tope 3) y `daily_step` (sin tope). | Las dos funciones repiten la regla "una por día". Usa `date.today()` del contenedor (UTC): el "día" cambia a las 7 p. m. de Colombia. Inyectar reloj y zona del business. |
| `services/quota.py` | 88 | `RunQuota`: reserva el cupo de la corrida, `allows(phone)`, libera al salir. | Bien encapsulado. Mover a su módulo con su puerto propio. |
| `services/product_videos.py` | 184 | Encuentra el video del producto (por id o por palabras), lo sube una vez y cachea el `media_id` en disco. | Tres responsabilidades en una clase: emparejar (puro), caché persistente y subida. Sube el video **dentro del lock** (hasta 120 s bloqueando a las demás corridas). |
| `services/run_guard.py` | 27 | Una corrida por clave a la vez, en memoria. | Vale solo con un worker de uvicorn; no sobrevive a un reinicio. Documentarlo como restricción o moverlo a Postgres (advisory lock). |
| `services/associate_directory.py` | 49 | Teléfono de asociado → business, en memoria, con respaldo local. | Nunca se refresca: un asociado nuevo no existe para el bot hasta reiniciarlo. |
| `services/backend_client.py` | 343 | Cliente HTTP del backend: login por api-key, re-login en 401 y 14 operaciones. | Cliente dios. Cada método repite `try/except/print`. `business_exists` y `get_business_country` hacen el mismo `GET /businesses/{id}` sin caché. `_token` y la sesión se comparten entre hilos sin lock. Arma las rutas con f-strings (`:120, :186, :197`): un número de guía con `/` rompe la URL. |
| `services/dropi_client.py` | 98 | Cliente de los webhooks de N8N (`/ordenes`, `/orden`) con reintentos y backoff. | Reintentos propios; hay otras dos implementaciones distintas en el bot. `GET` con cuerpo JSON (lo exige N8N, pero es frágil ante proxies). |
| `services/google_sheets.py` | 37 | Descarga una hoja como CSV con polars; descarga un archivo por URL. | `download_file` (`:28`) no pertenece a un cliente de hojas: es una descarga genérica. `fetch_sheet` no tiene timeout ni manejo de errores (polars lee la URL directo). |
| `services/recipient_extractor.py` | 58 | DeepSeek extrae nombre, teléfono, guía, dirección y producto del texto del PDF. | El prompt fuerza `+57` (`:17`): no sirve para Guatemala. Modelo y URL fijos en el código. Sin validar el JSON devuelto. |
| `services/pdf_extractor.py` | 18 | Texto de un PDF con PyPDF2. | PyPDF2 está descontinuado (sucesor: `pypdf`). Sin límite de tamaño ni de páginas. |
| `services/ai_agents_client.py` | 42 | Avisa a ai-agents que un cliente escribió. | Bien acotado. Mover a `integrations/`. |
| `whatsapp/client.py` | 87 | Cliente de la Graph API `v19.0`: URL de media, descarga, subida y envío de plantilla. | Devuelve `None` ante cualquier error (se pierde el motivo). Sin reintentos. Versión de la API fija en el código y repetida en el backend. **Segundo cliente de Meta del sistema.** |
| `whatsapp/templates/base.py` | 53 | Contrato `TemplateMessage`: nombre, componentes, payload y metadatos para el historial. | `language_code = "es_CO"` fijo. |
| `whatsapp/templates/*.py` (8) | 32-54 c/u | Una clase por plantilla de Meta: `guia`, `mensaje_guia_es`, `recordatorio`, `recordatorio_final`, `pedido_oficina`, `pedido_enviado_2`, `pedido_camino_1`, `video_producto`. | `log_message()` copia a mano el texto de Meta (se desincroniza si la plantilla cambia). Tres constructores reciben un `media_id` que no usan "para mantener la firma". `__init__.py` exporta 4 de las 8. |
| `Dockerfile` (raíz) | 14 | Imagen del bot. | El contexto de build es todo el repositorio (`COPY . .` sin `.dockerignore`): la imagen del bot carga `webapp/`, `ai-agents/` y `tests/`. Consecuencia de que el bot viva en la raíz. |
| `requirements.txt` | 7 | Dependencias. | Sin versiones fijadas: dos builds del mismo commit pueden salir distintos. |

### 1.2 Problemas estructurales

| # | Problema | Evidencia |
|---|---|---|
| B-1 | **`services/` es un cajón de sastre.** 18 archivos de seis naturalezas: clientes HTTP de sistemas externos, orquestadores de casos de uso, políticas puras, infraestructura, cachés con estado y utilidades. No se puede saber qué hace el bot mirando las carpetas. | `app/services/` |
| B-2 | **La lógica de envío está copiada tres veces.** "Enviar plantillas en orden, desviar al número de pruebas, mandar copia de depuración, registrar en el historial" existe en tres sitios, dos de ellos idénticos carácter a carácter. | `shipping_notifier.py:147-187`, `sheet_notifier.py:181-226`, `dropi_orders.py:311-356` |
| B-3 | **Tres estilos de orquestación para el mismo problema.** El flujo PDF recibe fábricas de plantillas inyectadas; el del Sheet instancia las plantillas a mano; los de Dropi heredan de una clase base. | los tres notifiers |
| B-4 | **Herencia usada para compartir código.** `DropiOrderNotifier` no es un "es-un": es una bolsa de helpers. Los dos `_notify` hijos repiten la misma secuencia de 9 pasos. | `dropi_orders.py:73`, `dropi_notifier.py:197`, `dropi_tracking_notifier.py:227` |
| B-5 | **Puerto dios y cliente dios hacia el backend.** Un flujo que solo necesita "consultar guía" depende de 14 métodos. Cualquier fake de test debe implementar los 14. | `interfaces.py:118`, `backend_client.py` |
| B-6 | **Datos sin tipar.** Pedidos, guías y detalles viajan como `dict`; las claves (`tracking_code`, `notification_count`, `user_id`, `video_sent`…) se leen con `.get()` en unos 60 sitios. Un cambio de nombre en el backend no falla: devuelve `None` en silencio. | `dropi_*.py`, `sheet_notifier.py` |
| B-7 | **Composition root que duplica infraestructura** y esconde estado global en `lru_cache`. | `dependencies.py` |
| B-8 | **Manejo de errores sin criterio único.** Según el método: devuelve `None`, devuelve `False`, lanza, o anota en el reporte. El llamador tiene que saber cuál toca. | `backend_client.py`, `whatsapp/client.py` |
| B-9 | **`print` como logging (87 llamadas), en dos idiomas**, a veces en la misma función. Sin niveles ni correlación por corrida. | todo `app/` |
| B-10 | **Colombia fija en el código** pese a que ya hay businesses en Guatemala. | `phones.py:15`, `recipient_extractor.py:17`, `templates/base.py:16` |
| B-11 | **Tres implementaciones de reintentos.** | `dropi_client.py:83`, `backend_client.py:47` y `:295` |
| B-12 | **Inconsistencia funcional entre corridas.** `/notify-sheet` no verifica el business ni impide dos corridas a la vez; las de Dropi sí. | `notify_sheet.py` vs `notify_dropi.py:94` |
| B-13 | **Concurrencia implícita.** Las tareas en segundo plano corren en un pool de hilos y comparten sesiones HTTP, el token del backend y cachés. `RunGuard` y el caché de videos solo son correctos con un único worker. | `backend_client.py:33`, `run_guard.py` |
| B-14 | **Tests organizados por archivo, no por caso de uso.** Seis archivos de hasta 1 038 líneas; `test_bot_integration.py` cubre 10 temas. Los fakes del backend se repiten en cada archivo. | `tests/` |
| B-15 | **Dos clientes de Meta en el sistema.** El bot envía plantillas y sube media; el backend envía mensajes libres, sube y descarga media. La misma versión de la API fijada en dos sitios, dos manejos de error, dos configuraciones del mismo token. | `app/whatsapp/client.py`, `webapp/backend/src/messages/meta_client.rs` |

---

## 2. Backend (Rust · Axum) — `webapp/backend/`

**Rol actual.** Es el único dueño de la base de datos (salvo lo que ai-agents
lee y escribe por su cuenta) y hace cuatro trabajos:

1. **Persistencia para el bot**: guías, mensajes, estados, errores de entrega,
   cupo.
2. **API de la webapp**: login, bandeja, historial, dashboard.
3. **Canal de WhatsApp para mensajes libres**: texto y multimedia de asesores
   y del agente de IA, más el túnel de descarga de multimedia.
4. **Reglas con estado**: ventana de 24 h, cupo de mensajería, libro de
   notificaciones de guías y su deshacer.

### 2.1 Mapa de archivos

Líneas: código + tests dentro del mismo archivo.

| Archivo | Líneas | Qué hace y para qué | Cómo mejorar |
|---|---:|---|---|
| `main.rs` | 183 + 78 | `build_app` (repositorios → servicios → estados → rutas), lectura de variables y arranque. | La configuración se lee aquí y también dentro de un servicio (`messages/service.rs:201`). `DATABASE_URL` tiene valor por defecto. Pool fijo en 5 conexiones. `println!`, sin apagado ordenado ni `/health`. |
| `state.rs` | 67 | Ocho structs `*State`, uno por módulo. | Todos cargan un `AppState {}` vacío. `ChatState` lleva tres servicios y `BusinessState` el de guías: los estados delatan los cruces entre módulos. |
| `db.sql` | 456 | Esquema completo, idempotente: se vuelve a correr para migrar. | Mezcla DDL con correcciones de datos (`UPDATE` de backfill en cada corrida). Sin historial de qué versión tiene cada base. `TIMESTAMP` sin zona. Tabla `invoices` sin uso. Tablas del agente de IA y catálogo de productos definidas aquí aunque el backend no las toca. |
| `tools/responses.rs` | 27 | `json_response` y respuesta de validación. | Cada handler arma su sobre con `serde_json::json!`: la clave del dato cambia según el endpoint (`data`, `chats`, `guide`, `business`…). |
| `tools/custom_validators.rs` | 27 + 21 | `validate_non_blank`, política de contraseñas. | La política de contraseñas es una regla de `auth`, no una herramienta. |
| `tools/phones.rs` | 36 + 26 | `PHONE_REGEX`, `normalize_phone`, `to_international`. | Tercera copia de la regla (bot y ai-agents tienen la suya). Convertir en un tipo `Phone`. |
| `tools/message_lock.rs` | 53 + 11 | Bloqueo por mensaje (`pg_advisory_xact_lock`) y lista de errores permanentes de Meta. | Regla de negocio (qué error deshace una notificación) guardada en `tools/`. La usan `messages` y `guides`: es el pegamento oculto entre los dos módulos. |
| `auth/controller.rs` | 116 | Login de asociado, login por api-key, cambio de contraseña (montado bajo `/associates`). | Traduce errores comparando textos (`"Associate not found"` → 404). |
| `auth/dtos.rs` | 71 + 38 | DTOs, `Claims` y la regla `can_access_business`. | `kind` es un `String` comparado con literales. Debe ser un enum con permisos explícitos por tipo de token. |
| `auth/middleware.rs` | 91 + 23 | `require_auth`, extractor `AuthenticatedClaims`, `authorize_business`, `require_bot`. | La autorización es una llamada manual al inicio de cada handler (32 sitios): olvidarla no da error de compilación. |
| `auth/repository.rs` | 96 | Credenciales de asociados y api-keys. | Lee `business_associates`, la misma tabla que `businesses/repository.rs`: dos módulos sobre una tabla. |
| `auth/service.rs` | 166 + 213 | Emisión, validación y renovación de JWT; cambio de contraseña. | Correcto. Duraciones fijas en el código. |
| `auth/test_support.rs` | 63 | `FakeAuthService` y helpers de tests de router. | Hay otras dos copias de este fake en `chats/controller.rs` y `messages/controller.rs`. |
| `businesses/controller.rs` | 266 + 164 | Businesses, asociados, configuración del sheet y **estadísticas** (que resuelve `guide_service`). | Cuatro recursos en un controlador. |
| `businesses/dtos.rs`, `repository.rs`, `service.rs` | 101, 208, 105 (+ tests) | Empresas, países, asociados y hoja de cálculo. | `BusinessServiceTrait` con 8 métodos de 4 recursos. Verifica unicidad con `SELECT` previo y además por índice (bien), pero los errores viajan como constantes de texto (`PHONE_TAKEN`). |
| `users/*` | 220 | Clientes finales: upsert, listar, consultar. | El nombre confunde (los usuarios de la webapp son los asociados). `GET /users` y `GET /users/{phone}` no tienen consumidor. |
| `chats/controller.rs` | 351 + 531 | Bandeja e importancia… y también **envío de texto, de multimedia y del agente de IA**. | 130 líneas de parseo de multipart dentro del handler (`:147-277`). El estado HTTP sale de buscar palabras en el mensaje de error (`e.contains("ffmpeg")`, `:270`). La renovación del token vive dentro del envío de mensajes. |
| `chats/repository.rs` | 178 | Bandeja, upsert de chat, último mensaje, business más reciente, importancia. | Sin paginación y con una subconsulta por chat (`:47`). `touch_guide_notification` (`:140`) actualiza los chats del cliente en **todos** los business. |
| `chats/service.rs` | 50 + 105 | Bandeja con `window_open`, importancia. | Importa `messages::service::is_window_open_at`: `chats` depende de `messages` y `messages` de `chats`. |
| `messages/service.rs` | 634 + 708 | Regla de 24 h, vista previa, validación de multimedia, **transcodificador ffmpeg**, `MessageServiceTrait` (11 métodos) y `MessageService`. | El archivo más grande del sistema. Infraestructura (lanzar procesos, archivos temporales) dentro de un servicio de dominio. Tres pasos de escritura sin transacción en `register_incoming` (`:505-561`). |
| `messages/repository.rs` | 291 | Guardar, historial, estados, **registrar el fallo y deshacer la notificación de la guía**. | Escribe en `guide_notifications` y `guides` (`:235-257`), tablas de otro módulo. `update_status_by_meta_id` permite retroceder de `delivered` a `sent` (`:172`). |
| `messages/meta_client.rs` | 271 + 66 | Cliente de Meta: texto, subida, multimedia, descarga. | `reqwest::Client::new()` sin timeout (`:50`). Errores como `String` con el cuerpo de la respuesta de Meta. Descarga el archivo completo a memoria. |
| `messages/controller.rs` | 190 + 358 | Registrar entrante/saliente, estado, error, túnel de multimedia. | El túnel de multimedia no es un caso de uso de "registrar mensajes": merece su propio módulo. |
| `messages/dtos.rs` | 190 + 49 | `Message`, enums y DTOs. | `Message.sender` es `Option<String>` aunque existe el enum `MessageSender`. |
| `guides/controller.rs` | 204 + 73 | Registrar, listar, consultar, lookup, marcar notificada, marcar seguimiento. | Bien. |
| `guides/repository.rs` | 374 | Guías, datos de pedido, productos, **libro de notificaciones**, estadísticas. | Lee `messages` para calcular `video_sent` (`:214`) y `messages_errors` vía `message_lock`. Las estadísticas agrupan por día UTC (`:343`). |
| `guides/service.rs` | 163 + 351 | Registro, lookup, marcas. | `register_guide` hace upsert de cliente, insert y detalles en tres sentencias sin transacción. Tras marcar, toca la bandeja (`chat_repository`). |
| `quota/dtos.rs` | 262 + 111 | DTOs… y el **algoritmo de asignación** (`allocate`, `:161`). | La regla más delicada del negocio vive en un archivo de DTOs. |
| `quota/repository.rs` | 279 | Cupo: CTEs sobre `messages`, `guide_notifications` y reservas; bloqueo global. | El SQL mejor documentado del proyecto, y sin un solo test contra Postgres. Lee tablas de dos módulos. |
| `quota/service.rs`, `controller.rs` | 68, 96 (+ tests) | Estado, reservar, liberar. | Bien. |

### 2.2 Problemas estructurales

| # | Problema | Evidencia |
|---|---|---|
| BE-1 | **Los errores son texto.** 305 firmas `Result<_, String>`. Los controladores deciden el código HTTP comparando o buscando palabras en el mensaje (33 sitios), y cualquier error de base de datos sale como 400 con el texto de sqlx. Cambiar una frase cambia el comportamiento de la API. | `chats/controller.rs:107, :267-273`, `auth/controller.rs:101` |
| BE-2 | **La autorización se escribe a mano en cada handler.** 32 llamadas a `require_bot` / `authorize_business`. El hallazgo 1 del documento de errores (ya corregido) fue exactamente un olvido de estas llamadas; el diseño sigue permitiendo el siguiente. | `auth/middleware.rs:68, :82` |
| BE-3 | **Archivos dios.** `messages/service.rs` (634 líneas de código, cinco temas) y `chats/controller.rs` (envío de mensajes dentro del módulo de bandeja). | — |
| BE-4 | **Traits gordos.** `MessageServiceTrait` 11 métodos, `BusinessServiceTrait` 8, `ChatRepositoryTrait` 7, `GuideServiceTrait` 7. Cada fake de test implementa todos: 29 `unimplemented!()` / `unreachable!()`. | `messages/service.rs:314` |
| BE-5 | **Los módulos se cruzan por la base de datos.** `messages` escribe tablas de `guides`; `guides` lee `messages`; `quota` lee las de ambos; `auth` y `businesses` comparten `business_associates`; `guides` actualiza `chats`. Las carpetas dicen una cosa y el SQL otra. | `messages/repository.rs:235`, `guides/repository.rs:214`, `quota/repository.rs:80-115` |
| BE-6 | **Las fronteras no siguen los casos de uso.** `businesses` = empresas + países + asociados + hoja + ruta de estadísticas. `chats` = bandeja + envío. `messages` = registro + estados + errores + túnel + cliente de Meta + ffmpeg. `guides` = registro + pedido + libro de notificaciones + estadísticas. | `main.rs:102-114` |
| BE-7 | **Casos de uso de varios pasos sin transacción.** Registrar un entrante, registrar una guía y marcar una notificación hacen de 2 a 4 escrituras sueltas: un fallo a mitad deja el chat actualizado sin mensaje, o la guía sin detalles. | `messages/service.rs:505-561`, `guides/service.rs:61-84, :124-142` |
| BE-8 | **Reglas de negocio fuera de sitio.** El algoritmo de cupo en `dtos.rs`; los errores permanentes de Meta en `tools/`; la regla de 24 h en el servicio de mensajes pero usada por `chats`. | `quota/dtos.rs:161`, `tools/message_lock.rs:19` |
| BE-9 | **Sin paginación en ninguna lista.** La bandeja y el historial devuelven todo; el frontend los pide enteros cada 15 s y cada 5 s. | `chats/repository.rs:37`, `messages/repository.rs:152` |
| BE-10 | **Tests dentro de los archivos y fakes repetidos.** El 39 % del código fuente son módulos `#[cfg(test)]`; `FakeAuthService` existe tres veces. | `chats/controller.rs:352-882` |
| BE-11 | **El SQL no tiene tests.** Ningún `repository.rs` tiene una prueba contra Postgres: ni el cupo, ni el deshacer, ni el bloqueo por mensaje. Los tests del backend "no requieren base de datos" (README) porque todo lo que la toca está sin probar. | los 7 `repository.rs` |
| BE-12 | **Configuración y observabilidad artesanales.** Variables leídas en `main.rs` y en un constructor; `println!` (8) como log; sin trazas por petición. ai-agents ya resolvió ambas cosas (`config/`, `tracing`). | `main.rs:126-180` |
| BE-13 | **Endpoints y tablas sin consumidor.** `GET /associates/phones`, `GET /users`, `GET /users/{phone}`, `GET /guides?user_phone=`, `GET /quota` y la tabla `invoices`. | búsqueda en bot, ai-agents y frontend: 0 usos |
| BE-14 | **Nombres heredados.** El crate se llama `mania-back-end`; el README habla de `src-EJEMPLO`; `users` son clientes. | `Cargo.toml:2` |

---

## 3. Agente de IA (Rust · Axum) — `ai-agents/`

**Rol actual.** Responde a los clientes por WhatsApp con mensajes libres.
Recibe un aviso del bot por cada mensaje, corre un turno en segundo plano
(9 pasos) y entrega la respuesta a través del backend.

**Es el servicio a imitar.** Lo que el bot y el backend necesitan ya está
resuelto aquí:

| Práctica | Dónde verla |
|---|---|
| Un archivo por paso, por regla y por herramienta | `steps/`, `policy/rules/`, `tools/` |
| Puertos pequeños, uno por responsabilidad, junto a quien los usa | `agents/customer_service/ports.rs` (8 traits de 1-2 métodos) |
| Un solo composition root por módulo | `assembly.rs`, `app.rs` |
| Errores tipados (`thiserror`) | `ports.rs`, `integrations/backend/mod.rs` |
| Configuración por secciones, validada al arrancar, que nombra la variable que falta | `config/` |
| Reintentos en un único sitio | `shared/retry.rs` |
| Logs estructurados con el teléfono enmascarado | `domain/chat.rs`, `shared/text.rs:12` |
| Reglas puras probadas sin base de datos, y repositorios probados contra Postgres | `policy/rules/*`, `tests/postgres_repositories.rs` |
| Abierto/cerrado de verdad: agregar una regla, paso o herramienta es un archivo y una línea | tabla "Cómo cambiar o agregar cosas" de su README |

### 3.1 Mapa de carpetas

| Carpeta | Archivos | Qué hace | Cómo mejorar |
|---|---:|---|---|
| `main.rs`, `app.rs`, `lib.rs` | 3 | Arranque, apagado ordenado, clientes compartidos, rutas. | — |
| `config/` | 7 | `Env` tipado, una sección por archivo, `.env` opcional. | — |
| `http/` | 3 | Middleware `api-key`, `/health`. | — |
| `shared/` | 7 | Reintentos, HTTP JSON, plantillas de texto, locks por clave, texto (truncar, enmascarar, normalizar teléfono, palabras de búsqueda). | `normalize_phone` y `search_words` repiten reglas que ya existen en el bot y el backend. |
| `llm/`, `transcription/` | 13 | Puertos `ChatModel` y `Transcriber` con sus adaptadores (DeepSeek, Gemini). | — |
| `agent/`, `pipeline/` | 6 | Runtime genérico: bucle modelo ↔ herramientas y pipeline de pasos. | Candidatos a patrón común: el bot necesita justo un pipeline así para sus corridas. |
| `integrations/backend/` | 3 | Token por api-key con caché, descarga de multimedia, envío del mensaje de IA. | Segunda implementación del cliente del backend (la otra es la del bot). |
| `integrations/dropi/` | 2 | `/orden` de N8N para el estado en vivo. | Segunda implementación del cliente de N8N. |
| `agents/customer_service/steps/` | 10 | Los 9 pasos del turno. | — |
| `agents/customer_service/policy/` | 10 | 8 reglas de elegibilidad. | — |
| `agents/customer_service/tools/` | 6 | 5 herramientas del modelo. | — |
| `agents/customer_service/repositories/` | 12 | SQL directo sobre 12 tablas. | **El problema del servicio** (ver AG-1). |
| `agents/customer_service/presentation/` | 7 | Formato para el prompt: fechas, dinero, guías, productos, historial. | `money.rs` es la segunda implementación del formato de moneda (la otra está en el bot). |
| `agents/customer_service/domain/`, `prompts/` | 12 | Tipos del agente y el texto del prompt (`system.md`, `user.md`). | — |
| `tests/` | 4 | Turno completo con fakes, DeepSeek con servidor falso, repositorios contra Postgres. | `tests/common/mod.rs:15` incluye `webapp/backend/db.sql`: el test depende de un archivo de otro servicio. Es útil como prueba de contrato, pero hoy es accidental. |

### 3.2 Problemas estructurales

| # | Problema | Evidencia |
|---|---|---|
| AG-1 | **Comparte la base de datos con el backend.** Lee por SQL 12 tablas (`chats`, `messages`, `businesses`, `countries`, `users`, `guides`, `guide_products`, `guide_notifications`, `products`, `product_dropi_ids`, `business_ai_agents`, `ai_escalations`) y escribe en 4 (`messages.transcription`, `chats.is_important`, `ai_escalations`, `guide_customer_updates`). Cualquier cambio de columna en el backend puede romper al agente sin que compile nada distinto. | `repositories/*.rs` |
| AG-2 | **Sus tablas no son suyas.** `business_ai_agents`, `ai_escalations` y `guide_customer_updates` se definen en `webapp/backend/db.sql`; el agente no puede evolucionarlas sin tocar otro servicio. | `db.sql:316-415` |
| AG-3 | **Escribe datos del backend sin pasar por él.** Marca chats como importantes y guarda transcripciones con `UPDATE` directo, saltándose las reglas y la autorización del backend. | `repositories/escalations.rs:19`, `voice_notes.rs:21` |
| AG-4 | **Clientes duplicados con el bot.** Backend (login + 401), N8N/Dropi y DeepSeek existen dos veces en dos lenguajes, con políticas de reintento distintas. | `integrations/` vs `app/services/` |
| AG-5 | **Documentación que apunta a archivos que no están.** El README describe `scripts/seed_products.sql` y `scripts/seed_test_chat.sql`; la carpeta `scripts/` está vacía en el repositorio. | `ai-agents/README.md:98, :220` |

---

## 4. Frontend (Next.js) — `webapp/frontend/`

**Rol actual.** Panel de los asociados: login, bandeja de chats con búsqueda
y filtros, historial con multimedia, envío de texto, archivos y notas de
voz, y dashboard de notificaciones.

### 4.1 Mapa de archivos

| Archivo | Líneas | Qué hace | Cómo mejorar |
|---|---:|---|---|
| `app/**/page.tsx`, `layout.tsx` | 5-26 c/u | Rutas: `/login`, `/chats`, `/chats/[businessId]`, `/chats/[businessId]/dashboard`. | Bien: solo componen. |
| `app/api/backend/[...path]/route.ts` | 50 | Proxy de todo `/api/backend/*` hacia el backend. | Proxy genérico: no sabe qué rutas usa la webapp. |
| `lib/api.ts` | 266 | Cliente del backend: login, business, chats, mensajes, estadísticas, multimedia. | La secuencia "poner token → fetch → 401 → error" está escrita cuatro veces (`request`, `login`, `fetchMediaBlob`, `sendChatMedia`). `defaultMediaFilename` (`:240`) es una cadena de 14 ternarios que repite `default_filename` del backend. |
| `lib/auth.ts` | 53 | Token en `localStorage`, decodificar claims, validez. | — |
| `lib/time.ts` | 92 | Fechas en hora local, tiempo restante de la ventana. | — |
| `lib/types.ts` | 61 | Tipos de la API escritos a mano. | Desfasados del backend: `Message` no tiene `sender`, así que el panel no distingue un mensaje del bot, de un asesor o de la IA. |
| `components/ChatApp.tsx` | 293 | Guardia de sesión, polling de la bandeja, búsqueda, filtros, selección, orden local y todo el layout. | Seis responsabilidades. La guardia de sesión (`:39-50`) está copiada en `DashboardApp.tsx:121-132`. |
| `components/ChatWindow.tsx` | 85 | Historial con polling cada 5 s. | Recarga el historial completo cada vez. |
| `components/MessageBubble.tsx` | 94 | Burbuja, estados, guía. | Bien. |
| `components/MediaAttachment.tsx` | 104 | Carga multimedia bajo demanda como blob. | Decide cómo mostrar por `mediaType`, no por el tipo real del archivo. |
| `components/MessageInput.tsx` | 434 | Texto, adjuntos, vista previa, grabación de voz y validaciones. | Tres componentes en uno. Límites y tipos permitidos repetidos del backend (`:13-21`). |
| `components/DashboardApp.tsx` | 577 | Guardia, carga, pivote de datos, selector de fechas y gráfica. | Las funciones puras (`buildDailyStats`, `generateDateRange`, `seriesOf`) están bien pero enterradas en el componente y sin tests. |
| `components/ChatListItem.tsx` | 78 | Fila de la bandeja. | Bien. |

### 4.2 Problemas estructurales

| # | Problema | Evidencia |
|---|---|---|
| FE-1 | **Organizado por tipo de archivo** (`components/`, `lib/`), no por funcionalidad: para tocar "enviar una nota de voz" hay que abrir `MessageInput.tsx`, `api.ts` y `types.ts`. | `src/` |
| FE-2 | **Componentes con demasiadas responsabilidades**: datos, estado, reglas y presentación juntos. Ningún hook propio. | `ChatApp.tsx`, `MessageInput.tsx`, `DashboardApp.tsx` |
| FE-3 | **Contrato con el backend mantenido a mano** y ya desfasado. | `lib/types.ts` |
| FE-4 | **Reglas duplicadas con el backend**: límites de tamaño, tipos MIME, nombres por defecto, ventana de 24 h. | `MessageInput.tsx:13-70`, `api.ts:240` |
| FE-5 | **Polling completo** de bandeja (15 s) e historial (5 s) sin paginación ni "desde el último". | `ChatApp.tsx:13`, `ChatWindow.tsx:10` |
| FE-6 | **Sin tests ni comprobación de tipos en CI.** | `package.json` |

---

## 5. Infraestructura y repositorio

| # | Problema | Evidencia |
|---|---|---|
| IN-1 | **El bot vive en la raíz del repositorio** (`app/`, `tests/`, `Dockerfile`, `requirements.txt`), los otros tres servicios en su carpeta. Por eso su imagen arrastra todo el repo y por eso "la raíz" parece el bot. | árbol del repo |
| IN-2 | **Sin integración continua.** No hay `.github/`: 434 tests que nadie corre automáticamente antes de mezclar. Es la primera cosa que hace falta para una refactorización de este tamaño. | — |
| IN-3 | **Migraciones por "volver a correr `db.sql`"**, con pasos manuales previos descritos en el README. | `README.md:528-539` |
| IN-4 | **Un README de 645 líneas** que mezcla manual de operación, referencia de API, historia de decisiones y guía de despliegue, y que ya se contradice con el código (hallazgo 14 del documento de errores). | `README.md` |
| IN-5 | **El contrato entre servicios no está escrito en ningún sitio.** La API del backend solo existe en una tabla del README; el evento hacia ai-agents, en un docstring. | — |
| IN-6 | **`docker-compose.yml` como única fuente de qué variable recibe cada servicio**, mantenida a mano en tres sitios (código, compose, `.env.example`). | `README.md:490-493` |
| IN-7 | **Dependencias sin fijar en el bot** y sin auditoría automática en ninguno. | `requirements.txt` |

---

## 6. Problemas transversales

### 6.1 Lo mismo, implementado varias veces

| Qué | Bot (Python) | Backend (Rust) | ai-agents (Rust) | Frontend (TS) |
|---|---|---|---|---|
| Cliente de Meta | `whatsapp/client.py` | `messages/meta_client.rs` | — | — |
| Cliente del backend (login por api-key, reintento en 401) | `services/backend_client.py` | — | `integrations/backend/` | `lib/api.ts` |
| Cliente de N8N / Dropi | `services/dropi_client.py` | — | `integrations/dropi/` | — |
| Cliente de DeepSeek | `services/recipient_extractor.py` | — | `llm/deepseek/` | — |
| Normalizar teléfonos | `core/phones.py` | `tools/phones.rs` | `shared/text.rs:24` | — |
| Formato de dinero por moneda | `dropi_orders.py:385` | — | `presentation/money.rs` | — |
| Emparejar nombres de producto (sin tildes, conectores, parcial ≥ 4) | `product_videos.py:175` | — | `shared/text.rs:37`, `domain/product.rs` | — |
| Reintentos con espera exponencial | 3 versiones | ninguna | `shared/retry.rs` | — |
| Ventana de 24 h | — | `messages/service.rs:22` | `policy/rules/service_window.rs` | `lib/time.ts` |
| Límites y tipos de multimedia | `product_videos.py:43` | `messages/service.rs:48-122` | — | `MessageInput.tsx:13-70` |
| Significado de `notification_count` / nivel | `notification_policy.py`, `dropi_tracking_notifier.py` | `guides/dtos.rs:178` | `domain/guide.rs:23` | `DashboardApp.tsx:79` |
| Texto de las plantillas | `templates/*.py` (`log_message`) | `db.sql:335-351` (backfill por `LIKE`) | — | — |

Entre lenguajes distintos no se puede compartir código, pero sí se puede
decidir **quién es el dueño de cada regla** y que los demás la consuman en
vez de reimplementarla. Eso es lo que hace la arquitectura objetivo.

### 6.2 Cuatro estilos para las mismas decisiones

| Decisión | Bot | Backend | ai-agents | Frontend |
|---|---|---|---|---|
| Errores | `None` / `False` / excepción / reporte | `Result<_, String>` | enums con `thiserror` | `ApiError` |
| Logs | `print`, español e inglés | `println!` | `tracing` estructurado | — |
| Configuración | dataclass con valores por defecto | `std::env::var` en `main` | `Settings` tipado que falla al arrancar | `process.env` |
| Inyección de dependencias | `lru_cache` + funciones | `*State` por módulo | `assembly.rs` | imports directos |
| Reutilización | herencia y copiar-pegar | traits grandes | composición con traits pequeños | copiar-pegar |
| Tests | por archivo, fakes repetidos | dentro del archivo | por comportamiento, fakes compartidos | no hay |
| Idioma de los mensajes | mezclado | inglés (API) y español (validación) | inglés (logs), español (dominio) | español |

### 6.3 Fronteras entre servicios

1. **Dos servicios hablan con Meta** (B-15). El envío y su registro están
   partidos: el bot envía y después le cuenta al backend. De ahí sale la
   carrera que obliga al bloqueo por mensaje (`tools/message_lock.rs`): el
   aviso de fallo de Meta puede llegar antes de que el backend sepa que el
   mensaje existe.
2. **Dos servicios escriben en la misma base** (AG-1, AG-3).
3. **El bot es un relevo** para tres de los cuatro eventos del webhook:
   recibe de Meta y reenvía al backend sin añadir nada.
4. **Ningún contrato versionado** entre los cuatro (IN-5).

### 6.4 Defectos de comportamiento encontrados

No se corrigen durante el movimiento de código (regla 1 del plan). Quedan
marcados con ⚠ en el inventario y tienen su paso propio.

| Defecto | Dónde | Ya estaba en el documento de errores |
|---|---|---|
| Guía del sheet registrada cuyo envío falló: se omite para siempre | `sheet_notifier.py:273` | n.º 6 |
| Una notificación reordena la bandeja de otros business | `chats/repository.rs:140` | n.º 8 |
| El estado de un mensaje puede retroceder de `delivered` a `sent` | `messages/repository.rs:172` | n.º 9 |
| Asociados nuevos invisibles para el bot hasta reiniciarlo | `associate_directory.py` | n.º 10 |
| "Una por día" y el dashboard usan el día UTC | `notification_policy.py:49`, `guides/repository.rs:343` | n.º 11 |
| El flujo PDF fuerza `+57`: no sirve para business de otro país | `recipient_extractor.py:17`, `phones.py:15` | nuevo |
| `/notify-sheet` permite dos corridas simultáneas del mismo business | `notify_sheet.py` | nuevo |
| El panel no distingue quién envió cada mensaje saliente | `lib/types.ts:48` | nuevo |
