# 009 — Chat con streaming contra el llama-server local

- **Tipo:** feature
- **Estado:** borrador
- **Rama:** feat/009-chat-streaming
- **Fase del roadmap:** Fase 6 (Chat) de [docs/MVP.md](../MVP.md), funcionalidad E. El usuario ha confirmado que el chat entra en el MVP (decisión pendiente 2 de MVP §7).

## Objetivo

Que el usuario compruebe que el modelo que acaba de lanzar funciona sin salir de la app: escribe en una pantalla Chat, ve la respuesta aparecer token a token, puede detenerla y empezar otra conversación, con los parámetros de muestreo de su perfil.

## Contexto

### Precondiciones (planes 003–008, que se suponen hechos)

- Tauri 2 + Svelte 5 + TypeScript en `ui/`; workspace Rust con `crates/core` (sin Tauri) y `src-tauri/`. `ui/package.json` tiene `lint`, `check` y `test` (vitest).
- Plan 008: hay **un único** `llama-server` en marcha como máximo, gestionado desde `src-tauri` con estado `listo / no listo` (health check `/health`: 200 `{"status":"ok"}` = listo; 503 `Loading model` = cargando). Expone el endpoint OpenAI `http://127.0.0.1:PUERTO/v1` y el perfil con el que se lanzó (incluye `temperature` y `top_p`, MVP §2.D).
- La UI ya tiene navegación entre pantallas (Explorar · Biblioteca · Ejecutar · Ajustes) y una forma de leer el estado del servidor.

**Supuesto de integración con 008.** Este plan no conoce los nombres exactos de 008. Necesita dos cosas, y la T5 las adapta a lo que exista:
1. En `src-tauri`, una función que devuelva `Option<RunningServer>` con `base_url` (`http://127.0.0.1:PUERTO`), `ready: bool` y el perfil (`temperature: Option<f32>`, `top_p: Option<f32>`).
2. En la UI, un store legible con el estado del servidor (`ready: boolean`).

Si 008 no los ofrece con esa forma, la T5 y la T9 añaden un adaptador fino, sin modificar la lógica de 008 (ver Preguntas abiertas).

### API de llama-server verificada (2026-11-26)

Fuentes: [README del server](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md), [`server-common.cpp`](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/server-common.cpp), [`server-context.cpp`](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/server-context.cpp), [issue #15443](https://github.com/ggml-org/llama.cpp/issues/15443).

- `POST /v1/chat/completions` con `"stream": true` responde en SSE. `format_oai_sse` escribe cada evento como `data: <json>\n\n`.
- Al terminar se envía `data: [DONE]\n\n`.
- Trozos (`chat.completion.chunk`): `choices[0].delta.content` (texto), `choices[0].delta.reasoning_content` (razonamiento de modelos tipo DeepSeek/Qwen3, según `--reasoning-format`), un primer delta con solo `role`, y un trozo final con `delta` vacío, `finish_reason` (`"stop"`, `"length"`…) y `timings`/`usage`.
- **Error a mitad de stream:** `data: {"error": {"code", "message", "type"}}` y el stream se cierra **sin** `[DONE]`. **Error antes de empezar** (p. ej. contexto excedido, JSON inválido): respuesta HTTP no 2xx con el cuerpo `{"error": {...}}` en formato OpenAI.
- **Cancelación:** el bucle de streaming consulta `should_stop()` (conexión cerrada por el cliente) y deja de generar. Cerrar la conexión HTTP es, por tanto, la forma de detener la generación. `POST /v1/chat/completions/control` solo admite `reasoning_end` y no sirve para parar.
- Pueden llegar líneas de comentario SSE (`:` …) como keep-alive; el parser las ignora.
- `temperature` y `top_p` se aceptan en el cuerpo (OpenAI compatible). Si se omiten, se usan los valores con que se lanzó el servidor.
- CORS: por defecto el server refleja cualquier `Origin` (`--cors-origins`, por defecto `*`), pero cambia a `localhost` con `--tools`/`--agent`.

### Decisión: el streaming pasa por Rust (comando + `tauri::ipc::Channel`), no por `fetch` desde el webview

| Criterio | Rust (`reqwest` en `core` + Channel) | `fetch` desde el webview |
|---|---|---|
| CSP de Tauri 2 | `connect-src` se queda en `ipc: http://ipc.localhost`. No se abre ningún origen HTTP. | Habría que añadir `http://127.0.0.1:*` a `connect-src`. Cualquier XSS que se colase en el markdown podría hablar con el servidor local. |
| Origen y CORS | No aplica (petición nativa). | El origen del webview es `http://tauri.localhost` (Windows) o `tauri://localhost` (macOS). Depende de la política CORS del server, que cambia con sus flags. En macOS además se mezcla un origen propio con `http:`. |
| Testabilidad (R3) | Parser SSE y cliente probados en Rust con un servidor TCP local de test, sin red. | Habría que simular `ReadableStream` en jsdom. |
| Cancelación | Se suelta la respuesta → se cierra el socket → `should_stop()` en el server. Comprobable en un test (el servidor de test ve EOF). | `AbortController`: equivalente, pero se prueba peor. |
| Arquitectura del MVP | Coherente con MVP §3: `core` hace la lógica y la UI recibe streams por IPC, igual que el progreso de descargas y los logs. | Lógica de red repartida entre Rust y TS. |
| Coste | Un salto IPC por lote de deltas. Los Channels están pensados para streams ordenados y rápidos ([docs Tauri 2](https://v2.tauri.app/develop/calling-frontend/)). | Ninguno. |

Se usa `Channel<ChatEvent>` y no eventos globales: los eventos de Tauri no garantizan el orden con listeners asíncronos y no están pensados para alto caudal.

### Diseño

**Rust, `crates/core/src/chat/`** (sin Tauri):
- `sse.rs`: `SseDecoder`, un parser incremental. `push(&[u8]) -> Vec<SseEvent>` acepta bytes partidos en cualquier punto, incluso a mitad de un carácter UTF-8 o de un `\r\n`. `finish() -> Result<(), SseError>` informa si queda un evento a medias. Implementa la parte de la especificación SSE que se necesita: `data:` (con espacio opcional, varias líneas unidas con `\n`), finales de línea `\n`, `\r\n` y `\r`, comentarios `:`, y se ignoran `event:`, `id:`, `retry:` y los campos desconocidos.
- `types.rs`: `Role` (`system|user|assistant`), `ChatMessage { role, content }`, `Sampling { temperature: Option<f32>, top_p: Option<f32> }`, `ChatEvent` (serde, `#[serde(tag = "type", rename_all = "camelCase")]`):
  - `Delta { content }`
  - `Reasoning { content }`
  - `Done { finish_reason: Option<String> }`
  - `Cancelled`
  - `Error { kind: ChatErrorKind, message }`
- `protocol.rs`:
  - `build_request_body(system_prompt: Option<&str>, history: &[ChatMessage], sampling: &Sampling) -> serde_json::Value`
  - `decode_data(data: &str) -> Result<StreamItem, ChatError>`, con `StreamItem` = `Delta | Reasoning | Finish(reason) | Done | Ignore`
- `client.rs`: `stream_chat(http: &reqwest::Client, base_url: &str, body: Value, cancel: CancellationToken, sink: impl FnMut(ChatEvent)) -> Result<StreamOutcome, ChatError>`, con `StreamOutcome = Completed { finish_reason } | Cancelled`.
- `error.rs`: `ChatError` (`thiserror`):
  - `NotReady`
  - `AlreadyGenerating`
  - `Connect(String)`
  - `Http { status: u16, message: String }`
  - `Server(String)` (error dentro del stream)
  - `Interrupted` (el stream se corta sin `[DONE]` ni `finish_reason`)
  - `Protocol(String)` (JSON o SSE inválido)

  Lleva además `ChatErrorKind`, serializable para la UI.
- `registry.rs`: `GenerationRegistry`, como mucho una generación activa. `start(id) -> Result<CancellationToken, ChatError>`, `cancel(id) -> bool`, `finish(id)`.

**`src-tauri`:**
- `chat_send(request_id: u64, system_prompt: Option<String>, messages: Vec<ChatMessage>, on_event: Channel<ChatEvent>)`:
  1. Lee `RunningServer`. Si no hay ninguno o no está listo, devuelve `NotReady`.
  2. Registra la generación (`AlreadyGenerating` si ya hay una activa).
  3. Construye el cuerpo con el `Sampling` del perfil.
  4. Lanza `stream_chat` en una tarea `tokio`.
  5. Reenvía cada `ChatEvent` al canal y termina siempre con exactamente un evento terminal (`Done`, `Cancelled` o `Error`).
- `chat_cancel(request_id: u64) -> bool`.
- **No** se pasa ningún parámetro de muestreo desde la UI: los pone Rust a partir del perfil del servidor en marcha (CA3).

**UI, `ui/src/lib/chat/`:**
- `state.ts`: un reducer puro `reduce(state, action)` con este estado:

  ```
  {
    messages: Msg[],
    status: 'idle' | 'generating',
    activeRequestId: number | null,
    nextRequestId: number,
    systemPrompt: string,
  }
  ```

  Cada `Msg` tiene `{ id, role, content, reasoning, state: 'complete' | 'streaming' | 'stopped' | 'interrupted' | 'error', error? }`. Se añade `toRequestMessages(state)`.
- `batcher.ts`: agrupa los deltas que llegan en el mismo frame (planificador inyectable) para no volver a renderizar el markdown por cada token.
- `markdown.ts`: `renderMarkdown(src): string` con `markdown-it` y `html: false` (el HTML crudo se escapa). Usa `validateLink` (lista blanca `http:`, `https:`, `mailto:`) y la regla de render de los enlaces añade `rel="noopener noreferrer"`. Las imágenes no se renderizan (se muestran como enlace de texto).
- `api.ts`: `sendChat` y `cancelChat`, envoltorios sobre `invoke` y `Channel` con `invoke` inyectable.
- `chatStore.svelte.ts`: el estado de módulo con runes (`$state`), que vive lo que dure la sesión de la app.
- `ui/src/routes|screens/Chat.svelte` (según la convención de 003): la vista.

**Historial que se envía al servidor:** el prompt de sistema (si `trim()` no está vacío) va primero y después los mensajes `user` y `assistant`:
- Entran los mensajes `complete` y `stopped`, y los `interrupted` con contenido.
- Se excluyen los asistentes en `error` o vacíos, y el usuario cuyo asistente acabó en `error` previo al primer token.
- Nunca se envía `reasoning`.

**Enlaces en el markdown:** un clic en un `<a>` dentro de un mensaje hace `preventDefault()`, para que el webview no navegue fuera de la app. Abrirlos en el navegador del sistema queda fuera de alcance.

## Criterios de aceptación

- **CA1 — Streaming visible.** Dado un servidor listo, cuando el usuario escribe "Hola" y pulsa Enviar (o Enter), entonces su mensaje aparece en el historial y la respuesta del asistente crece con cada `Delta` en el orden de llegada. Al recibir `Done` el mensaje pasa a `complete` y el estado vuelve a `idle`. Lo verifican `stream_emits_deltas_in_order_then_done` (T4), `reducer_appends_deltas_to_streaming_message` (T6) y `chat_view_shows_streamed_text` (T9).
- **CA2 — Parser SSE robusto a la fragmentación.** Dada una respuesta SSE válida, cuando llega partida en trozos arbitrarios (1 byte, a mitad de un carácter UTF-8 de 4 bytes, entre `\r` y `\n`), entonces la secuencia de eventos decodificados es idéntica a la que sale de procesarla de una sola vez. Lo verifican los tests de T1.
- **CA3 — Muestreo del perfil.** Dado un servidor lanzado con un perfil de `temperature = 0.3` y `top_p = 0.8`, cuando se envía un mensaje, entonces el cuerpo POST contiene `"stream": true`, `"temperature": 0.3` y `"top_p": 0.8`. Si el perfil no los define, las claves no aparecen. Lo verifican `body_includes_profile_sampling`, `body_omits_unset_sampling` (T2) y `server_receives_expected_body` (T4).
- **CA4 — Prompt de sistema opcional.** Dado un prompt de sistema "Responde en catalán", cuando se envía un mensaje, entonces `messages[0]` es `{role: "system", content: "Responde en catalán"}`. Con el prompt vacío o solo con espacios, no hay mensaje `system`. Lo verifican `body_puts_system_prompt_first` y `body_skips_blank_system_prompt` (T2).
- **CA5 — Detener generación.** Dado un mensaje en `streaming`, cuando el usuario pulsa Detener, entonces:
  - el cliente cierra la conexión HTTP: el servidor de test lee EOF sin haber enviado `[DONE]`;
  - el canal recibe exactamente un `Cancelled`;
  - el mensaje conserva el texto parcial en estado `stopped`;
  - la entrada vuelve a estar habilitada.

  Lo verifican `cancel_closes_connection_and_returns_cancelled` (T4), `reducer_cancelled_marks_stopped_keeps_text` (T6) y `chat_view_stop_calls_cancel` (T9).
- **CA6 — Nueva conversación.** Dado un historial con mensajes (con o sin una generación activa), cuando el usuario pulsa Nueva conversación, entonces:
  - se cancela la generación activa si la hay;
  - el historial queda vacío y el prompt de sistema se conserva;
  - los eventos que lleguen después con el `request_id` anterior se ignoran.

  Lo verifican `reducer_new_conversation_clears_messages_keeps_system_prompt` y `reducer_ignores_events_from_stale_request` (T6).
- **CA7 — Historial en memoria de la sesión.** Dada una conversación de dos turnos, cuando se envía el tercer mensaje, entonces la petición incluye los cuatro mensajes previos más el nuevo, en orden. Al cambiar de pantalla y volver, el historial sigue ahí. Al reiniciar la app, está vacío: no se escribe ningún fichero. Lo verifican `to_request_messages_includes_full_history` (T6) y `chat_store_survives_remount` (T9). Que no se persista se comprueba en la revisión: no hay escritura a disco en `chat/`.
- **CA8 — Markdown seguro.** Dado un contenido del asistente con `<script>`, `<img onerror>`, `[x](javascript:alert(1))`, `[x](data:text/html,…)` o `<a href="vbscript:…">`, cuando se renderiza, entonces el HTML resultante no contiene ninguna etiqueta `<script>`, `<img>` ni `<iframe>`, ningún atributo `on*` y ningún `href` cuyo esquema no sea `http`, `https` o `mailto`. Los bloques de código, listas, negritas y tablas se renderizan. Lo verifican los tests de T7.
- **CA9 — Deshabilitado si el servidor no está listo.** Dado que no hay servidor o que está cargando, cuando el usuario abre Chat, entonces el campo de texto y Enviar están deshabilitados y se muestra "Inicia un modelo en Ejecutar para chatear". Si se invoca `chat_send` igualmente, devuelve `NotReady` sin hacer ninguna petición HTTP. Lo verifican `chat_view_disabled_when_server_not_ready` (T9) y `send_rejected_when_not_ready` (T5).
- **CA10 — Una generación a la vez.** Dada una generación activa, cuando se intenta enviar otra, entonces Enviar está deshabilitado en la UI y `chat_send` devuelve `AlreadyGenerating`. Lo verifican `registry_rejects_second_generation` (T3) y `reducer_send_ignored_while_generating` (T6).
- **CA11 — Errores.** Dado:
  - (a) un servidor que rechaza la conexión;
  - (b) un servidor que corta el stream tras dos deltas sin `[DONE]`;
  - (c) un `data: {"error":{...,"message":"X"}}` a mitad de stream;
  - (d) una respuesta HTTP 400 con `{"error":{"message":"exceeds context"}}`,

  cuando se envía un mensaje, entonces el canal recibe exactamente un `Error` con `kind` = `connect`, `interrupted`, `server` (mensaje "X") o `http` (mensaje "exceeds context"), respectivamente. En (b) el texto parcial se conserva en estado `interrupted`, y en todos los casos se puede volver a enviar. Lo verifican los tests de error de T4 y `reducer_error_after_deltas_marks_interrupted` (T6).
- **CA12 — Razonamiento separado.** Dado un stream con `delta.reasoning_content`, cuando se recibe, entonces se acumula en `reasoning` (no en `content`), se muestra en un bloque plegado "Razonamiento" y no se envía en el historial de las peticiones siguientes. Lo verifican `decode_reasoning_content` (T2), `reducer_accumulates_reasoning_separately` y `to_request_messages_excludes_reasoning` (T6).
- **CA13 — CSP sin acceso HTTP directo.** Dado `src-tauri/tauri.conf.json`, cuando se inspecciona `app.security.csp`, entonces existe y su `connect-src` no contiene `http://127.0.0.1`, `http://localhost` ni `*`. Lo verifica `csp_does_not_allow_direct_http` (T5).

## Tareas

### T1 — Parser SSE incremental en `core`
- **Ficheros/módulos:** `crates/core/src/chat/mod.rs`, `crates/core/src/chat/sse.rs`, `crates/core/src/lib.rs`
- **Depende de:** —
- **Cubre:** CA2
- **Subtareas:**
  - [ ] T1.1 `SseEvent { data: String }` y `SseError` (`thiserror`): `InvalidUtf8` y `IncompleteEvent`.
  - [ ] T1.2 `SseDecoder::push(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, SseError>`: búfer de bytes, corte por líneas en bytes (`\n`, `\r\n`, `\r`, con un `\r` final pendiente hasta ver el siguiente byte), y UTF-8 validado por línea completa.
  - [ ] T1.3 `finish(self) -> Result<(), SseError>`: error si hay datos acumulados sin la línea en blanco que cierra el evento.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `single_event_in_one_chunk` | `b"data: {\"a\":1}\n\n"` | 1 evento con `data = {"a":1}` |
  | `two_events_in_one_chunk` | `b"data: a\n\ndata: b\n\n"` | `["a", "b"]` |
  | `event_split_byte_by_byte` | La respuesta de referencia (3 deltas + `[DONE]`) troceada de 1 en 1 byte | Los mismos eventos que en un solo trozo |
  | `split_at_every_position` | Para cada `i` en `0..len`, `push(&r[..i])` y luego `push(&r[i..])` | Siempre iguales a la referencia |
  | `utf8_multibyte_split` | `"data: 🦙ñ\n\n"` partido dentro de los 4 bytes de 🦙 | 1 evento con `data = "🦙ñ"` |
  | `crlf_line_endings` | `b"data: a\r\n\r\n"` | `["a"]` |
  | `crlf_split_between_cr_and_lf` | `b"data: a\r"` y luego `b"\n\r\n"` | `["a"]`, sin evento vacío extra |
  | `cr_only_line_endings` | `b"data: a\r\r"` | `["a"]` |
  | `comment_lines_ignored` | `b": ping\n\ndata: a\n\n"` | `["a"]` |
  | `multiline_data_joined` | `b"data: a\ndata: b\n\n"` | `["a\nb"]` |
  | `data_without_space` | `b"data:a\n\n"` | `["a"]` |
  | `other_fields_ignored` | `b"event: x\nid: 1\nretry: 5\ndata: a\n\n"` | `["a"]` |
  | `blank_line_without_data_emits_nothing` | `b"\n\n"` | `[]` |
  | `invalid_utf8_is_error` | `b"data: \xff\n\n"` | `Err(SseError::InvalidUtf8)` |
  | `finish_with_pending_event_is_error` | `push(b"data: a\n")` y `finish()` | `Err(SseError::IncompleteEvent)` |
  | `finish_clean_is_ok` | `push(b"data: a\n\n")` y `finish()` | `Ok(())` |

### T2 — Tipos, cuerpo de la petición y decodificación de trozos
- **Ficheros/módulos:** `crates/core/src/chat/types.rs`, `crates/core/src/chat/protocol.rs`, `crates/core/src/chat/error.rs`
- **Depende de:** —
- **Cubre:** CA3, CA4, CA12, parte de CA11
- **Subtareas:**
  - [ ] T2.1 `Role`, `ChatMessage`, `Sampling`, `ChatEvent` y `ChatErrorKind` con serde (`camelCase`; `kind` en minúsculas: `notReady`, `alreadyGenerating`, `connect`, `http`, `server`, `interrupted`, `protocol`).
  - [ ] T2.2 `ChatError` (`thiserror`) y `ChatError::kind()`.
  - [ ] T2.3 `build_request_body`: `{"messages": [...], "stream": true}`, más `temperature` y `top_p` solo si son `Some`. Sin `model` (llama-server sirve un solo modelo).
  - [ ] T2.4 `decode_data(&str) -> Result<StreamItem, ChatError>`, que reconoce:
    - `[DONE]`;
    - el objeto `error`;
    - `choices[0].delta.content` y `reasoning_content` (como `String` no vacío);
    - `finish_reason` no nulo;
    - todo lo demás, como `Ignore`.

    Si en un mismo trozo vienen contenido y `finish_reason`, devuelve los dos (`Vec<StreamItem>`).
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `body_includes_profile_sampling` | Historial `[user "Hola"]`, `Sampling{0.3, 0.8}` | JSON con `stream: true`, `temperature: 0.3`, `top_p: 0.8` |
  | `body_omits_unset_sampling` | `Sampling{None, None}` | Sin claves `temperature` ni `top_p` |
  | `body_puts_system_prompt_first` | system `"Responde en catalán"`, historial `[user, assistant, user]` | `messages[0].role == "system"` y 4 mensajes en orden |
  | `body_skips_blank_system_prompt` | system `"  \n"` | Ningún mensaje `system` |
  | `body_has_no_model_field` | Cualquiera | Sin clave `model` |
  | `decode_done_marker` | `"[DONE]"` | `[Done]` |
  | `decode_content_delta` | `{"choices":[{"delta":{"content":"Ho"}}]}` | `[Delta("Ho")]` |
  | `decode_role_only_delta_ignored` | `{"choices":[{"delta":{"role":"assistant"}}]}` | `[Ignore]` |
  | `decode_reasoning_content` | `{"choices":[{"delta":{"reasoning_content":"pienso"}}]}` | `[Reasoning("pienso")]` |
  | `decode_finish_reason_with_timings` | `{"choices":[{"delta":{},"finish_reason":"stop"}],"timings":{}}` | `[Finish(Some("stop"))]` |
  | `decode_content_and_finish_same_chunk` | `delta.content "a"` + `finish_reason "length"` | `[Delta("a"), Finish(Some("length"))]` |
  | `decode_empty_choices_ignored` | `{"choices":[],"usage":{}}` | `[Ignore]` |
  | `decode_stream_error_object` | `{"error":{"code":500,"message":"X","type":"server_error"}}` | `Err(ChatError::Server("X"))` |
  | `decode_invalid_json_is_protocol_error` | `"{no json"` | `Err(ChatError::Protocol(_))` |
  | `chat_event_serializes_tagged` | `ChatEvent::Delta{content:"a"}` | `{"type":"delta","content":"a"}` |
  | `error_kind_serializes_camel_case` | `ChatErrorKind::AlreadyGenerating` | `"alreadyGenerating"` |

### T3 — Registro de la generación activa
- **Ficheros/módulos:** `crates/core/src/chat/registry.rs`, `crates/core/Cargo.toml` (`tokio-util` para `CancellationToken`, si no está ya)
- **Depende de:** T2
- **Cubre:** CA10, parte de CA5 y CA6
- **Subtareas:**
  - [ ] T3.1 `GenerationRegistry` con `Mutex<Option<(u64, CancellationToken)>>`: `start(id)`, `cancel(id) -> bool` y `finish(id)` (solo limpia si coincide el id).
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `registry_starts_when_idle` | `start(1)` | `Ok(token)` sin cancelar |
  | `registry_rejects_second_generation` | `start(1)`, `start(2)` | Segundo: `Err(AlreadyGenerating)` |
  | `registry_cancel_matching_id` | `start(1)`, `cancel(1)` | `true` y token cancelado |
  | `registry_cancel_other_id_is_noop` | `start(1)`, `cancel(2)` | `false` y token no cancelado |
  | `registry_finish_frees_slot` | `start(1)`, `finish(1)`, `start(2)` | `Ok` |
  | `registry_finish_stale_id_keeps_active` | `start(1)`, `finish(9)`, `start(2)` | `Err(AlreadyGenerating)` |
  | `registry_cancel_when_idle` | `cancel(1)` | `false` |

### T4 — Cliente de streaming con cancelación y errores
- **Ficheros/módulos:** `crates/core/src/chat/client.rs`, `crates/core/tests/chat_stream.rs` (o `#[cfg(test)]` con helper `test_support::sse_server`), `crates/core/Cargo.toml` (feature `stream` de `reqwest`, si falta)
- **Depende de:** T1, T2
- **Cubre:** CA1, CA3, CA5, CA11
- **Subtareas:**
  - [ ] T4.1 Servidor de test: un `tokio::net::TcpListener` en `127.0.0.1:0` que lee la petición (cabeceras + cuerpo por `Content-Length`), la devuelve al test por un `oneshot`, y escribe una respuesta HTTP/1.1 `Transfer-Encoding: chunked` con un guion de trozos. La secuencia se marca con canales (`mpsc`), **sin `sleep`** (R3). Al terminar informa si leyó EOF del cliente.
  - [ ] T4.2 `stream_chat`:
    1. hace el POST a `{base_url}/v1/chat/completions`;
    2. si el estado no es 2xx, lee el cuerpo y extrae `error.message` → `Http{status, message}`;
    3. si es 2xx, consume `bytes_stream()` con `SseDecoder` y `decode_data`, y emite `Delta` y `Reasoning`;
    4. guarda `finish_reason`;
    5. termina con `Ok(Completed)` ante `[DONE]`, o al llegar al EOF con un `finish_reason` ya visto;
    6. con EOF sin `[DONE]` ni `finish_reason` → `Err(Interrupted)`.
  - [ ] T4.3 Cancelación: `tokio::select!` entre el siguiente trozo y `cancel.cancelled()`. Al cancelar se suelta el stream (se cierra el socket) y se devuelve `Ok(Cancelled)`. La comprobación también se hace antes de enviar.
  - [ ] T4.4 Errores de conexión → `Connect(msg)` (timeout de conexión de 5 s en el `reqwest::Client` que se construye en T5; el test usa un puerto cerrado, sin esperar a ningún reloj).
- **Tests unitarios** (respuesta de referencia: `role`, `"Ho"`, `"la"`, `finish stop`, `[DONE]`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `stream_emits_deltas_in_order_then_done` | Respuesta de referencia | Sink: `[Delta "Ho", Delta "la"]`; `Ok(Completed{Some("stop")})` |
  | `stream_handles_chunk_boundaries_mid_event` | Referencia escrita en trozos HTTP de 3 bytes | El mismo resultado que el anterior |
  | `server_receives_expected_body` | `Sampling{0.3,0.8}`, system + user | El servidor recibe `POST /v1/chat/completions`, `Content-Type: application/json` y el cuerpo de T2 |
  | `stream_forwards_reasoning` | Trozo con `reasoning_content "r"` y luego `content "a"` | `[Reasoning "r", Delta "a"]` |
  | `stream_ignores_sse_comments` | `: ping\n\n` intercalado | Los mismos deltas, sin error |
  | `cancel_closes_connection_and_returns_cancelled` | El servidor envía `"Ho"` y espera. El test cancela al recibir el primer `Delta` | `Ok(Cancelled)`; el servidor lee EOF; no hay más deltas |
  | `cancel_before_send_makes_no_request` | Token ya cancelado | `Ok(Cancelled)`; el servidor no recibe ninguna conexión (el `accept` sigue pendiente; se comprueba con `try_recv` vacío) |
  | `stream_cut_without_done_is_interrupted` | `"Ho"`, `"la"` y cierre del socket | Sink con 2 deltas; `Err(Interrupted)` |
  | `stream_eof_after_finish_reason_is_completed` | `"Ho"`, `finish stop` y cierre sin `[DONE]` | `Ok(Completed{Some("stop")})` |
  | `stream_error_event_is_server_error` | `"Ho"` y luego `data: {"error":{"message":"X"}}` | `Err(Server("X"))` |
  | `http_400_maps_to_http_error` | `400` + `{"error":{"code":400,"message":"exceeds context","type":"invalid_request_error"}}` | `Err(Http{400, "exceeds context"})` |
  | `http_500_without_json_body` | `500` + cuerpo `oops` | `Err(Http{500, "oops"})` |
  | `connection_refused_is_connect_error` | `base_url` en un puerto obtenido con `bind(0)` y liberado | `Err(Connect(_))` |
  | `malformed_json_chunk_is_protocol_error` | `data: {roto\n\n` | `Err(Protocol(_))` |

### T5 — Comandos Tauri `chat_send` / `chat_cancel` y CSP
- **Ficheros/módulos:** `src-tauri/src/commands/chat.rs` (o la convención de 003), `src-tauri/src/lib.rs` (registro de comandos y estado), `src-tauri/tauri.conf.json` (CSP), adaptador a `RunningServer` de 008, `src-tauri/capabilities/*.json` si 003 restringe comandos
- **Depende de:** T3, T4, plan 008
- **Cubre:** CA9, CA10, CA13, y el cableado de CA1, CA5 y CA11
- **Subtareas:**
  - [ ] T5.1 `ChatState { http: reqwest::Client, registry: GenerationRegistry }`, gestionado con `app.manage`. El cliente tiene un `connect_timeout` de 5 s y ningún timeout total (las respuestas largas son legítimas).
  - [ ] T5.2 Una función pura y testeable `prepare_send(server: Option<RunningServer>, registry, request_id, system_prompt, messages) -> Result<(String, Value, CancellationToken), ChatError>`, que devuelve la URL base, el cuerpo y el token.
  - [ ] T5.3 `chat_send`:
    1. llama a `prepare_send`;
    2. lanza `tauri::async_runtime::spawn(stream_chat(...))`;
    3. reenvía los eventos a `Channel<ChatEvent>`;
    4. al final emite un único evento terminal (`Done`, `Cancelled` o `Error{kind, message}`) y llama a `registry.finish(id)`.

    El comando devuelve `Ok(())` en cuanto arranca la tarea, o un `Err(ChatErrorDto)` si `prepare_send` falla.
  - [ ] T5.4 `chat_cancel(request_id) -> bool`.
  - [ ] T5.5 CSP en `tauri.conf.json`: si 003 ya la define, solo se comprueba. Si no, se fija `default-src 'self'; connect-src ipc: http://ipc.localhost; img-src 'self' data:; style-src 'self' 'unsafe-inline'`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `send_rejected_when_no_server` | `server = None` | `Err(NotReady)`; registro libre |
  | `send_rejected_when_not_ready` | `RunningServer{ready:false}` | `Err(NotReady)`; registro libre |
  | `send_rejected_when_already_generating` | Registro con id 1 activo; `prepare_send(id 2)` | `Err(AlreadyGenerating)` |
  | `send_uses_profile_sampling` | Perfil `temperature 0.3`, `top_p None` | Cuerpo con `temperature: 0.3` y sin `top_p` |
  | `send_builds_url_from_running_server` | `base_url "http://127.0.0.1:8081"` | URL base `http://127.0.0.1:8081` |
  | `terminal_event_for_outcomes` | `Ok(Completed{stop})`, `Ok(Cancelled)`, `Err(Interrupted)` → `to_terminal_event` | `Done{Some("stop")}`, `Cancelled`, `Error{kind: interrupted}` |
  | `csp_does_not_allow_direct_http` | `include_str!("../tauri.conf.json")` parseado | `app.security.csp` existe y `connect-src` no contiene `http://127.0.0.1`, `http://localhost` ni `*` |

### T6 — Estado del chat en la UI (reducer puro)
- **Ficheros/módulos:** `ui/src/lib/chat/types.ts`, `ui/src/lib/chat/state.ts`, `ui/src/lib/chat/state.test.ts`
- **Depende de:** T2 (contrato de `ChatEvent` y `ChatErrorKind`)
- **Cubre:** CA1, CA5, CA6, CA7, CA10, CA11, CA12
- **Subtareas:**
  - [ ] T6.1 Tipos TS espejo de `ChatEvent`, `ChatErrorKind` y `ChatMessage`.
  - [ ] T6.2 Acciones:
    - `send(text)`: ignora el texto vacío o el estado `generating`. Asigna `requestId = nextRequestId++` (contador, sin aleatoriedad) y añade un `user` y un `assistant` en `streaming`.
    - `event(requestId, ChatEvent)`: ignora los ids que no son `activeRequestId`.
    - `setSystemPrompt`.
    - `newConversation`.
    - `sendFailed(requestId, error)`: fallo síncrono del `invoke`.
  - [ ] T6.3 `toRequestMessages(state)` según las reglas de historial del Diseño. Excluye el par recién añadido que todavía está en `streaming`: el texto nuevo lo añade `send`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `reducer_send_adds_user_and_streaming_assistant` | Estado vacío, `send("Hola")` | 2 mensajes, `status generating`, `activeRequestId 0`, `nextRequestId 1` |
  | `reducer_send_trims_and_ignores_blank` | `send("   ")` | Estado sin cambios |
  | `reducer_send_ignored_while_generating` | `generating`, `send("x")` | Estado sin cambios |
  | `reducer_appends_deltas_to_streaming_message` | `delta "Ho"`, `delta "la"` | `content "Hola"` |
  | `reducer_done_marks_complete_and_idle` | `done` | Mensaje `complete`, `idle`, `activeRequestId null` |
  | `reducer_cancelled_marks_stopped_keeps_text` | `"Ho"` y `cancelled` | `stopped`, `content "Ho"`, `idle` |
  | `reducer_error_before_tokens_marks_error` | `error{connect}` sin deltas | Asistente `error` con mensaje, `idle` |
  | `reducer_error_after_deltas_marks_interrupted` | `"Ho"` y `error{interrupted}` | `interrupted`, `content "Ho"`, error visible |
  | `reducer_send_failed_marks_error` | `sendFailed(0, notReady)` | Asistente `error`, `idle` |
  | `reducer_accumulates_reasoning_separately` | `reasoning "a"`, `reasoning "b"`, `delta "c"` | `reasoning "ab"`, `content "c"` |
  | `reducer_ignores_events_from_stale_request` | `newConversation` durante `generating` (id 0) y luego `event(0, delta)` | Sin mensajes |
  | `reducer_new_conversation_clears_messages_keeps_system_prompt` | Historial + `systemPrompt "S"` | `messages []`, `systemPrompt "S"`, `idle` |
  | `to_request_messages_includes_full_history` | 2 turnos completos + un `send` nuevo | 5 mensajes `user`/`assistant` en orden |
  | `to_request_messages_excludes_failed_turns` | Turno con asistente `error` sin contenido | Ni ese asistente ni su `user` |
  | `to_request_messages_keeps_stopped_partial` | Asistente `stopped` `"Ho"` | Incluido como `assistant "Ho"` |
  | `to_request_messages_excludes_reasoning` | Asistente con `reasoning` | El objeto solo tiene `role` y `content` |

### T7 — Renderizado de markdown seguro
- **Ficheros/módulos:** `ui/src/lib/chat/markdown.ts`, `ui/src/lib/chat/markdown.test.ts`, `ui/package.json` (`markdown-it`, `@types/markdown-it`)
- **Depende de:** —
- **Cubre:** CA8
- **Subtareas:**
  - [ ] T7.1 Instancia de `markdown-it` con `html: false`, `linkify: false` y `typographer: false`.
  - [ ] T7.2 `validateLink` con lista blanca de esquemas `http:`, `https:` y `mailto:`, más rutas relativas y anclas `#`.
  - [ ] T7.3 Regla `image` → se renderiza como texto `[imagen: alt]` sin `<img>`. Regla `link_open` → añade `rel="noopener noreferrer"`.
- **Tests unitarios** (las aserciones se hacen sobre el HTML parseado con `DOMParser` de jsdom: etiquetas, atributos y `href`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `escapes_script_tag` | `<script>alert(1)</script>` | No hay ningún `<script>`; el texto aparece escapado |
  | `escapes_img_onerror` | `<img src=x onerror=alert(1)>` | Ningún `<img>` ni atributo `on*` |
  | `blocks_javascript_link` | `[x](javascript:alert(1))` | No hay `<a>` con `href` que empiece por `javascript:` |
  | `blocks_javascript_link_obfuscated` | `[x](JaVaScRiPt:alert(1))`, `[x](java&#x09;script:alert(1))` | Ningún `href` peligroso |
  | `blocks_data_and_vbscript_links` | `[x](data:text/html,<b>)`, `[x](vbscript:msgbox)` | Ningún `href` con esos esquemas |
  | `blocks_file_link` | `[x](file:///etc/passwd)` | Ningún `href` `file:` |
  | `markdown_image_not_rendered` | `![a](https://e.com/i.png)` | Ningún `<img>`; aparece `imagen: a` |
  | `raw_iframe_escaped` | `<iframe src="https://e.com">` | Ningún `<iframe>` |
  | `allows_https_link_with_rel` | `[x](https://e.com)` | `<a href="https://e.com" rel="noopener noreferrer">` |
  | `renders_code_block` | ```` ```rust\nfn a(){}\n``` ```` | `<pre><code class="language-rust">` con el contenido escapado |
  | `renders_inline_formatting_and_lists` | `**a** _b_\n\n- c\n- d` | `<strong>`, `<em>`, `<ul>` con 2 `<li>` |
  | `renders_table` | Tabla GFM 2x2 | `<table>` con 2 `<th>` |
  | `unterminated_code_fence_while_streaming` | ```` ```py\nprint( ```` | `<pre><code>` sin lanzar ninguna excepción |
  | `empty_input` | `""` | `""` |

### T8 — Adaptador IPC y agrupador de deltas
- **Ficheros/módulos:** `ui/src/lib/chat/api.ts`, `ui/src/lib/chat/batcher.ts` y sus `*.test.ts`
- **Depende de:** T6
- **Cubre:** CA1, CA5 (cableado UI ↔ Rust)
- **Subtareas:**
  - [ ] T8.1 `createChatApi({ invoke, createChannel })`:
    - `send(requestId, systemPrompt, messages, onEvent)` crea el `Channel`, asigna `onmessage` y llama a `invoke('chat_send', {...})`. Si `invoke` rechaza, llama a `onEvent` con un `error` sintético.
    - `cancel(requestId)` llama a `invoke('chat_cancel', { requestId })`.
    - Con `@tauri-apps/api/core` por defecto.
  - [ ] T8.2 `createDeltaBatcher(schedule: (cb) => void, flush: (events) => void)`:
    - acumula los `delta` y `reasoning` consecutivos y los fusiona en un solo evento por frame;
    - un evento terminal se entrega inmediatamente, después de vaciar lo pendiente y conservando el orden.

    En producción, `schedule` es `requestAnimationFrame`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `api_send_invokes_chat_send_with_args` | Un `invoke` falso | Llamada `('chat_send', {requestId, systemPrompt, messages, onEvent: channel})` |
  | `api_forwards_channel_messages` | El canal falso emite `delta` | `onEvent` recibe el `delta` |
  | `api_invoke_rejection_becomes_error_event` | `invoke` rechaza con `{kind:"notReady", message:"…"}` | `onEvent({type:"error", kind:"notReady"})` |
  | `api_cancel_invokes_chat_cancel` | `cancel(3)` | `('chat_cancel', {requestId: 3})` |
  | `batcher_merges_deltas_in_one_frame` | 3 `delta` antes de que corra `schedule` | 1 `flush` con `delta "abc"` |
  | `batcher_keeps_reasoning_and_content_order` | `reasoning "r"`, `delta "a"`, `reasoning "s"` | 3 eventos en ese orden (solo se fusionan los consecutivos del mismo tipo) |
  | `batcher_flushes_pending_before_terminal` | `delta "a"` y `done` | `flush([delta "a", done])` síncrono, sin esperar a `schedule` |
  | `batcher_schedules_once_per_frame` | 5 deltas | `schedule` llamado 1 vez |

### T9 — Pantalla Chat
- **Ficheros/módulos:** `ui/src/lib/chat/chatStore.svelte.ts`, `ui/src/(routes|screens)/Chat.svelte` y sus componentes (`MessageList.svelte`, `MessageBubble.svelte`, `Composer.svelte`), entrada en la navegación, `ui/package.json` (`@testing-library/svelte` si 003 no la incluye)
- **Depende de:** T5, T6, T7, T8
- **Cubre:** CA1, CA5, CA6, CA7, CA9, CA12 (y CA11 visible)
- **Subtareas:**
  - [ ] T9.1 `chatStore`: el estado del reducer en `$state` de módulo, `send`, `stop` y `newConversation`, que encadenan reducer, api y batcher. `newConversation` llama a `cancel` si hay generación activa. `api` y `serverStatus` son inyectables para los tests.
  - [ ] T9.2 La vista:
    - campo opcional "Prompt de sistema", plegable;
    - lista de mensajes: usuario como texto plano y asistente con `{@html renderMarkdown(content)}`;
    - bloque `<details>` "Razonamiento" cuando `reasoning` no está vacío;
    - etiquetas de estado "Detenido", "Interrumpido" y "Error: …";
    - auto-scroll mientras llegan deltas.
  - [ ] T9.3 Compositor: Enter envía y Shift+Enter hace un salto de línea. Mientras `generating`, el botón es Detener. Botón Nueva conversación.
  - [ ] T9.4 Deshabilitado con `!serverReady`, con el aviso "Inicia un modelo en Ejecutar para chatear". Se intercepta el clic en `a` dentro de los mensajes (`preventDefault`).
- **Tests unitarios** (testing-library + vitest/jsdom, con la api y el estado del servidor falsos):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `chat_view_disabled_when_server_not_ready` | `serverReady = false` | Textarea y Enviar `disabled`; aviso visible |
  | `chat_view_enabled_when_server_ready` | `serverReady = true` | Textarea habilitada |
  | `chat_view_shows_streamed_text` | Enviar "Hola"; la api falsa emite `delta "**Ho**"`, `delta "la"` y `done`; se vacía el `schedule` falso | Burbuja del asistente con `<strong>Ho</strong>la` |
  | `chat_view_enter_sends_shift_enter_newline` | Enter / Shift+Enter en la textarea | Enter llama a `api.send`; Shift+Enter no |
  | `chat_view_stop_calls_cancel` | Durante `generating`, clic en Detener | `api.cancel(0)` llamado |
  | `chat_view_new_conversation_clears` | Historial con 2 mensajes y clic en Nueva conversación | Lista vacía |
  | `chat_view_shows_error_and_reenables` | La api emite `error{interrupted}` | Etiqueta "Interrumpido" y textarea habilitada |
  | `chat_view_reasoning_collapsed` | `reasoning "pienso"` y `delta "a"` | `<details>` sin `open` que contiene "pienso" |
  | `chat_view_link_click_prevented` | Mensaje con `[x](https://e.com)` y clic en el enlace | Evento con `defaultPrevented = true` |
  | `chat_store_survives_remount` | Montar, enviar, desmontar y volver a montar | Los mensajes siguen ahí |

## Fuera de alcance

- Persistir conversaciones en disco, exportarlas o tener varias conversaciones en paralelo.
- Abrir enlaces en el navegador del sistema (requiere el plugin opener y su scope).
- Resaltado de sintaxis, botón de copiar en los bloques de código y renderizado LaTeX.
- Editar o regenerar mensajes anteriores.
- Mostrar métricas (`timings`: tokens/s) o el uso de contexto.
- Truncar automáticamente el historial cuando excede `ctx-size`: el error del servidor se muestra tal cual (CA11 d).
- Ajustar `temperature` y `top_p` desde la pantalla Chat (se hace en el perfil, plan de Ejecución).
- Multimodal (`mmproj`), tool calling y varios servidores (MVP §6).
- Usar `POST /v1/chat/completions/control` (`reasoning_end`).

## Riesgos

| Riesgo | Mitigación |
|---|---|
| Cambia el formato de streaming de llama-server (campos del delta, posición de `timings`) | `decode_data` ignora lo desconocido (`Ignore`) y solo depende de `delta.content`, `delta.reasoning_content`, `finish_reason`, `error` y `[DONE]`. Los tests fijan esos contratos. |
| La interfaz real de 008 no coincide con el supuesto `RunningServer` | Lógica pura en `prepare_send` con un tipo propio y un adaptador fino en T5. Si 008 no expone el perfil o `ready`, se escala antes de T5 (Preguntas abiertas). |
| El servidor se cuelga sin cerrar el socket (no llega ningún trozo y no hay EOF) | Detener siempre funciona (`select!` con el token) y Nueva conversación cancela. No se pone timeout de inactividad porque el procesado del prompt puede tardar minutos legítimamente. |
| Rendimiento: re-renderizar el markdown completo por cada token en respuestas largas | El batcher (T8) limita el render a 1 por frame. Si el rendimiento no basta, se evalúa un render incremental en un plan posterior. |
| XSS por `{@html}` | `html: false` + lista blanca de esquemas + imágenes desactivadas + CSP sin orígenes HTTP (CA13). Los tests de T7 incluyen vectores ofuscados. |
| Fuga del registro (una generación que se queda "activa" para siempre si la tarea entra en pánico) | `finish(id)` en un guard `Drop` dentro de la tarea; además se cubre con `registry_finish_frees_slot`. |
| Orden de los eventos entre Channel y la respuesta del `invoke` | La UI no depende de la respuesta de `invoke` para el estado: el ciclo de vida lo marcan solo los eventos del canal, y el `invoke` rechazado se traduce a un `error` (T8). |
| Tests de red flakys | Servidor TCP en `127.0.0.1:0`, secuenciado con canales y sin `sleep` ni reloj real (R3). |

## Preguntas abiertas

1. **Interfaz de 008:** ¿cómo se llama y qué forma tiene el acceso al servidor en marcha desde `src-tauri` (URL base, `ready` y perfil con `temperature`/`top_p`), y el store de estado en la UI? Hay que confirmarlo con el plan 008 antes de empezar T5 y T9.
2. **CSP y testing-library:** ¿el plan 003 ya fija una CSP en `tauri.conf.json` y añade `@testing-library/svelte`? Si es así, T5.5 solo comprueba y T9 no añade la dependencia.
3. **Conversación al cambiar de modelo:** si el usuario para el servidor y lanza otro modelo, ¿se conserva el historial (comportamiento propuesto: sí, con el envío deshabilitado mientras no esté listo) o se vacía automáticamente?
4. **Enlaces:** ¿basta con que no naveguen (propuesto) o se quiere abrirlos en el navegador del sistema dentro del MVP (añadiría `tauri-plugin-opener` con scope `https:`)?
