# 03 · Arquitectura objetivo

A dónde llega el código al terminar la refactorización: quién es dueño de
qué, cómo se ve un módulo por dentro, qué puede importar a qué y qué
convenciones son iguales en los cuatro servicios.

- [1. Lo que tomamos de VictorStore-Back](#1-lo-que-tomamos-de-victorstore-back)
- [2. Dueño único de cada capacidad](#2-dueño-único-de-cada-capacidad)
- [3. El mismo esqueleto en todos los servicios](#3-el-mismo-esqueleto-en-todos-los-servicios)
- [4. Anatomía de un módulo](#4-anatomía-de-un-módulo)
- [5. Reglas de dependencia](#5-reglas-de-dependencia)
- [6. Estructura objetivo por servicio](#6-estructura-objetivo-por-servicio)
- [7. Convenciones transversales](#7-convenciones-transversales)
- [8. SOLID y patrones: dónde se aplica cada uno](#8-solid-y-patrones-dónde-se-aplica-cada-uno)
- [9. Contratos entre servicios](#9-contratos-entre-servicios)
- [10. Decisiones que necesito de ti](#10-decisiones-que-necesito-de-ti)

---

## 1. Lo que tomamos de VictorStore-Back

Revisé `API/src` completo. Lo que hace que ese proyecto se sienta ordenado
son seis decisiones, y las seis se pueden llevar a Python y a Rust:

| En VictorStore | Por qué funciona | Aquí |
|---|---|---|
| Una carpeta por funcionalidad (`users/`, `orders/`, `products/`…) con **el mismo contenido en todas** | Se sabe dónde está cada cosa sin buscar | [Anatomía de un módulo](#4-anatomía-de-un-módulo), idéntica en los cuatro servicios |
| `x.controller.ts` solo habla HTTP, `x.service.ts` tiene la lógica, el repositorio tiene el SQL | Cada archivo cambia por una sola razón | Igual, con el servicio dividido en casos de uso |
| Cada servicio implementa una interfaz declarada en `interfaces/` | Se puede sustituir y probar | `ports` por módulo |
| `x.module.ts` declara qué importa y qué exporta | Las dependencias están escritas, no implícitas | `mod.rs` / `module.py` como único punto de entrada |
| Lo transversal en `tools/` (guards, decorators, strategies) y la infraestructura aparte (`database/`, `email/`) | El negocio no se mezcla con la plomería | `shared/` e `integrations/` |
| Seguro por defecto: guard global y `@Public()` para las excepciones; permisos declarados con `@CheckPolicies` | Olvidarse de proteger una ruta es imposible | Extractores de autorización tipados en el backend |

Y cuatro cosas de ese proyecto que **no** copiamos:

| En VictorStore | Problema | Aquí |
|---|---|---|
| Todos los repositorios en `database/repositories/` | Todos los módulos dependen de un módulo central; cualquiera puede leer la tabla de cualquiera | Cada módulo es dueño de su repositorio y de sus tablas |
| `OrdersService` recibe la clase `ProductsService` | Un módulo conoce la implementación de otro | Los módulos se hablan por puertos |
| Un `service.ts` por módulo que crece sin tope (`products.service.ts`: ocho métodos casi iguales) | Vuelve a ser un archivo dios | Un caso de uso por archivo |
| El "¿es el dueño?" dentro de cada controlador (`if req.user.id != id`) | Regla de seguridad repetida a mano | En la capa de autorización, una vez |

---

## 2. Dueño único de cada capacidad

La regla que arregla "dos servicios envían por Meta": **cada capacidad tiene
un solo dueño, y los demás la piden**.

| Capacidad | Dueño | Cómo la usan los demás | Hoy |
|---|---|---|---|
| Hablar con Meta: enviar plantillas y mensajes libres, subir y descargar multimedia | **backend** · `integrations/whatsapp` | bot, ai-agents y webapp, por la API del backend | bot y backend, cada uno con su cliente |
| Guardar un mensaje enviado | **backend**, en la misma operación que lo envía | — | el bot envía y después avisa |
| Recibir el webhook de Meta | **bot** · `modules/webhook` (ver decisión D2) | — | bot |
| Base de datos de negocio | **backend** | ai-agents lee por vistas de contrato; todo lo demás, por API | backend y ai-agents por SQL |
| Tablas del agente de IA | **ai-agents**, con sus propias migraciones | — | definidas en `webapp/backend/db.sql` |
| Decidir qué notificar, a quién y cuándo | **bot** | — | bot |
| Catálogo de plantillas (nombre, variables, texto para el historial) | **bot** · `modules/templates` | el backend recibe la plantilla ya armada y no la interpreta | bot |
| Cupo de mensajería | **backend** · `modules/quota` | bot | igual |
| Regla de la ventana de 24 h | **backend** · `modules/messages/window` | la webapp lee `window_open`; ai-agents la recibe en su vista | 3 implementaciones |
| Países, indicativos y zonas horarias | **backend** · tabla `countries` | bot y ai-agents reciben el país del business | 3 normalizadores |
| Conversar con los clientes | **ai-agents** | — | igual |

Cuando una regla tiene que existir en dos lenguajes (normalizar un teléfono
en Python y en Rust), no se comparte código: se comparten **los casos de
prueba**. Un archivo en `contracts/fixtures/` con entradas y salidas
esperadas, que los tests de cada servicio leen. Si las implementaciones se
separan, falla el CI.

```
                         ┌──────────────┐
            webhook      │     bot      │  decide qué notificar
  Meta ─────────────────▶│   (Python)   │──────────┐
    ▲                    └──────┬───────┘          │ corridas: Sheets, Dropi (N8N)
    │                           │ API              ▼
    │  único cliente     ┌──────▼───────┐    ┌───────────┐
    └────────────────────│   backend    │◀───│ ai-agents │──▶ DeepSeek, Gemini, N8N
       enviar, subir,    │    (Rust)    │ API│  (Rust)   │
       descargar         └──┬────────▲──┘    └─────┬─────┘
                            │        │ API         │ vistas de contrato (solo lectura)
                       ┌────▼───┐  ┌─┴────────┐    │
                       │Postgres│◀─┼──────────┼────┘
                       └────────┘  │ frontend │
                                   └──────────┘
```

---

## 3. El mismo esqueleto en todos los servicios

Cuatro carpetas de primer nivel, con el mismo significado en Python, Rust y
TypeScript:

| Carpeta | Contiene | No contiene |
|---|---|---|
| `bootstrap/` | Configuración, logging y el único sitio que conoce implementaciones concretas y las conecta | Reglas de negocio |
| `shared/` | Lo que no es de ningún caso de uso y no habla con nadie: tipos de valor, reintentos, reloj, errores base | Nada que importe un módulo o una integración |
| `integrations/` | Un adaptador por sistema externo: implementa puertos que declaran los módulos | Decisiones de negocio |
| `modules/` | Los casos de uso, una carpeta por funcionalidad | SQL de tablas ajenas, clientes HTTP, lectura de variables de entorno |

ai-agents ya tiene esta forma con otros nombres (`app.rs` + `config/`,
`shared/`, `integrations/` + `llm/` + `transcription/`, `agents/`). No se
renombra: se documenta la equivalencia.

---

## 4. Anatomía de un módulo

Todos los módulos, en los tres lenguajes, tienen las mismas piezas con el
mismo nombre. Una pieza que no hace falta no se crea.

| Pieza | Rust | Python | TypeScript | Responsabilidad única |
|---|---|---|---|---|
| Entrada del módulo | `mod.rs` | `__init__.py` + `module.py` | `index.ts` | Lo único que el resto puede importar. Construye el módulo a partir de sus puertos |
| Entrada HTTP | `controller.rs` | `router.py` | `page.tsx` / componente contenedor | Traducir HTTP ↔ caso de uso. Sin decisiones |
| Casos de uso | `use_cases/<verbo>.rs` | `use_cases/<verbo>.py` | `use<Verbo>.ts` | Una operación completa del negocio. Orquesta dominio y puertos |
| Dominio | `domain.rs` | `domain.py` | `<tema>.ts` | Tipos y reglas puras. Sin I/O, sin reloj, sin entorno |
| Contratos de entrada y salida | `dtos.rs` | `schemas.py` | `types.ts` | La forma de lo que entra y sale, con su validación |
| Puertos | `ports.rs` | `ports.py` | `ports.ts` | Lo que el módulo necesita de fuera, en interfaces de 1 a 5 métodos |
| Persistencia | `repository.rs` | — | — | SQL de las tablas del módulo, y solo de esas |
| Errores | `errors.rs` | `errors.py` | — | Los fallos que el módulo puede devolver, con nombre |
| Tests | `tests/` del módulo | `tests/<módulo>/` | `*.test.ts` | Fuera de los archivos de código |

Un módulo con uno o dos casos de uso usa `service.rs` / `service.py` en vez
de la carpeta `use_cases/`.

**Topes** (los revisa el CI; superarlos pide dividir, no una excepción):

| Qué | Máximo |
|---|---|
| Líneas por archivo de código | 300 |
| Líneas por función | 50 |
| Métodos por puerto | 5 |
| Dependencias de un constructor | 6 |

Ejemplo — `modules/messages/` del backend:

```
modules/messages/
├── mod.rs                    # pub fn module(ports) -> MessagesModule { router, api pública }
├── controller.rs             # extrae, llama al caso de uso, responde
├── dtos.rs                   # IncomingMessageDto, SendTextDto, extractor MediaUpload…
├── domain.rs                 # Message, MessageStatus, MessageSender
├── window.rs                 # regla de 24 h (pura)
├── media_rules.rs            # tipos y tamaños por medio (pura)
├── errors.rs                 # MessagesError { ChatNotFound, WindowClosed, … }
├── ports.rs                  # WhatsAppSender, ChatDirectory, CustomerDirectory, FailureListener
├── repository.rs             # MessageStore (trait) + PgMessageStore
├── use_cases/
│   ├── get_history.rs
│   ├── register_incoming.rs
│   ├── register_outgoing.rs
│   ├── send_free_text.rs
│   ├── send_free_media.rs
│   ├── send_template.rs      # fase 6
│   ├── update_status.rs
│   └── register_failure.rs
└── tests/
```

---

## 5. Reglas de dependencia

```
                bootstrap                 conoce todo, solo conecta
                    │
     ┌──────────────┼──────────────┐
     ▼              ▼              ▼
  modules ◀── integrations      (los adaptadores implementan
     │              │            puertos de los módulos)
     └──────┬───────┘
            ▼
          shared                   no importa a nadie
```

1. **Un módulo nunca importa otro módulo.** Lo que necesita de otro lo
   declara como puerto en su `ports`, y `bootstrap` conecta ese puerto con
   la entrada pública del módulo que lo provee.
2. **Un módulo nunca importa una integración.** Declara el puerto;
   `integrations/` lo implementa.
3. **El puerto lo declara quien lo usa**, con los métodos que usa y ninguno
   más. Si dos módulos necesitan cosas distintas del mismo sistema, cada uno
   tiene su puerto y un mismo adaptador implementa los dos.
4. **Escribir, solo en tablas propias.** Si un caso de uso debe cambiar
   datos de dos módulos de forma atómica, recibe una transacción
   (`shared/tx`) y llama al puerto del otro módulo dentro de ella.
5. **Leer tablas ajenas, solo en un archivo `read_model.rs`** del módulo que
   consulta, declarado en su `mod.rs`. Hoy lo necesitan dos sitios: el cupo
   (lee mensajes y notificaciones en una sola sentencia) y `video_sent`.
6. **Las variables de entorno solo se leen en `bootstrap/`.**
7. **`shared/` no conoce el negocio.** Si un archivo de `shared/` menciona
   una guía, un chat o una plantilla, está en el sitio equivocado.

**Se verifican solas**, no por revisión:

| Servicio | Herramienta | Qué bloquea |
|---|---|---|
| Bot | `import-linter` (contratos `layers` y `independence`) | módulo → módulo, módulo → integración, `shared` → lo que sea |
| Backend, ai-agents | test de arquitectura en `tests/architecture.rs` que recorre los `use` de cada archivo | lo mismo, más `sqlx::query` fuera de `repository.rs` / `read_model.rs` |
| Frontend | ESLint `no-restricted-imports` por zona | `features/a` → `features/b`, `shared` → `features` |

---

## 6. Estructura objetivo por servicio

### 6.1 Repositorio

```
NotiAsis/
├── bot/                 # hoy en la raíz: app/, tests/, Dockerfile, requirements
├── backend/             # hoy webapp/backend
├── ai-agents/
├── frontend/            # hoy webapp/frontend
├── contracts/           # OpenAPI del backend, esquema del evento a ai-agents,
│   └── fixtures/        # contrato de N8N y casos de prueba compartidos
├── docs/
├── docker-compose.yml
└── .github/workflows/
```

### 6.2 Bot

```
bot/app/
├── main.py                       # crea la app y monta los routers de los módulos
├── bootstrap/
│   ├── settings.py               # Settings validado al arrancar, una sección por módulo
│   ├── container.py              # construye adaptadores y módulos; único composition root
│   └── logging.py
├── shared/
│   ├── phone.py                  # Phone + reglas por país
│   ├── text.py                   # una línea, sin tildes, palabras significativas
│   ├── money.py                  # formato por moneda
│   ├── clock.py                  # reloj inyectable y "hoy" en la zona del business
│   ├── retry.py                  # la única política de reintentos
│   ├── run_guard.py
│   └── errors.py
├── integrations/
│   ├── backend/                  # session.py (token) + un archivo por recurso:
│   │                             # guides.py, messages.py, businesses.py, associates.py, quota.py
│   ├── whatsapp/                 # hasta la fase 6; después lo sustituye backend/messages.py
│   ├── dropi/
│   ├── google_sheets/
│   ├── file_download/
│   ├── deepseek/
│   └── ai_agents/
└── modules/
    ├── webhook/                  # verification, parser, dispatcher, events, handlers/
    ├── associates/               # directorio con refresco
    ├── templates/                # una clase por plantilla + catálogo
    ├── delivery/                 # enviar plantillas y registrarlas: lo que hoy está copiado 3 veces
    ├── escalation/               # política de pasos
    ├── quota/                    # RunQuota
    ├── product_videos/           # matcher, media_cache, library, policy
    ├── notification_run/         # pipeline común de una corrida: planear → cupo → enviar → informe
    ├── guide_pdf/
    ├── sheet_notifications/
    ├── dropi_orders/             # pedidos de Dropi tipados: paginación, detalle, dirección, pago
    ├── dropi_office/
    └── dropi_tracking/
```

Cambios de fondo respecto a hoy:

- **`delivery`** absorbe las tres copias de envío. El desvío a un número de
  pruebas y la copia de depuración dejan de ser `if` dentro de cada flujo:
  son decoradores del puerto `MessageSender` que `bootstrap` aplica según la
  configuración.
- **`notification_run`** sustituye la herencia de `DropiOrderNotifier`. Una
  corrida es un pipeline de pasos (el mismo concepto que `pipeline/` de
  ai-agents): traer candidatos, planear, reservar cupo, y por cada uno
  resolver datos, armar plantillas, entregar, registrar. Sheet, oficina y
  seguimiento aportan sus pasos propios y reutilizan los comunes.
- **Pedidos y guías con tipo** (`DropiOrder`, `GuideSnapshot`, `OrderDetails`)
  en vez de `dict`. La conversión desde JSON pasa en un solo sitio, en el
  adaptador.
- **El puerto `NotificationBackend` de 14 métodos desaparece.** Cada módulo
  declara el suyo: `guide_pdf` necesita `GuideRegistry` (3 métodos);
  `webhook` necesita `MessageLog` (4).

### 6.3 Backend

```
backend/
├── migrations/                   # versionadas; sustituyen a db.sql
└── src/
    ├── main.rs                   # config → app → serve, con apagado ordenado
    ├── bootstrap/
    │   ├── config.rs             # Settings tipado; falla nombrando la variable
    │   ├── app.rs                # une los módulos y aplica las capas
    │   ├── wiring.rs             # adaptadores que conectan puertos entre módulos
    │   └── observability.rs      # tracing
    ├── shared/
    │   ├── error.rs              # AppError + IntoResponse: el único sitio que decide el código HTTP
    │   ├── response.rs           # un solo sobre de respuesta
    │   ├── validation.rs         # ValidatedJson<T>
    │   ├── phone.rs              # Phone
    │   ├── ids.rs                # BusinessId, GuideNumber, MetaMessageId
    │   ├── clock.rs
    │   ├── pagination.rs
    │   └── tx.rs                 # transacción compartida entre repositorios
    ├── integrations/
    │   ├── whatsapp/             # el único cliente de Meta del sistema
    │   └── ffmpeg/               # notas de voz
    └── modules/
        ├── auth/                 # login, tokens, sesión, extractores de autorización
        ├── businesses/           # empresas y países
        ├── associates/           # alta, listado, contraseña
        ├── sheet_configs/
        ├── customers/            # hoy users
        ├── chats/                # bandeja, importancia, enrutar al business
        ├── messages/             # historial, entrantes, salientes, envío libre, estados, fallos
        ├── media/                # túnel de descarga y subida
        ├── guides/               # registro, pedido, lookup
        ├── notifications/        # libro de notificaciones, deshacer, estadísticas
        └── quota/                # asignación (pura), reservas, estado
```

Cambios de fondo:

- **Autorización por tipo, no por llamada.** Un handler declara lo que exige
  en su firma: `ServiceCaller`, `AssociateSession` o `BusinessAccess`. Si no
  pide ninguno, no compila contra la ruta protegida. Desaparecen las 32
  llamadas manuales.
- **`AppError`** con variantes y un único `IntoResponse`. Los errores
  internos se registran completos y al cliente le llega un código estable y
  un mensaje. Desaparecen `Result<_, String>` y las comparaciones de texto.
- **`notifications`** nace como módulo: hoy su lógica está repartida entre
  `guides/repository.rs`, `messages/repository.rs` y `tools/message_lock.rs`.
- **Casos de uso transaccionales.** Registrar un entrante, registrar una
  guía y registrar un fallo abren una transacción y la pasan a los puertos.
- **Los ocho `*State` desaparecen**: cada módulo expone su router ya
  construido.

### 6.4 ai-agents

Se queda como está, con cuatro ajustes de frontera (fase 7):

| Ajuste | Motivo |
|---|---|
| Los repositorios leen vistas `agent_api.*` en vez de tablas del backend | El backend puede cambiar sus tablas sin romper al agente (AG-1) |
| `business_ai_agents`, `ai_escalations` y `guide_customer_updates` pasan a `ai-agents/migrations/` | Son suyas (AG-2) |
| Marcar un chat como importante y guardar una transcripción se hacen por la API del backend | Una sola puerta para escribir datos del backend (AG-3) |
| Los traits `MediaDownloader`, `AiMessageSender` y `OrderLookup` se mudan de `integrations/` a `agents/customer_service/ports.rs` | Regla 3: el puerto lo declara quien lo usa |

### 6.5 Frontend

```
frontend/src/
├── app/                          # rutas: solo componen features
├── shared/
│   ├── http.ts                   # el único fetch: token, 401, errores
│   ├── api-types.ts              # generado desde contracts/openapi.json
│   ├── time.ts
│   └── ui/
└── features/
    ├── auth/                     # LoginForm, session.ts, useSessionGuard.ts
    ├── inbox/                    # useInbox, filters.ts, ChatList, ChatListItem
    ├── conversation/             # useMessages, MessageBubble, media/, window.ts
    ├── composer/                 # texto, attachments/, voice/useVoiceRecorder.ts
    └── dashboard/                # useStats, series.ts (puro), components/
```

Cada feature separa tres cosas: **hooks** (datos y estado), **funciones
puras** (filtros, series, validaciones, con tests) y **componentes** (solo
presentación).

---

## 7. Convenciones transversales

| Tema | Regla única | Modelo existente |
|---|---|---|
| **Errores** | Tipados por módulo. Una consulta devuelve "no existe" como valor (`None` / `Option`); todo lo demás es un error con nombre. Quién tolera un fallo y quién se detiene lo decide el caso de uso, en un solo sitio, nunca el adaptador | `ai-agents/…/ports.rs` |
| **Logs** | Estructurados, nombre del evento en inglés y campos (`business_id`, `guide`, `meta_message_id`, `run_id`). Teléfonos enmascarados. Ni `print` ni `println!` | `ai-agents/src/domain/chat.rs` |
| **Configuración** | Tipada, validada al arrancar, una sección por módulo. Lo obligatorio no tiene valor por defecto | `ai-agents/src/config/` |
| **Tiempo** | Se guarda en UTC con zona (`TIMESTAMPTZ`). "Hoy" siempre es el día del business. El reloj se inyecta | — |
| **Tipos de valor** | `Phone`, `GuideNumber`, `MetaMessageId`, `Money`, `BusinessId`: se validan al construirse y no viajan como texto suelto | `ChatKey` de ai-agents |
| **Reintentos** | Una política por servicio en `shared/retry`. Nunca se reintenta un envío a un cliente | `ai-agents/src/shared/retry.rs` |
| **Idioma** | Código, logs y nombres de eventos en inglés. Comentarios, documentación y textos que ve una persona en español | — |
| **Nombres** | Módulos: sustantivos del negocio. Casos de uso: verbo + objeto (`register_incoming`). Puertos: lo que proveen (`GuideRegistry`), no quién los implementa (`BackendClient`) | — |
| **Listas** | Todas paginadas o con "desde" | — |
| **Tests** | Dominio puro sin dobles → casos de uso con fakes de sus puertos → adaptadores contra el sistema real o un servidor falso → contrato entre servicios. Un fake por puerto, compartido en `tests/support` | `ai-agents/tests/` |

---

## 8. SOLID y patrones: dónde se aplica cada uno

### SOLID

| Principio | Violación más clara hoy | Cómo queda |
|---|---|---|
| **S** — una razón para cambiar | `messages/service.rs`: regla de 24 h, validación, ffmpeg y siete casos de uso | Un archivo por caso de uso, dominio aparte, ffmpeg en `integrations/` |
| **O** — abierto a extender | Agregar un nivel de seguimiento obliga a editar `_template` y `TRACKING_LEVELS` | Tabla de niveles: nivel → estados → constructor de plantillas. Una fila nueva |
| **L** — sustituible | Plantillas con constructores distintos "para mantener la firma" que no usan | Cada plantilla recibe lo que usa; el catálogo las construye |
| **I** — interfaces pequeñas | `NotificationBackend` (14 métodos), `MessageServiceTrait` (11) | Puertos de 1 a 5 métodos, declarados por quien los usa |
| **D** — depender de abstracciones | `ShippingNotificationService` recibe la clase `AssociateDirectory`; `chats` importa una función de `messages` | Puertos, conectados en `bootstrap` |

### Patrones

| Patrón | Dónde | Sustituye a |
|---|---|---|
| **Puertos y adaptadores** | Todas las integraciones de los tres servicios | Clientes HTTP dentro de `services/` |
| **Composition root** | `bootstrap/` de cada servicio | `lru_cache`, ocho `*State`, `Depends` que construyen |
| **Pipeline / Step** | Corridas de notificación del bot (ya existe en ai-agents) | Herencia de `DropiOrderNotifier` y dos `_notify` casi iguales |
| **Strategy** | Escalación (con tope / diaria), formato de dinero por moneda, teléfono por país, plantillas por paso o nivel | `if` por tipo repartidos |
| **Decorator** | Desvío a número de pruebas y copia de depuración sobre `MessageSender`; reintentos y logs sobre los clientes HTTP | Lógica copiada en cada flujo |
| **Observer / eventos de dominio** | "Mensaje de cliente registrado" → ai-agents (ya existe en el bot); "mensaje falló" → `notifications` deshace | `messages` escribiendo tablas de `guides` |
| **Repository** | Un repositorio por módulo del backend, dueño de sus tablas | Repositorios que cruzan módulos |
| **Unit of Work** | `shared/tx` en el backend | Varias escrituras sueltas por caso de uso |
| **Value Object** | `Phone`, `GuideNumber`, `Money`… | `String` y `dict` |
| **Registry** | Catálogo de plantillas, tabla de niveles de seguimiento | Constantes dispersas |
| **Specification / cadena de reglas** | Reglas de "¿se notifica?" del bot (ya existe como `ReplyRule` en ai-agents) | `if` encadenados en `_plan` y `_needs_notification` |
| **Extractor tipado** | Autorización y validación en el backend | Llamadas manuales al inicio de cada handler |

---

## 9. Contratos entre servicios

Todo lo que cruza de un servicio a otro queda escrito en `contracts/` y se
comprueba en CI.

| Contrato | Formato | Quién lo genera | Quién lo comprueba |
|---|---|---|---|
| API del backend | `openapi.json` | el backend, desde sus DTOs (`utoipa`) | bot y ai-agents: sus clientes se prueban contra él; frontend: sus tipos se generan de él |
| Evento bot → ai-agents | JSON Schema | ai-agents | test del bot |
| Webhooks de N8N (`/ordenes`, `/orden`) | JSON Schema + ejemplos reales | escrito a mano una vez | tests del bot y de ai-agents |
| Vistas `agent_api.*` | SQL en `backend/migrations/` | backend | `ai-agents/tests/postgres_repositories.rs` |
| Reglas duplicadas por lenguaje (teléfonos, dinero, emparejar productos, pasos de notificación) | `fixtures/*.json`: entrada → salida | escrito a mano | tests de cada servicio que las implementa |

Un cambio de contrato es un PR que toca `contracts/` y que no se puede
mezclar si algún consumidor queda en rojo.

---

## 10. Decisiones que necesito de ti

El plan está escrito con la opción recomendada. Si eliges otra, cambia la
fase indicada y nada más.

| # | Decisión | Recomendación | Alternativa | Afecta a |
|---|---|---|---|---|
| D1 | ¿Quién habla con Meta? | **Solo el backend.** El bot le pide enviar; enviar y guardar pasan juntos. Quita el token de WhatsApp del bot y la carrera que hoy cubre el bloqueo por mensaje | Dejar los dos clientes y solo alinearlos. Menos trabajo, y el problema que señalaste sigue ahí | Fase 6 |
| D2 | ¿Quién recibe el webhook? | **El bot, por ahora.** Es el único servicio que hoy necesita estar expuesto a internet además del frontend | Moverlo al backend cuando termine la fase 6: el bot deja de ser un relevo, pero el backend pasa a estar expuesto y a tener que avisar al bot de los PDF | Fase opcional tras la 6 |
| D3 | ¿Cómo lee ai-agents la base? | **Vistas de contrato + usuario de Postgres propio + escrituras por la API** | Todo por API (más limpio, mucho más trabajo y más latencia por turno) o dejarlo como está | Fase 7 |
| D4 | ¿Mover el bot a `bot/` y sacar backend y frontend de `webapp/`? | **Sí**, en la fase 1. Son solo movimientos, y arreglan el contexto de build del bot | Dejar las carpetas como están | Fase 1 |
| D5 | ¿Migraciones versionadas? | **Sí**, `sqlx migrate`, partiendo del esquema actual como versión inicial | Seguir con `db.sql` idempotente | Fase 4 |
| D6 | ¿Renombrar `users` a `customers`? | **Solo en el código** (módulo y tipos). La tabla se queda | Renombrar también la tabla (migración y ventana de mantenimiento) | Fase 5 |
| D7 | ¿Se congela el desarrollo de funcionalidades durante la refactorización? | **No congelar, pero lo nuevo entra con la estructura nueva.** Las fases del bot y del backend son independientes: se puede seguir entregando en el servicio que no se está moviendo | Congelar hasta terminar | Todo el plan |
