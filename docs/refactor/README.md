# Refactorización de NotiAsis

Plan para reorganizar los cuatro servicios (bot, backend, ai-agents y
frontend) en módulos independientes, uno por funcionalidad, con las mismas
convenciones en todos.

## El resumen

**Qué pasa hoy.** El sistema creció por funcionalidades entregadas rápido y
cada una dejó su estilo. Los cinco problemas de fondo:

1. **Capacidades sin dueño único.** Dos servicios hablan con Meta, dos
   escriben en la base de datos, tres normalizan teléfonos, dos formatean
   dinero. Lo mismo está implementado varias veces y ya diverge.
2. **Carpetas que no dicen lo que hay dentro.** `app/services/` del bot
   mezcla seis tipos de cosas; en el backend, el módulo de chats envía
   mensajes y el de mensajes deshace notificaciones de guías.
3. **Código copiado en vez de compartido.** La lógica de envío de
   plantillas está tres veces en el bot; el cliente HTTP, cuatro veces en
   el frontend.
4. **Cuatro maneras de resolver lo mismo.** Errores, logs, configuración,
   inyección de dependencias y tests se hacen distinto en cada servicio.
5. **Sin red.** No hay integración continua y el SQL del backend, donde
   viven las reglas más delicadas, no tiene un solo test.

**La idea de la solución.** Tres reglas, las mismas en los cuatro servicios:

- **Un dueño por capacidad.** Solo el backend habla con Meta; solo el bot
  decide qué notificar; solo ai-agents conversa. Los demás piden.
- **Un módulo por funcionalidad, todos con la misma forma**: entrada HTTP,
  casos de uso, dominio, puertos, persistencia. Es la organización de
  VictorStore-Back, con el servicio dividido en un archivo por caso de uso.
- **Los módulos no se importan entre sí.** Lo que uno necesita de otro lo
  declara como una interfaz pequeña, y un único sitio las conecta. El CI
  lo comprueba.

`ai-agents/` ya está construido así y es el modelo: lo que proponemos es
llevar al bot, al backend y al frontend a ese mismo nivel.

**Cómo se llega.** Once fases, cada una desplegable. Primero la red de
seguridad (CI y tests del SQL), después mover sin cambiar comportamiento, y
solo al final cambiar contratos entre servicios. Ningún paso mezcla mover
código con cambiarlo.

## Los documentos

Se leen en orden. Cada uno responde una pregunta.

| # | Documento | Pregunta | Tamaño |
|---|---|---|---|
| 1 | [Diagnóstico](01-diagnostico.md) | ¿Qué hace cada archivo hoy y qué le falla? | 4 servicios, 47 problemas con evidencia |
| 2 | [Casos de uso](02-casos-de-uso.md) | ¿Qué hace el sistema, funcionalidad por funcionalidad, y a qué módulo va cada una? | 257 casos de uso (bot 129, backend 70, ai-agents 35, frontend 23) |
| 3 | [Arquitectura objetivo](03-arquitectura-objetivo.md) | ¿Cómo queda? ¿Qué reglas y convenciones se siguen? | 7 decisiones para ti al final |
| 4 | [Plan de ejecución](04-plan-de-ejecucion.md) | ¿En qué orden y cómo se verifica cada paso? | 11 fases, 93 pasos |

El **plan de seguridad** es un documento aparte y **no está en esta
carpeta ni en git**: el repositorio es público y ese plan describe fallos
sin corregir. Vive en `docs/seguridad/`, que está en `.gitignore`.

## Antes de empezar

1. Revisa las [siete decisiones](03-arquitectura-objetivo.md#10-decisiones-que-necesito-de-ti).
   El plan está escrito con la opción recomendada en cada una.
2. Lee la sección de contención del plan de seguridad: hay cosas que no
   deberían esperar a que termine la refactorización.
3. La fase 0 no mueve código. Se puede empezar hoy.

## Cómo se usa

- Cada paso del plan tiene un número (`3.6`) y cada funcionalidad un
  identificador (`BOT-040`). Los commits y los PR los citan.
- Los pasos están pensados para hacerse de uno en uno:
  "ejecuta el paso 3.6 del plan de refactorización".
- Al terminar un paso se marca su casilla en el plan de ejecución.
- Los casos marcados con ⚠ en el inventario son defectos conocidos. No se
  corrigen al mover el código, sino en el paso que el plan les asigna.
