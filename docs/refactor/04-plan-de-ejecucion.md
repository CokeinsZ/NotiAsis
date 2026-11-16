# 04 · Plan de ejecución

Cómo llegar de [lo que hay](01-diagnostico.md) a la
[arquitectura objetivo](03-arquitectura-objetivo.md) sin dejar de tener un
sistema que funciona. Once fases; cada paso es un PR pequeño que se puede
revisar, verificar y revertir solo.

- [Reglas](#reglas)
- [Orden y dependencias](#orden-y-dependencias)
- [Fase 0 · Red de seguridad](#fase-0--red-de-seguridad)
- [Fase 1 · Esqueleto](#fase-1--esqueleto)
- [Fase 2 · Bot: base](#fase-2--bot-base)
- [Fase 3 · Bot: módulos](#fase-3--bot-módulos)
- [Fase 4 · Backend: base](#fase-4--backend-base)
- [Fase 5 · Backend: módulos](#fase-5--backend-módulos)
- [Fase 6 · Un solo canal de WhatsApp](#fase-6--un-solo-canal-de-whatsapp)
- [Fase 7 · ai-agents: fronteras](#fase-7--ai-agents-fronteras)
- [Fase 8 · Contratos](#fase-8--contratos)
- [Fase 9 · Frontend](#fase-9--frontend)
- [Fase 10 · Cierre](#fase-10--cierre)
- [Riesgos](#riesgos)

---

## Reglas

Valen para todos los pasos. Si un paso no puede cumplirlas, se parte en dos.

1. **Mover o cambiar, nunca las dos cosas en el mismo commit.** Un commit
   que mueve código no cambia su comportamiento; uno que lo cambia no lo
   mueve. Los prefijos lo dicen: `move:`, `refactor:`, `fix:`, `test:`.
2. **Los tests existentes son el contrato.** Los 434 tests pasan antes y
   después de cada paso. Un test solo se modifica para cambiarle un import
   o un fake, nunca una aserción. Si hay que tocar una aserción, el paso
   cambió el comportamiento: va con `fix:` y en su propio PR.
3. **Primero la red, después el salto.** Antes de reorganizar una zona sin
   tests (todo el SQL del backend) se escriben tests que describen lo que
   hace hoy, aunque lo que haga hoy esté mal.
4. **Estrangular, no reemplazar.** La estructura nueva se crea al lado; el
   sitio viejo queda como un reexport de una línea hasta que nadie lo
   importa. Así cada paso deja el sistema desplegable.
5. **Los defectos (⚠ del inventario) se corrigen después de mover**, cada
   uno con su test, cuando el código ya está en su módulo.
6. **Los contratos entre servicios no cambian hasta la fase 6.** Ni rutas,
   ni campos, ni códigos HTTP. Las excepciones están marcadas como
   *cambio visible* y van solas.
7. **Las reglas de arquitectura se activan con trinquete.** Desde la fase 1
   se registran las violaciones existentes; el CI solo falla si aparece
   una nueva. La lista baja hasta cero y entonces la regla pasa a estricta.
8. **Cada fase termina desplegada en el entorno de pruebas**, con su lista
   de comprobación. No se empieza la siguiente con la anterior a medias.

**Verificación mínima de cualquier paso**

```bash
cd bot && pytest -q                                   # 178
cd backend && cargo test                              # 145 + los de la fase 0
cd ai-agents && TEST_DATABASE_URL=... cargo test      # 111
cd frontend && npm run lint && npx tsc --noEmit && npm run build
```

---

## Orden y dependencias

```
0 Red ──▶ 1 Esqueleto ──┬──▶ 2 Bot: base ─────▶ 3 Bot: módulos ─────┐
                        │                                           ├──▶ 6 WhatsApp único ──▶ 7 ai-agents
                        └──▶ 4 Backend: base ──▶ 5 Backend: módulos ┘                              │
                                                                                                   ▼
                                                              10 Cierre ◀── 9 Frontend ◀── 8 Contratos
```

- Las fases 2-3 (bot) y 4-5 (backend) no se tocan entre sí mientras la API
  no cambie: se pueden alternar o hacer en paralelo.
- Si hay que entregar una funcionalidad a mitad de camino, entra en el
  servicio que no se está moviendo, o en la estructura nueva del que sí.
- El orden recomendado para una sola persona es 0 → 1 → 2 → 3 → 4 → 5 → 6…:
  el bot primero porque es donde más duele, tiene los mejores tests y da
  el primer resultado visible (una sola copia de la lógica de envío).

| Fase | Pasos | Riesgo | Qué se gana al terminarla |
|---|---:|---|---|
| 0 | 6 | bajo | Nada se rompe sin que el CI lo diga |
| 1 | 7 | bajo | Las carpetas ya dicen qué es cada cosa |
| 2 | 8 | bajo | El bot tiene configuración, logs y clientes ordenados |
| 3 | 16 | medio | Un módulo por caso de uso; desaparece el código copiado |
| 4 | 11 | medio | Errores tipados, autorización por tipo, migraciones |
| 5 | 13 | **alto** (5.9) | Cada módulo es dueño de sus tablas |
| 6 | 8 | **alto** | Un solo servicio habla con Meta |
| 7 | 6 | medio | ai-agents deja de depender del esquema del backend |
| 8 | 5 | bajo | Los contratos están escritos y el CI los vigila |
| 9 | 9 | bajo | Frontend por funcionalidades, con tipos generados |
| 10 | 4 | bajo | Sin restos: ni reexports, ni banderas, ni código muerto |

---

## Fase 0 · Red de seguridad

**Objetivo.** Que cualquier error de los pasos siguientes lo detecte una
máquina. No se mueve código de producción.

- [ ] **0.1 Integración continua.** Un workflow con cuatro trabajos: bot
      (`pytest`), backend (`cargo test`), ai-agents (`cargo test` con un
      Postgres de servicio, como ya describe su README) y frontend (`lint`,
      `tsc --noEmit`, `build`). Obligatorio para mezclar a `refactor` y a `main`.
- [ ] **0.2 Versiones fijadas en el bot.** `requirements.txt` con versiones
      exactas (generado con `pip-compile` o `uv`), y `PyPDF2` → `pypdf`
      solo si los tests del flujo PDF pasan sin tocarlos.
- [ ] **0.3 Formato y lint, un commit por servicio que no hace otra cosa.**
      `ruff format` + `ruff check` en el bot; el `rustfmt.toml` de ai-agents
      copiado al backend y `cargo fmt`; `cargo clippy` en modo aviso. Así
      los diffs de las fases siguientes solo muestran cambios reales.
- [ ] **0.4 Tests del SQL del backend contra Postgres.** Hoy ningún
      repositorio tiene pruebas. Con el mismo mecanismo de ai-agents
      (`TEST_DATABASE_URL`, esquema aplicado una vez), cubrir como mínimo:

  | Zona | Casos |
  |---|---|
  | Cupo | clasificación de cada teléfono; oficina usa todo y seguimiento respeta su tope; dos reservas simultáneas no aprueban el mismo cupo; liberar dos veces; reserva vencida |
  | Fallo de entrega | deshace una notificación de oficina y una de seguimiento; no deshace con error permanente; repetir el aviso no cambia nada |
  | Orden por mensaje | el fallo llega antes que el registro (nace `failed`); el fallo llega antes que la marca (no cuenta) |
  | Mensajes | mismo id de Meta dos veces devuelve el primero; estados que no retroceden de `read` |
  | Chats | business del chat con actividad más reciente; orden de la bandeja |
  | Guías | `created` verdadero/falso; detalles que conservan lo anterior; `video_sent`; estadísticas por día |

- [ ] **0.5 Foto de la API.** Tests de router que guardan, por endpoint,
      la forma exacta de la respuesta y los códigos (snapshots). Es lo que
      permite cambiar por dentro los controladores en las fases 4 y 5 con
      la certeza de que el bot, ai-agents y el frontend no notan nada.
- [ ] **0.6 Prueba de humo de extremo a extremo** con `docker compose`, un
      Postgres local y un servidor falso que hace de Meta y de N8N. Dos
      escenarios: (a) llega un mensaje de cliente por el webhook y queda
      en el historial; (b) una corrida de Dropi con dos pedidos envía,
      registra y marca. Se corre a mano al cerrar cada fase.

**Hecho cuando:** el CI está en verde en `refactor`, un PR con un test roto
no se puede mezclar y la prueba de humo pasa en local.

---

## Fase 1 · Esqueleto

**Objetivo.** Dejar las carpetas y las herramientas de la arquitectura
objetivo, moviendo archivos sin cambiar una línea de lógica.

- [ ] **1.1 Carpetas del repositorio** (decisión D4). `git mv` de `app/`,
      `tests/`, `Dockerfile` y `requirements*.txt` a `bot/`; `webapp/backend`
      a `backend/`; `webapp/frontend` a `frontend/`. Se actualizan
      `docker-compose.yml`, el CI y la ruta de `ai-agents/tests/common/mod.rs:15`.
      `bot/.dockerignore` nuevo.
- [ ] **1.2 `contracts/` y documentación por servicio.** Un `README.md` en
      cada servicio con las secciones que hoy están en el de la raíz,
      movidas tal cual. La reescritura es de la fase 10.
- [ ] **1.3 Bot: paquetes vacíos** `bootstrap/`, `shared/`, `integrations/`,
      `modules/` y `import-linter` configurado con trinquete (regla 7).
- [ ] **1.4 Backend: nombre y carpetas.** Crate `mania-back-end` →
      `notiasis-backend`; `tools/` → `shared/`; los siete módulos actuales
      pasan a `src/modules/`. Solo cambian rutas de `use`.
- [ ] **1.5 Backend y ai-agents: test de arquitectura** con su lista
      inicial de violaciones conocidas.
- [ ] **1.6 Frontend: carpetas** `features/` y `shared/`, y zonas de ESLint
      en modo aviso.
- [ ] **1.7 `CONTRIBUTING.md`** con la anatomía de un módulo, las reglas de
      dependencia y los prefijos de commit (resumen de la arquitectura
      objetivo, una página).

**Hecho cuando:** los cuatro servicios construyen y pasan sus tests desde
sus carpetas nuevas, y `docker compose up --build` levanta los cuatro.

---

## Fase 2 · Bot: base

**Objetivo.** Ordenar lo que no es negocio: configuración, logs, utilidades
y clientes de sistemas externos. Los cuatro flujos siguen como están.

- [ ] **2.1 Logging.** Los 87 `print` pasan a un logger estructurado con
      nombre de evento y campos. Teléfonos enmascarados.
- [ ] **2.2 Configuración.** `bootstrap/settings.py` validado al arrancar,
      con los mismos nombres de variables. *Cambio visible:* si falta una
      variable obligatoria, el bot no arranca y dice cuál (hoy arranca y
      falla en el primer envío).
- [ ] **2.3 `shared/`.** `phone`, `text`, `money`, `clock`, `retry`,
      `run_guard`, `errors`, extraídos de `core/phones.py`, `dropi_orders.py`
      y `product_videos.py`. Los archivos de origen los reexportan. Los
      casos de prueba de teléfonos y dinero van a `contracts/fixtures/`.
- [ ] **2.4 Cliente del backend por recurso.** `integrations/backend/` con
      `session.py` (token, con lock) y un archivo por recurso.
      `RustBackendClient` queda como fachada que delega, para que ningún
      flujo cambie todavía.
- [ ] **2.5 Resto de integraciones** a su carpeta: `whatsapp`, `dropi`,
      `google_sheets`, `file_download`, `deepseek`, `ai_agents`. Todos usan
      `shared/retry`.
- [ ] **2.6 Modelos tipados en el borde.** `GuideSnapshot`, `BusinessCountry`,
      `DropiOrder`, `OrderDetails` y sus conversores desde JSON, con tests.
      Todavía no los usa nadie.
- [ ] **2.7 Composition root.** `bootstrap/container.py` sustituye a
      `api/dependencies.py`: una instancia por cliente, `lifespan` que carga
      el directorio de asociados al arrancar.
- [ ] **2.8 Fakes compartidos.** `tests/support/` con un fake por puerto;
      los seis archivos de tests dejan de definir el suyo.

**Hecho cuando:** `app/services/` solo contiene los cinco orquestadores y
reexports, y los 178 tests pasan sin cambiar una aserción.

---

## Fase 3 · Bot: módulos

**Objetivo.** Un módulo por caso de uso. Se empieza por las hojas (lo que
no depende de nada) y se termina por los flujos.

- [ ] **3.1 `templates`.** Las ocho plantillas y un catálogo. Cada
      constructor recibe solo lo que usa.
- [ ] **3.2 `escalation`.** Las dos políticas como estrategias sobre una
      única regla "una por día". Reloj inyectado (sigue en UTC: la zona se
      corrige en 3.16).
- [ ] **3.3 `quota`.** `RunQuota` con su puerto `QuotaLedger` (2 métodos).
- [ ] **3.4 `product_videos`.** `matcher` (puro), `media_cache`, `library`
      y `policy` ("una vez por guía").
- [ ] **3.5 `associates`.** Directorio con su puerto `AssociateSource`.
- [ ] **3.6 `delivery`.** `TemplateDispatcher`: enviar en orden y registrar.
      El desvío de pruebas y la copia de depuración, como decoradores de
      `MessageSender`. Tres PR, uno por copia que se elimina: flujo PDF,
      flujo Sheet, flujos Dropi. **La prueba de que está bien hecho es que
      los tests de cada flujo pasan sin tocarlos.**
- [ ] **3.7 `webhook`.** `parser` puro que convierte el payload de Meta en
      eventos tipados (`StatusUpdate`, `DeliveryFailure`, `CustomerMessage`,
      `AssociateDocument`), un handler por evento y el `dispatcher`.
- [ ] **3.8 `guide_pdf`.** Con sus puertos: `MediaDownloader`, `PdfText`,
      `RecipientExtractor`, `GuideRegistry`.
- [ ] **3.9 `notification_run`.** El pipeline de una corrida y sus pasos
      comunes: reservar cupo, entregar, registrar, informar.
- [ ] **3.10 `sheet_notifications`** sobre el pipeline: `pending_guides`
      (anti-join, puro), `row_mapper`, `planner`, `report`.
- [ ] **3.11 `dropi_orders`.** Con los modelos de 2.6: `order_pager`,
      `order_details`, `customer_phone`, `address`, `payment`, `ownership`.
      Funciones puras con los tests que hoy están mezclados en
      `test_dropi_notifier.py`.
- [ ] **3.12 `dropi_office`** sobre el pipeline.
- [ ] **3.13 `dropi_tracking`** sobre el pipeline, con la tabla de niveles
      (nivel → estados → plantillas).
- [ ] **3.14 Borrar lo viejo.** `app/services/`, `app/core/`, `app/models/`,
      `app/api/`, `app/whatsapp/`. `import-linter` en modo estricto.
- [ ] **3.15 Tests por módulo.** `tests/<módulo>/`, mismos tests, mismas
      aserciones.
- [ ] **3.16 Defectos**, uno por PR, cada uno con su test:

  | Inventario | Corrección |
  |---|---|
  | BOT-072 | Control de corridas y verificación del business en `/notify-sheet` |
  | BOT-024 | Refresco del directorio de asociados con caducidad |
  | BOT-054 | "Hoy" en la zona horaria del business |
  | BOT-03A | Extracción del PDF con el país del business, sin `+57` fijo |
  | n.º 6 del documento de errores | Guía del sheet registrada sin notificar: reintentar en vez de omitir |
  | 1.1 `product_videos` | Subir el video fuera del lock |

**Hecho cuando:** `bot/app/` tiene exactamente `main.py`, `bootstrap/`,
`shared/`, `integrations/` y `modules/`; ningún archivo pasa de 300 líneas;
`import-linter` no tiene excepciones; la prueba de humo pasa.

---

## Fase 4 · Backend: base

**Objetivo.** Poner los cimientos que hoy faltan, sin partir todavía los
módulos.

- [ ] **4.1 Configuración.** `bootstrap/config.rs` tipado (el patrón de
      `ai-agents/src/config/`). Todas las lecturas de entorno salen de
      `main.rs` y de `messages/service.rs`.
- [ ] **4.2 Observabilidad y arranque.** `tracing` con un id por petición,
      apagado ordenado, `GET /health`.
- [ ] **4.3 Errores tipados.** `shared/error.rs` (`AppError` + `IntoResponse`)
      y un enum de errores por módulo. Los servicios dejan de devolver
      `String`, módulo por módulo. Las fotos de 0.5 garantizan que los
      códigos y cuerpos no cambian. *Cambio visible*, en su PR: los fallos
      internos dejan de responder 400 con el texto del error y pasan a 500
      con un mensaje genérico. Antes de mezclarlo, confirmar que el bot
      trata igual un 400 y un 500 en cada llamada (lo hace: ambos acaban en
      "no notificar").
- [ ] **4.4 Sobre de respuesta.** `shared/response.rs`; cada endpoint
      conserva sus claves actuales.
- [ ] **4.5 Autorización por tipo.** `ClaimsKind` como enum y los
      extractores `ServiceCaller`, `AssociateSession` y `BusinessAccess`.
      Se sustituyen las 32 llamadas manuales, controlador por controlador.
      Los tests de 403 existentes son la prueba.
- [ ] **4.6 Tipos de valor.** `shared/phone.rs` (con los casos de
      `contracts/fixtures/`), `shared/ids.rs`.
- [ ] **4.7 Transacciones.** `shared/tx.rs`: los repositorios aceptan una
      transacción abierta.
- [ ] **4.8 Integraciones.** `meta_client.rs` → `integrations/whatsapp/`
      (con timeouts y errores tipados); el transcodificador →
      `integrations/ffmpeg/`.
- [ ] **4.9 Fuera los `*State`.** Cada módulo construye su router.
- [ ] **4.10 Migraciones** (decisión D5). `migrations/0001_baseline.sql`
      con el esquema actual; las correcciones de datos de `db.sql`, en
      migraciones aparte. Para las bases que ya existen, un procedimiento
      escrito y probado en el entorno de pruebas que marca la base como
      "ya en la versión inicial".
- [ ] **4.11 Tests fuera de los archivos.** A `tests/` de cada módulo, con
      un solo `FakeAuthService`.

**Hecho cuando:** no queda ningún `Result<_, String>`, ningún `println!`,
ninguna llamada a `require_bot` ni `authorize_business`, y las fotos de la
API siguen iguales salvo el cambio visible de 4.3.

---

## Fase 5 · Backend: módulos

**Objetivo.** Que cada módulo sea dueño de sus tablas y de sus casos de
uso. Orden: primero los módulos de los que dependen los demás.

- [ ] **5.1 `customers`** (hoy `users`; decisión D6).
- [ ] **5.2 `businesses`** con países.
- [ ] **5.3 `associates`.** Alta y listado (hoy en `businesses`) y cambio de
      contraseña (hoy en `auth`). Un solo repositorio para
      `business_associates`; `auth` lo usa por el puerto `CredentialStore`.
- [ ] **5.4 `sheet_configs`.**
- [ ] **5.5 `auth`.** Casos de uso de login, `tokens.rs`, y la renovación
      de sesión como capa de respuesta (el campo `renewed_token` no cambia).
- [ ] **5.6 `chats`.** Bandeja, importancia, abrir chat, registrar
      actividad, enrutar al business.
- [ ] **5.7 `messages`.** Un caso de uso por archivo. `register_incoming`
      pasa a ser una transacción. El parseo del multipart, a un extractor.
- [ ] **5.8 `media`.** El túnel de descarga, fuera de `messages`.
- [ ] **5.9 `notifications`.** El paso más delicado del plan: el libro de
      notificaciones sale de `guides/repository.rs`, el deshacer sale de
      `messages/repository.rs` y el bloqueo por mensaje sale de `shared/`.
      `messages` avisa del fallo por el puerto `FailureListener`, dentro de
      la misma transacción y con el mismo orden de bloqueos que hoy. Solo
      se empieza con todos los tests de 0.4 en verde, y no se toca ninguno
      de ellos en este paso.
- [ ] **5.10 `guides`.** Registro, datos del pedido, lookup. `video_sent`
      por el puerto `VideoHistory`.
- [ ] **5.11 `quota`.** `allocation.rs` (dominio puro, con sus tests
      actuales), `read_model.rs`, casos de uso.
- [ ] **5.12 Arquitectura estricta.** El test de arquitectura sin lista de
      excepciones.
- [ ] **5.13 Defectos**, uno por PR:

  | Inventario | Corrección |
  |---|---|
  | BE-056 | La notificación solo toca el chat de su business |
  | BE-074 | El estado nunca retrocede |
  | BE-097 | Estadísticas por el día del business |
  | BE-9 (diagnóstico) | Paginación opcional en bandeja e historial, sin romper a quien no la pide |

**Hecho cuando:** ningún `repository.rs` nombra una tabla de otro módulo,
ningún trait tiene más de 5 métodos, ningún archivo pasa de 300 líneas y
la prueba de humo pasa.

---

## Fase 6 · Un solo canal de WhatsApp

**Objetivo.** Que solo el backend hable con Meta (decisión D1). Es la
primera fase que cambia un contrato entre servicios, y se hace con
bandera para poder volver atrás en segundos.

- [ ] **6.1 Contrato**, en `contracts/` antes que en código:

  | Endpoint | Entrada | Hace |
  |---|---|---|
  | `POST /messages/templates` | business, teléfono del cliente, teléfono de entrega (opcional, para el desvío de pruebas), plantilla ya armada (nombre, idioma, componentes), texto para el historial, tipo y media, guía | Envía por Meta y guarda el mensaje en una sola operación. Devuelve el mensaje, o el rechazo de Meta con su código |
  | `POST /media` | archivo | Lo sube a Meta y devuelve su `media_id` |
  | `GET /messages/media/{id}` | — | Ya existe: descarga |

- [ ] **6.2 Backend:** caso de uso `send_template` sobre
      `integrations/whatsapp`, y `media/use_cases/upload.rs`.
- [ ] **6.3 Bot:** adaptadores `BackendTemplateSender` y `BackendMediaStore`
      que implementan los puertos de `delivery`, `guide_pdf`,
      `sheet_notifications` y `product_videos`. Variable `WHATSAPP_VIA`
      con valores `direct` (por defecto) y `backend`.
- [ ] **6.4 Entorno de pruebas con `backend`.** Cada flujo contra el
      business de pruebas, comparando las filas de `messages` y
      `guide_notifications` con las de una corrida en `direct`.
- [ ] **6.5 Producción, un flujo cada vez:** guía PDF → Sheet →
      seguimiento → oficina. Entre uno y otro, un día de observación.
      Volver atrás es cambiar la variable.
- [ ] **6.6 Retirar el camino viejo.** `integrations/whatsapp` del bot,
      `WHATSAPP_TOKEN` y `WHATSAPP_PHONE_ID` de su entorno, la bandera.
- [ ] **6.7 Simplificar lo que ya no hace falta.** Con el envío y el
      registro juntos, el aviso de fallo de Meta ya no puede adelantarse
      al registro del mensaje. Se mide durante dos semanas cuántas veces se
      usa el camino "nace `failed`"; si es cero, se retira en su propio PR.
- [ ] **6.8 Opcional (decisión D2):** mover la recepción del webhook al
      backend. Solo si después de 6.6 el relevo del bot sigue molestando.

**Hecho cuando:** `grep -r "graph.facebook.com"` solo encuentra resultados
en `backend/src/integrations/whatsapp/`.

---

## Fase 7 · ai-agents: fronteras

**Objetivo.** Que el agente no dependa de cómo guarda sus datos el backend
(decisión D3).

- [ ] **7.1 Puertos a su sitio.** `MediaDownloader`, `AiMessageSender` y
      `OrderLookup` pasan a `agents/customer_service/ports.rs`.
- [ ] **7.2 Vistas de contrato.** Una migración del backend crea el
      esquema `agent_api` con una vista por necesidad de lectura del
      agente. Los repositorios del agente cambian tablas por vistas.
      `tests/postgres_repositories.rs` es la prueba.
- [ ] **7.3 Tablas propias.** `business_ai_agents`, `ai_escalations` y
      `guide_customer_updates` pasan a `ai-agents/migrations/`.
- [ ] **7.4 Escrituras por la API.** Guardar una transcripción (endpoint
      nuevo en `messages`) y marcar un chat como importante (ya existe).
- [ ] **7.5 Reglas compartidas, probadas igual.** Los tests del agente leen
      los mismos `contracts/fixtures/` que el bot y el backend: teléfonos,
      dinero y emparejamiento de nombres de producto.
- [ ] **7.6 Contrato de N8N** escrito una vez y comprobado por los dos
      clientes (bot y agente).

**Hecho cuando:** el agente arranca y pasa sus tests con un usuario de base
de datos que solo puede leer `agent_api` y escribir sus tres tablas.

---

## Fase 8 · Contratos

**Objetivo.** Que lo que cruza entre servicios esté escrito y vigilado.

- [ ] **8.1 OpenAPI del backend** generado desde sus DTOs y guardado en
      `contracts/openapi.json`. El CI falla si el código y el archivo no
      coinciden.
- [ ] **8.2 Cliente del bot comprobado contra el OpenAPI** (sus modelos de
      2.6 se validan con los esquemas).
- [ ] **8.3 Cliente del agente comprobado** igual.
- [ ] **8.4 Esquema del evento bot → ai-agents.**
- [ ] **8.5 Tipos del frontend generados** en `shared/api-types.ts`
      (sustituyen a `lib/types.ts`).

**Hecho cuando:** cambiar un campo de un DTO del backend sin actualizar el
contrato deja el CI en rojo en el servicio que lo consume.

---

## Fase 9 · Frontend

**Objetivo.** Organizar por funcionalidad y separar datos, reglas y
presentación.

- [ ] **9.1 `shared/http.ts`.** Una sola función de petición; el login, la
      multimedia y el envío con archivo dejan de reimplementarla.
- [ ] **9.2 `features/auth`.** `useSessionGuard` sustituye las dos copias
      de la guardia.
- [ ] **9.3 `features/inbox`.** `useInbox` y `filters.ts` (puro, con tests).
- [ ] **9.4 `features/conversation`.** `useMessages`, burbuja, multimedia.
- [ ] **9.5 `features/composer`.** Texto, adjuntos y `useVoiceRecorder`
      como tres piezas.
- [ ] **9.6 `features/dashboard`.** `series.ts` (puro, con tests) y
      componentes de la gráfica.
- [ ] **9.7 Tests** (`vitest`) de las funciones puras, en CI.
- [ ] **9.8 Zonas de ESLint en modo estricto.**
- [ ] **9.9 Defecto FE-043:** mostrar quién envió cada mensaje.

**Hecho cuando:** `src/components/` y `src/lib/` no existen y ningún
componente pasa de 200 líneas.

---

## Fase 10 · Cierre

- [ ] **10.1 Código sin consumidor.** `GET /associates/phones`, `GET /users`,
      `GET /users/{phone}`, `GET /guides?user_phone=`, `GET /quota` y la
      tabla `invoices`. Antes de borrar cada uno: dos semanas de logs de
      acceso sin una sola llamada, o tu confirmación de que es de uso manual.
- [ ] **10.2 Documentación.** README de la raíz en una página (qué es, cómo
      se levanta, mapa de servicios); un README por servicio; `docs/operacion.md`
      (despliegue, migraciones, trabajos programados); un registro de
      decisión por cada una de las D1 a D7.
- [ ] **10.3 Sin restos.** Reexports, banderas de compatibilidad y listas
      de excepciones de arquitectura: todos en cero.
- [ ] **10.4 Empezar el plan de seguridad**, y actualizar el estado de los
      hallazgos del documento de errores que esta refactorización cerró de
      paso (6, 8, 9, 10, 11, 14 y 15).

---

## Riesgos

| Riesgo | Dónde aparece | Cómo se cubre |
|---|---|---|
| Notificaciones duplicadas o perdidas por cambiar el orden de registrar, marcar y deshacer | 5.9, 6.x | Tests de 0.4 antes de empezar; comparación de filas en el entorno de pruebas; un flujo cada vez en producción |
| Un cambio "solo de mover" que cambia el comportamiento | todas | Regla 1 y regla 2; fotos de la API (0.5); prueba de humo al cerrar cada fase |
| El trabajo cada 15 minutos corre mientras se despliega una versión a medias | 6.5 | La bandera se cambia con los trabajos detenidos; cada flujo tiene su control de corridas |
| La base de producción y las migraciones versionadas no coinciden | 4.10 | El procedimiento se ensaya sobre una copia de producción antes de aplicarlo |
| La refactorización se alarga y `main` se aleja de `refactor` | todas | Cada fase se mezcla a `main` al cerrarse; nunca más de una fase sin mezclar |
| Funcionalidades urgentes a mitad de una fase | 3, 5 | Decisión D7: entran en la estructura nueva o en el servicio que no se está moviendo |
| Reglas de arquitectura que estorban y se desactivan "por ahora" | 3.14, 5.12 | Los topes son parte del CI; una excepción es un PR que alguien tiene que justificar |
