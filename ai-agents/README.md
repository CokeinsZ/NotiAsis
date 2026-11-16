# ai-agents

Servicio en Rust (Axum) con los agentes de IA de NotiAsis. Hoy tiene uno:
el **agente de atención al cliente**, que responde por WhatsApp los mensajes
de los clientes con mensajes libres (dentro de la ventana de 24 h de Meta).

- Chat: **DeepSeek `deepseek-flash`**, con thinking activo y
  `reasoning_effort: max`.
- Notas de voz: **Gemini 3.5 Transcribe** (`gemini-3.5-transcribe`).

## Cómo funciona

```
Cliente → Meta → Bot /webhook → POST /messages/incoming (backend: elige business, guarda)
                              → POST ai-agents /agents/customer-service/events  {message_id, business_id, user_phone}
ai-agents (202 inmediato, turno en segundo plano):
  1. wait_for_silence        espera 15 s por si el cliente sigue escribiendo
  2. lock_chat               un turno a la vez por chat
  3. check_eligibility       reglas de policy/ (ver abajo)
  4. reserve_capacity        tope de turnos simultáneos con el modelo
  5. load_customer_context   business, persona, políticas, guías y fichas de sus productos
  6. transcribe_voice_notes  notas de voz pendientes → messages.transcription (Gemini)
  7. load_transcript         últimos 100 mensajes de `messages` (con el mensaje que cita cada uno)
  8. generate_reply          DeepSeek + herramientas → texto o [NO_RESPONDER]
  9. deliver_reply           reglas otra vez → POST backend /chats/{bid}/{phone}/ai-messages
```

El agente **no usa una memoria aparte**: lee la tabla `messages`, la misma
que ve la webapp, con cada mensaje etiquetado (`CLIENTE`, `NOTIFICACIÓN
AUTOMÁTICA`, `ASESOR HUMANO`, `TÚ (asistente)`). Sus respuestas se envían por
el backend, que las guarda con `sender = 'ai'`: no hay nada que sincronizar.

Para retomar bien después de un asesor, además ve:

- **Lo que cita el cliente**: si responde citando un mensaje (o toca un botón
  de una plantilla), el backend guarda `messages.reply_to_message_id` y el
  historial lo muestra: `CLIENTE (cita a ASESOR HUMANO del dom 15/11 13:20:
  «¿Me envías una foto?»): ...`, aunque el mensaje citado sea antiguo.
- **Lo que ya pasó**: sus escalamientos anteriores (motivo y si un asesor ya
  respondió) y las novedades que registró (fechas de recogida, etc.).
- **Mensajes de asesores anteriores a `sender`**: `db.sql` los clasifica
  una vez (plantillas del bot por su texto, PDF de guía y videos de producto
  = `bot`; el resto = `associate`), para que no aparezcan como avisos
  automáticos.

### Reglas (cuándo NO responde)

| Regla | Se calla si... |
|---|---|
| `agent_enabled` | el business no tiene fila en `business_ai_agents` o `enabled = false` |
| `allowlist` | `CUSTOMER_AGENT_ALLOWED_PHONES` no está vacío y el teléfono no está |
| `latest_message` | el cliente escribió otra cosa después (responde el turno de ese último) |
| `service_window` | pasaron 24 h (menos 5 min de margen) desde el último mensaje del cliente |
| `not_yet_answered` | el agente ya respondió el último mensaje (avisos duplicados) |
| `human_takeover` | un asesor escribió en las últimas `human_takeover_hours` (8 por defecto) |
| `open_escalation` | el agente escaló el chat y ningún asesor ha respondido (máx. 8 h, `CUSTOMER_AGENT_ESCALATION_QUIET_HOURS`) |
| `rate_limit` | el agente ya envió 6 mensajes en la última hora en ese chat |

Las reglas se evalúan antes de llamar al modelo **y otra vez antes de
enviar** (mientras el modelo razona pudo escribir un asesor o el cliente).

### Herramientas del agente

| Herramienta | Qué hace |
|---|---|
| `detalle_guia` | Productos, dirección, valor y todos los avisos enviados de una guía |
| `estado_envio_en_vivo` | Estado actual en Dropi (webhook `/orden` de N8N) |
| `info_producto` | Ficha de un producto del catálogo del business, buscada por nombre |
| `registrar_novedad` | Guarda `will_pickup` (con fecha), `picked_up` o `declined` en `guide_customer_updates` |
| `escalar_a_humano` | Crea un `ai_escalations` y marca el chat como importante |

El business y el teléfono **nunca los elige el modelo**: llegan del aviso del
bot en `CustomerToolContext`, y cada consulta filtra por ese chat. Una guía
de otra persona devuelve "no encontrada" (hay un test de integración para esto).

## Productos

El catálogo de cada business está en la tabla `products`: para qué sirve,
ingredientes, presentación, dosis, modo de uso, precauciones, duración del
tratamiento, envío, garantía y `agent_notes` (indicaciones para el agente
que no se le leen al cliente). Un campo vacío es un dato que no tenemos: el
agente no lo inventa, dice que lo consulta y escala (`product_question`).

`product_dropi_ids` enlaza cada ficha con los ids de producto de Dropi
(`guide_products.product_id`), porque un mismo producto suele tener varias
publicaciones en Dropi. Con ese enlace:

- las fichas de los productos de las guías del cliente van directo en el
  prompt (sin gastar una llamada a herramienta);
- `info_producto` busca en el resto del catálogo por nombre, sin tildes ni
  mayúsculas ("testosterona" encuentra "Testosterone+"); si no encuentra,
  le devuelve al modelo los nombres del catálogo.

El prompt le pide no prometer resultados más allá de la ficha, recomendar
consultar al médico si el cliente menciona enfermedades, medicamentos o
embarazo, y escalar (`product_issue`) si cuenta una reacción.

`scripts/seed_products.sql` carga el catálogo de Victor Store (desde
`Productos_Tienda.csv`) en "Victor Store" y "Victor Store Pruebas". Es
idempotente: para cambiar una ficha se edita el script y se corre otra vez,
o se hace un `UPDATE products ...` directo.

## DeepSeek: "reasoning_content must be passed back"

En modo thinking, si la petición lleva `tools`, DeepSeek exige recibir de
vuelta el `reasoning_content` de **todos** los mensajes previos del asistente
(aunque ese turno no haya llamado herramientas). Si falta, responde 400. Casi
todos los clientes compatibles con OpenAI lo pierden al rearmar el historial.

Aquí se resuelve en tres puntos:

1. `llm::AssistantMessage` tiene el campo `reasoning` junto a `content` y
   `tool_calls`: el razonamiento es parte del mensaje, no se descarta.
2. `agent::AgentRunner` agrega a la conversación la respuesta **completa** del
   modelo (`conversation.push_assistant(message)`), antes de ejecutar las
   herramientas.
3. `llm/deepseek/wire.rs` serializa `reasoning_content` en cada mensaje del
   asistente cuando thinking está activo (vacío si el modelo no razonó). En
   modo normal no lo manda.

Así se dejan activos thinking y `reasoning_effort: max` (el máximo; sube el
`max_tokens` por defecto a 128K). `temperature` no se envía porque en thinking
no tiene efecto, y `tool_choice` va en `auto` porque `required` o una función
específica dan 400 en ese modo. Lo prueban `wire.rs` (formato) y
`tests/deepseek_reasoning_round_trip.rs`: un servidor falso que, como
DeepSeek, responde 400 si falta el razonamiento.

## Gemini 3.5 Transcribe

`POST {GEMINI_BASE_URL}/models/gemini-3.5-transcribe:generateContent` con el
header `x-goog-api-key`. El audio va inline en base64 (`inlineData`, MIME
sin parámetros: `audio/ogg`) y la configuración en
`generationConfig.audioTranscriptionConfig`:

- `languageCodes: ["es-419"]` (español de Latinoamérica),
- `mode: "SMART"` (quita muletillas y da formato a cifras y monedas),
- `customVocabulary`: productos, transportadoras, ciudades y nombre del
  cliente de ese chat, más `CUSTOMER_AGENT_VOCABULARY` (máx. 100).

La respuesta trae el texto en
`candidates[0].content.parts[].audioTranscription.text`, **no** en
`parts[].text` como los demás modelos de Gemini (`wire.rs` acepta los dos).
Verificado con una nota de voz real de WhatsApp el 2026-11-15.

Si una transcripción falla, el turno sigue y el historial dice
`[nota de voz] (sin transcripción)`; la nota queda pendiente
(`transcription IS NULL`) y se reintenta en el siguiente turno del chat.

Para probar Gemini sin WhatsApp, con el mismo cliente y configuración:

```bash
cargo run --example transcribe_audio -- nota.ogg [vocabulario...]
```

## Estructura

```
src/
├── main.rs, app.rs          # arranque y composition root (proveedores concretos)
├── config/                  # variables de entorno, una sección por archivo
├── observability.rs         # logs (RUST_LOG)
├── http/                    # middleware api-key, /health
├── shared/                  # reintentos, HTTP JSON, plantillas, locks por clave
├── llm/                     # puerto ChatModel + deepseek/{config,wire,client}
├── transcription/           # puerto Transcriber + gemini/{config,wire,client}
├── agent/                   # runtime genérico: Tool, TypedTool, ToolBox, AgentRunner
├── pipeline/                # Step + Pipeline genéricos
├── domain/                  # ChatKey (compartido por los agentes)
├── integrations/            # backend de NotiAsis (token, media, envío) y Dropi
└── agents/customer_service/
    ├── assembly.rs          # conecta todo: el único que conoce las implementaciones
    ├── controller.rs        # POST /events
    ├── dispatcher.rs        # un turno por aviso (tokio::spawn)
    ├── steps/               # un paso por archivo
    ├── policy/rules/        # una regla por archivo
    ├── tools/               # una herramienta por archivo
    ├── prompts/             # system.md y user.md (el texto del prompt)
    ├── presentation/        # formato para el modelo: fechas, dinero, historial, guías
    ├── repositories/        # Postgres, un repositorio por puerto
    ├── ports.rs             # traits que el agente necesita
    ├── domain/              # tipos del agente
    ├── delivery.rs          # envío real o modo borrador
    └── reply.rs             # interpreta la salida ([NO_RESPONDER], límites)
```

## Cómo cambiar o agregar cosas

| Quiero... | Toco |
|---|---|
| Cambiar el comportamiento o el tono | `prompts/system.md` (no hay que tocar código) |
| Políticas de un business (plazos, pagos) | `UPDATE business_ai_agents SET policies = '...'` |
| La ficha de un producto | `scripts/seed_products.sql` (y correrlo) o `UPDATE products SET ...` |
| Enlazar una publicación nueva de Dropi | Su id en el arreglo de la ficha en `seed_products.sql`, o `INSERT INTO product_dropi_ids ...` |
| Una herramienta nueva | Un archivo en `tools/` con un struct que implemente `TypedTool<CustomerToolContext>` (el JSON Schema sale del struct de argumentos) + una línea en `tools/mod.rs` |
| Una regla nueva | Un archivo en `policy/rules/` que implemente `ReplyRule` + una línea en `ReplyPolicy::standard` |
| Un paso nuevo (p. ej. describir imágenes) | Un archivo en `steps/` que implemente `Step<Turn>` + una línea en `assembly.rs` |
| Otro modelo de chat | Un módulo en `llm/` que implemente `ChatModel` + cambiar una línea en `app.rs` |
| Otro transcriptor | Un módulo en `transcription/` que implemente `Transcriber` + una línea en `app.rs` |
| Otro agente | Una carpeta en `agents/` con su `router` + un `.nest(...)` en `app.rs` |

## Configuración

Ver `.env.example`. Obligatorias: `AI_AGENTS_API_KEY`, `DATABASE_URL`,
`DEEPSEEK_API_KEY`, `GOOGLE_AI_API_KEY`, `BACKEND_API_URL`, `BACKEND_API_KEY`
y `N8N_API_KEY`. Si falta una, el servicio no arranca y dice cuál es.

## Ejecutar y probar

```bash
cp .env.example .env
cargo run
cargo test                      # unitarios + DeepSeek con servidor falso
# Repositorios contra Postgres real (aplica webapp/backend/db.sql solo):
docker run -d --rm -e POSTGRES_PASSWORD=test -p 55432:5432 postgres:17
TEST_DATABASE_URL=postgres://postgres:test@localhost:55432/postgres cargo test
```

## Chat de prueba

- `scripts/seed_test_chat.sql` crea (o reinicia) el business "Victor Store
  Pruebas" con el agente activado y un chat con el teléfono de prueba, como si
  NotiAsis ya le hubiera notificado una guía en oficina (4 avisos, el último
  hace 1 hora). Es idempotente: correrlo otra vez borra la conversación y
  la deja recién notificada. Solo toca ese business.
- `scripts/seed_products.sql` le carga el catálogo (córrelo después del
  chat de prueba si el business es nuevo).
- `cargo run --example preview_prompt -- <business_id> <telefono>` imprime lo
  que el agente vería en ese chat (prompt de sistema, fichas e historial)
  sin llamar al modelo.

## Puesta en marcha

1. Correr `webapp/backend/db.sql` (agrega `messages.sender`,
   `messages.transcription`, `countries.timezone` y las tablas
   `business_ai_agents`, `ai_escalations`, `guide_customer_updates`,
   `products` y `product_dropi_ids`), y luego `scripts/seed_products.sql`.
2. Desplegar el backend (endpoint `ai-messages` y la columna `sender`).
3. Crear la api_key de este servicio:
   `INSERT INTO api_keys (key) VALUES ('<AI_AGENTS_BACKEND_API_KEY>');`
4. Desplegar ai-agents en **modo borrador**
   (`CUSTOMER_AGENT_DRY_RUN=true`) y el bot con `AI_AGENTS_URL` y
   `AI_AGENTS_API_KEY`.
5. Activar el agente en un business:
   `INSERT INTO business_ai_agents (business_id, enabled, policies) VALUES (1, true, '...');`
6. Revisar las respuestas en los logs (`DRY RUN: reply not sent`). Cuando se
   vean bien, quitar el modo borrador (primero con
   `CUSTOMER_AGENT_ALLOWED_PHONES` para unos pocos números).

Recomendado: un rol de Postgres propio en vez de `neondb_owner`:

```sql
CREATE ROLE ai_agents LOGIN PASSWORD '...';
GRANT USAGE ON SCHEMA public TO ai_agents;
GRANT SELECT ON businesses, countries, users, chats, messages, guides, guide_products,
      guide_notifications, business_ai_agents, ai_escalations, products, product_dropi_ids TO ai_agents;
GRANT UPDATE (transcription) ON messages TO ai_agents;
GRANT UPDATE (is_important) ON chats TO ai_agents;
GRANT INSERT ON ai_escalations, guide_customer_updates TO ai_agents;
GRANT USAGE ON SEQUENCE ai_escalations_id_seq, guide_customer_updates_id_seq TO ai_agents;
```
