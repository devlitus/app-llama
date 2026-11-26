# 008 — Ejecución de modelos con llama-server

- **Tipo:** feature
- **Estado:** borrador
- **Rama:** feat/008-ejecucion-servidor
- **Fase del roadmap:** Fase 5 (Ejecución) de [docs/MVP.md](../MVP.md), funcionalidad D

## Objetivo

Que el usuario pueda configurar un perfil por modelo, arrancar y parar `llama-server` desde la pantalla Ejecutar, ver su estado (cargando, listo o error) y sus logs en vivo, y copiar el endpoint OpenAI `http://127.0.0.1:PUERTO/v1` para usarlo desde otras apps, sin que queden procesos huérfanos al cerrar la app.

## Contexto

### Qué se da por hecho

Este plan supone integrados los planes 003 a 007. Se escriben en paralelo, así que estos son los **contratos mínimos** que se esperan de ellos. Si su forma final es distinta, el developer adapta solo la capa de `src-tauri` (T11), nunca `core`.

| Plan | Lo que este plan consume | Forma supuesta |
|---|---|---|
| 003 (esqueleto) | Workspace Rust, crate `core` sin Tauri, `src-tauri/`, `ui/` (Svelte 5 + TS, Vitest), directorio de datos de la app (crate `directories`) | Rutas `crates/core/server/` y `crates/core/config/` como módulos o subcrates según decida 003 |
| 004 (runtime) | Ruta al binario `llama-server` instalado o elegido por el usuario | Una función o comando que devuelve `Option<PathBuf>` |
| 005 | Cliente HTTP (`reqwest` + `tokio`) ya en el workspace | Si no estuviera, T4 lo añade |
| 007 (biblioteca + GGUF) | Lista de modelos locales con id estable, ruta y metadatos | `id: String`, `path: PathBuf` (primera parte si el modelo está dividido), `context_length: Option<u64>`, `block_count: Option<u64>`, `chat_template: Option<String>` |

Para no acoplar `config` al parser GGUF, `config` define su propio `ModelHints { context_length: Option<u32>, block_count: Option<u32>, has_chat_template: bool }` y `src-tauri` hace la traducción desde los tipos de 007.

### llama-server (verificado en el README de `tools/server`, rama `master`, 2026-11-26)

Fuente: <https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md>

| Parámetro del perfil | Flag que usamos | Valores y defecto upstream |
|---|---|---|
| Modelo | `--model FNAME` (`-m`) | — |
| Host | `--host HOST` | defecto `127.0.0.1` |
| Puerto | `--port PORT` | defecto **9931** (ya no 8080) |
| ctx-size | `--ctx-size N` (`-c`) | defecto 0 = el del modelo |
| n-gpu-layers | `--n-gpu-layers N` (`-ngl`, `--gpu-layers`) | número exacto, `auto` o `all`; defecto `auto` |
| threads | `--threads N` (`-t`) | defecto -1 |
| batch-size | `--batch-size N` (`-b`) | defecto 2048 |
| flash-attn | `--flash-attn on\|off\|auto` (`-fa`) | defecto `auto` |
| Plantilla de chat | `--chat-template JINJA_TEMPLATE` | defecto: la de los metadatos del modelo; `--jinja` activado por defecto |
| Temperatura | `--temp N` | defecto 0.80 |
| Top-p | `--top-p N` | defecto 0.95 |

`GET /health` (también `/v1/health`), público:
- **503** con `{"error": {"code": 503, "message": "Loading model", "type": "unavailable_error"}}`: el modelo se está cargando.
- **200** con `{"status": "ok"}`: el modelo está cargado y el servidor está listo.
- El README no documenta otros códigos.

### Decisiones de diseño

1. **Siempre `--host 127.0.0.1` y siempre `--port` explícito.** El endpoint nunca se expone a la red local, y el puerto no depende del defecto upstream, que ha cambiado.
2. **Valores `auto` sin flag.** `n_gpu_layers = Auto`, `flash_attn = Auto` y `threads = None` no generan argumento: es el defecto upstream y así funcionan también los binarios algo más antiguos que el usuario elija en 004, que no aceptaban `auto` ni `-fa` con valor.
3. **Valores por defecto derivados de metadatos** (`default_profile(&ModelHints)`):
   - `ctx_size = min(context_length, 8192)`; 4096 si falta `context_length`. El tope evita que un modelo de 128k contextos reserve una KV cache que no cabe en memoria.
   - `n_gpu_layers = Auto`, y si el usuario fija un número, el máximo admitido es `block_count + 1` (capas más la de salida).
   - `chat_template = None` (la del modelo) si el GGUF trae plantilla, y `Some("chatml")` si no la trae.
   - Resto: `threads = None`, `batch_size = 2048`, `port = 8080` (el del MVP), `flash_attn = Auto`, `temperature = 0.8`, `top_p = 0.95`.
4. **Persistencia:** `profiles.json` en el directorio de datos de la app: `{ "version": 1, "profiles": { "<model_id>": { … } } }`. Escritura atómica (fichero temporal + `rename`). Si el fichero no existe, el almacén está vacío. Si está corrupto, se devuelve un error tipado y **no se sobrescribe**. Los campos que falten se completan con el valor por defecto (`#[serde(default)]`).
5. **Máquina de estados pura** (`transition(state, event)`), sin E/S, separada del supervisor que hace E/S. Estados: `Stopped`, `Starting { port }`, `Ready { port }`, `Stopping`, `Error { kind, message }`.
6. **Arranque:** se comprueba el puerto antes de lanzar. Mientras `/health` devuelve 503, el estado sigue en `Starting` sin límite de tiempo (los modelos grandes tardan minutos y el usuario puede parar). Si `/health` rechaza la conexión durante más de 60 s seguidos (120 sondeos de 500 ms), pasa a `Error`. Si el proceso termina en cualquier momento antes de `Stopping`, pasa a `Error` con el código de salida y las últimas 20 líneas de log.
7. **Tiempo inyectado (R3):** el bucle de sondeo recibe un trait `Sleeper` y cuenta sondeos, no mide tiempo. Los tests usan un `Sleeper` falso que vuelve al instante.
8. **Proceso abstraído:** traits `ProcessSpawner`, `ServerProcess` y `HealthProbe`. La implementación real usa `tokio::process` con `kill_on_drop(true)`, `stdout`/`stderr` en `piped` y, en Windows, `CREATE_NO_WINDOW` para que no se abra una consola. Los tests del ciclo de vida usan un **proceso falso en memoria** (canal de líneas, salida controlable y registro de `kill`).
9. **Parar:** `kill()` y espera la salida hasta 5 s. `llama-server` no guarda estado, así que no hace falta un apagado ordenado con SIGTERM.
10. **Sin huérfanos:**
    - **Windows:** el proceso hijo se asigna a un *Job Object* con `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, cuyo handle vive lo mismo que la app. Si la app muere, incluso desde el Administrador de tareas, el sistema mata al hijo.
    - **Ambos SO:** en `RunEvent::ExitRequested`/`Exit` de Tauri, la app llama a `stop()` y espera (como mucho 5 s) antes de salir. `kill_on_drop` es la última red de seguridad.
    - El caso de cuelgue de la app en macOS queda en Preguntas abiertas.
11. **Logs:** cada línea de `stdout`/`stderr` se guarda en un `LogBuffer` circular en `core` (2000 líneas como máximo; cada línea se trunca a 4096 bytes) y se emite a la UI como evento Tauri `server-log` `{ seq, stream, line }`. La UI tiene su propio búfer acotado al mismo tamaño. El comando `server_logs` devuelve el contenido actual del búfer, para cuando se vuelve a la pantalla.
12. **Un solo servidor:** el supervisor rechaza `start` si el estado no es `Stopped` ni `Error` (`AlreadyRunning`). Los cambios de perfil con el servidor en marcha se aplican en el siguiente arranque, y la UI lo indica.

## Criterios de aceptación

- **CA1** — Dado un modelo con `context_length = 32768`, `block_count = 32` y plantilla de chat, sin perfil guardado, cuando se abre su perfil en Ejecutar, entonces se ve `ctx-size 8192`, `n-gpu-layers auto`, `threads auto`, `batch-size 2048`, `puerto 8080`, `flash-attn auto`, `plantilla: la del modelo`, `temperatura 0.8` y `top-p 0.95`. Con `context_length = 2048`, `ctx-size` es 2048; sin `context_length`, 4096; sin plantilla en el GGUF, `chatml`.
- **CA2** — Dado un perfil modificado y guardado para el modelo A, cuando se cierra y se vuelve a abrir la app, entonces el perfil de A conserva los valores guardados y el modelo B sigue con sus valores por defecto.
- **CA3** — Dado un perfil con un valor fuera de rango (por ejemplo `top-p 1.5`, `puerto 80` o `n-gpu-layers 40` con `block_count = 32`), cuando se pulsa Guardar, entonces no se guarda nada y la UI muestra el error junto a ese campo.
- **CA4** — Dado el perfil del test `args_perfil_completo` (T3), cuando se construyen los argumentos, entonces el vector es exactamente el de la tabla del test, con `--host 127.0.0.1` y `--port` siempre presentes, y sin flags para los valores `auto`.
- **CA5** — Dado un runtime configurado (004) y un modelo de la biblioteca, cuando se pulsa Iniciar, entonces el estado pasa a **Cargando** mientras `/health` responde 503, y a **Listo** cuando responde 200. El endpoint `http://127.0.0.1:PUERTO/v1` y el botón Copiar solo aparecen en Listo.
- **CA6** — Dado el servidor en Cargando o Listo, cuando el proceso termina solo (por ejemplo, un modelo corrupto), entonces el estado pasa a **Error**, con el código de salida y las últimas 20 líneas de log visibles en la pantalla.
- **CA7** — Dado el puerto configurado ocupado por otro proceso, cuando se pulsa Iniciar, entonces no se lanza `llama-server`, el estado pasa a Error con el mensaje "El puerto N está ocupado" y se ofrece el primer puerto libre entre N+1 y N+20.
- **CA8** — Dado que no hay runtime configurado o el binario no existe, cuando se abre Ejecutar, entonces Iniciar está desactivado y se muestra un aviso con enlace a la configuración del runtime. Si el binario desaparece después, `start` devuelve un error tipado `BinaryNotFound` y el estado pasa a Error.
- **CA9** — Dado el servidor arrancado, cuando escribe en `stdout` o `stderr`, entonces cada línea aparece en la consola en el orden en que se emitió dentro de cada stream. Tras 2500 líneas, la consola conserva las 2000 últimas e indica "500 líneas descartadas".
- **CA10** — Dado el servidor en Listo en el puerto 8080, cuando se pulsa Copiar, entonces el portapapeles contiene exactamente `http://127.0.0.1:8080/v1`.
- **CA11** — Dado el servidor en Cargando o en Listo, cuando se pulsa Parar, entonces el proceso termina en 5 s como máximo, el estado pasa a **Detenido** y el puerto vuelve a estar libre.
- **CA12** — Dado el servidor en Listo, cuando se cierra la app de forma normal, entonces 5 s después no queda ningún proceso `llama-server` (Windows: `tasklist /FI "IMAGENAME eq llama-server.exe"`; macOS: `pgrep -x llama-server`). En Windows se cumple también si la app se mata desde el Administrador de tareas.
- **CA13** — Dado un servidor en Cargando o Listo, cuando se intenta iniciar otro (desde la UI o con el comando), entonces Iniciar está desactivado en la UI y el comando `server_start` devuelve `AlreadyRunning` sin lanzar ningún proceso.
- **CA14** — `node scripts/quality-gate.mjs` en verde en Windows y macOS (CI), y todos los tests de este plan sin red externa, sin reloj real y sin aleatoriedad (R3).

## Tareas

### T1 — Perfil de modelo: tipos, validación y valores por defecto
- **Ficheros/módulos:** `crates/core/config/` (`profile.rs`, `error.rs`)
- **Depende de:** — (estructura de 003)
- **Cubre:** CA1, CA3
- **Subtareas:**
  - [ ] T1.1 `ModelProfile { ctx_size: u32, n_gpu_layers: GpuLayers, threads: Option<u32>, batch_size: u32, port: u16, flash_attn: FlashAttn, chat_template: Option<String>, temperature: f32, top_p: f32 }`, con `GpuLayers { Auto, All, Exact(u32) }` y `FlashAttn { Auto, On, Off }`, serializables con serde.
  - [ ] T1.2 `ModelHints` y `default_profile(&ModelHints) -> ModelProfile`, según la decisión 3.
  - [ ] T1.3 `validate(&ModelProfile, &ModelHints) -> Result<(), Vec<FieldError>>`. `FieldError { field, reason }` con `thiserror`. Rangos: `ctx_size` entre 128 y `context_length` (sin tope superior si falta); `Exact(n)` con `n <= block_count + 1` (sin tope si falta); `threads` ≥ 1; `batch_size` entre 1 y 65536; `port` entre 1024 y 65535; `temperature` entre 0.0 y 2.0; `top_p` entre 0.0 y 1.0; `chat_template` no vacío tras `trim` y de un máximo de 64 caracteres (es un nombre de plantilla integrada, ver Preguntas abiertas). Devuelve **todos** los errores, no solo el primero.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `defecto_contexto_largo_se_limita` | `context_length = Some(32768)` | `ctx_size = 8192` |
  | `defecto_contexto_corto_se_respeta` | `context_length = Some(2048)` | `ctx_size = 2048` |
  | `defecto_sin_contexto` | `context_length = None` | `ctx_size = 4096` |
  | `defecto_con_plantilla_usa_la_del_modelo` | `has_chat_template = true` | `chat_template = None` |
  | `defecto_sin_plantilla_usa_chatml` | `has_chat_template = false` | `chat_template = Some("chatml")` |
  | `defecto_resto_de_valores` | hints cualesquiera | `Auto`, `None`, 2048, 8080, `Auto`, 0.8, 0.95 |
  | `valida_perfil_por_defecto` | `default_profile(h)` con los mismos `h` | `Ok(())` |
  | `valida_top_p_fuera_de_rango` | `top_p = 1.5` | `Err` con un `FieldError` en `top_p` |
  | `valida_temperatura_negativa` | `temperature = -0.1` | `Err` en `temperature` |
  | `valida_puerto_privilegiado` | `port = 80` | `Err` en `port` |
  | `valida_puerto_limite_inferior` | `port = 1024` | `Ok(())` |
  | `valida_capas_gpu_sobre_bloques` | `Exact(34)`, `block_count = Some(32)` | `Err` en `n_gpu_layers` |
  | `valida_capas_gpu_en_el_limite` | `Exact(33)`, `block_count = Some(32)` | `Ok(())` |
  | `valida_capas_gpu_sin_bloques` | `Exact(500)`, `block_count = None` | `Ok(())` |
  | `valida_ctx_sobre_el_del_modelo` | `ctx_size = 4097`, `context_length = Some(4096)` | `Err` en `ctx_size` |
  | `valida_ctx_minimo` | `ctx_size = 127` | `Err` en `ctx_size` |
  | `valida_threads_cero` | `threads = Some(0)` | `Err` en `threads` |
  | `valida_batch_cero` | `batch_size = 0` | `Err` en `batch_size` |
  | `valida_plantilla_vacia` | `chat_template = Some("  ")` | `Err` en `chat_template` |
  | `valida_varios_errores_a_la_vez` | `top_p = 2.0`, `port = 1` | `Err` con 2 `FieldError` |
  | `serde_ida_y_vuelta` | perfil con `Exact(20)`, `On`, `Some("llama3")` | serializar y deserializar devuelve el mismo perfil |

### T2 — Almacén de perfiles persistente
- **Ficheros/módulos:** `crates/core/config/` (`store.rs`)
- **Depende de:** T1
- **Cubre:** CA2
- **Subtareas:**
  - [ ] T2.1 `ProfileStore::open(path: PathBuf) -> Result<Self, StoreError>`: fichero inexistente → almacén vacío; JSON inválido o `version` desconocida → `StoreError::Corrupt` sin tocar el fichero.
  - [ ] T2.2 `get(model_id) -> Option<ModelProfile>`, `save(model_id, profile)` y `remove(model_id)`, con escritura atómica (temporal en el mismo directorio + `rename`); crea el directorio padre si no existe.
  - [ ] T2.3 `profile_for(model_id, &ModelHints) -> ModelProfile`: el guardado, o `default_profile` si no hay.
- **Tests unitarios** (todos en `tempfile::TempDir`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `abrir_sin_fichero_da_almacen_vacio` | ruta inexistente | `Ok`, `get("a") == None` |
  | `guardar_y_reabrir_conserva_el_perfil` | `save("a", p)`, nuevo `open` | `get("a") == Some(p)` |
  | `perfiles_independientes_por_modelo` | `save("a", p)` | `get("b") == None` |
  | `guardar_sobrescribe` | `save("a", p1)`, `save("a", p2)` | `get("a") == Some(p2)` |
  | `eliminar_vuelve_al_defecto` | `save("a", p)`, `remove("a")`, `profile_for("a", h)` | `default_profile(h)` |
  | `fichero_corrupto_da_error_y_no_se_toca` | fichero con `{no json` | `Err(Corrupt)`; contenido intacto |
  | `version_desconocida_da_error` | `{"version": 99, "profiles": {}}` | `Err(Corrupt)` |
  | `campo_ausente_toma_defecto` | perfil JSON sin `top_p` | `top_p = 0.95` |
  | `crea_directorio_padre` | ruta `tmp/x/y/profiles.json` | `save` crea `x/y` y el fichero |
  | `no_deja_temporales` | tras `save` | el directorio solo contiene `profiles.json` |
  | `id_con_caracteres_especiales` | id `org/modelo:Q4_K_M` | ida y vuelta correcta |

### T3 — Construcción de argumentos y endpoint
- **Ficheros/módulos:** `crates/core/server/` (`args.rs`)
- **Depende de:** T1
- **Cubre:** CA4, CA10
- **Subtareas:**
  - [ ] T3.1 `build_args(model: &Path, profile: &ModelProfile) -> Vec<OsString>`, en este orden: `--model`, `--host 127.0.0.1`, `--port`, `--ctx-size`, `[--n-gpu-layers]`, `[--threads]`, `--batch-size`, `[--flash-attn]`, `[--chat-template]`, `--temp`, `--top-p`. Los decimales se formatean con `Display` de `f32` (`0.8` → `"0.8"`, `1.0` → `"1"`).
  - [ ] T3.2 `endpoint_url(port: u16) -> String` → `http://127.0.0.1:{port}/v1`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `args_perfil_por_defecto` | `default_profile` (con plantilla), modelo `m.gguf` | `["--model", "m.gguf", "--host", "127.0.0.1", "--port", "8080", "--ctx-size", "8192", "--batch-size", "2048", "--temp", "0.8", "--top-p", "0.95"]` |
  | `args_perfil_completo` | `Exact(20)`, `threads Some(8)`, `batch 512`, `port 9000`, `On`, `Some("chatml")`, `ctx 4096`, `temp 0.2`, `top_p 0.9` | `["--model", "m.gguf", "--host", "127.0.0.1", "--port", "9000", "--ctx-size", "4096", "--n-gpu-layers", "20", "--threads", "8", "--batch-size", "512", "--flash-attn", "on", "--chat-template", "chatml", "--temp", "0.2", "--top-p", "0.9"]` |
  | `args_gpu_all` | `GpuLayers::All` | contiene `"--n-gpu-layers", "all"` |
  | `args_gpu_auto_sin_flag` | `GpuLayers::Auto` | no contiene `--n-gpu-layers` |
  | `args_flash_off` | `FlashAttn::Off` | contiene `"--flash-attn", "off"` |
  | `args_flash_auto_sin_flag` | `FlashAttn::Auto` | no contiene `--flash-attn` |
  | `args_threads_auto_sin_flag` | `threads None` | no contiene `--threads` |
  | `args_ruta_con_espacios_es_un_solo_argumento` | `PathBuf` con `Mis modelos` y `m.gguf` unidos con `join` | el valor de `--model` es un único `OsString` igual a esa ruta |
  | `args_temperatura_entera` | `temp 1.0` | `"--temp", "1"` |
  | `endpoint_con_puerto` | `8080` | `"http://127.0.0.1:8080/v1"` |
  | `endpoint_puerto_maximo` | `65535` | `"http://127.0.0.1:65535/v1"` |

### T4 — Health: parseo y sonda HTTP
- **Ficheros/módulos:** `crates/core/server/` (`health.rs`)
- **Depende de:** — (cliente HTTP de 005; si no existe, añade `reqwest` con `default-features = false`)
- **Cubre:** CA5
- **Subtareas:**
  - [ ] T4.1 `parse_health(status: u16, body: &str) -> HealthStatus { Ready, Loading, Failed(String) }`: 200 → `Ready`; 503 → `Loading`; cualquier otro código → `Failed`, con `error.message` del JSON si existe o `"HTTP <código>"` si no.
  - [ ] T4.2 `trait HealthProbe { async fn probe(&self, port: u16) -> ProbeResult }`, con `ProbeResult { Status(HealthStatus), Unreachable }`. La implementación `HttpHealthProbe` hace `GET http://127.0.0.1:{port}/health` con 2 s de timeout; una conexión rechazada o el timeout dan `Unreachable`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `health_200_ok` | `200`, `{"status": "ok"}` | `Ready` |
  | `health_200_cuerpo_vacio` | `200`, `""` | `Ready` |
  | `health_503_cargando` | `503`, cuerpo del README | `Loading` |
  | `health_503_sin_cuerpo` | `503`, `""` | `Loading` |
  | `health_500_con_mensaje` | `500`, `{"error":{"code":500,"message":"boom","type":"server_error"}}` | `Failed("boom")` |
  | `health_404_sin_json` | `404`, `not found` | `Failed("HTTP 404")` |
  | `sonda_contra_servidor_local_listo` | `TcpListener` de test en `127.0.0.1:0` que responde 200 | `Status(Ready)` |
  | `sonda_contra_servidor_local_cargando` | igual, con 503 | `Status(Loading)` |
  | `sonda_puerto_cerrado` | puerto obtenido de un listener ya cerrado | `Unreachable` |

  Los tests de sonda usan un servidor TCP mínimo en `127.0.0.1` dentro del propio test (no salen a la red, R3).

### T5 — Máquina de estados del servidor
- **Ficheros/módulos:** `crates/core/server/` (`state.rs`)
- **Depende de:** —
- **Cubre:** CA5, CA6, CA11, CA13
- **Subtareas:**
  - [ ] T5.1 `ServerState { Stopped, Starting { port }, Ready { port }, Stopping, Error { kind: ErrorKind, message } }`, con `ErrorKind { PortBusy, BinaryNotFound, SpawnFailed, Exited, Unreachable, HealthFailed }`, serializable para la UI.
  - [ ] T5.2 `ServerEvent { Start { port }, HealthLoading, HealthReady, Fail { kind, message }, ProcessExited { code: Option<i32> }, StopRequested, StopCompleted }`.
  - [ ] T5.3 `transition(&ServerState, ServerEvent) -> Result<ServerState, InvalidTransition>`, pura.
- **Tests unitarios:**
  | Test | Estado inicial | Evento | Resultado esperado |
  |---|---|---|---|
  | `parado_inicia` | `Stopped` | `Start{8080}` | `Starting{8080}` |
  | `error_reinicia` | `Error{..}` | `Start{8080}` | `Starting{8080}` |
  | `cargando_sigue_cargando` | `Starting{8080}` | `HealthLoading` | `Starting{8080}` |
  | `cargando_pasa_a_listo` | `Starting{8080}` | `HealthReady` | `Ready{8080}` |
  | `cargando_falla` | `Starting{8080}` | `Fail{Unreachable, ..}` | `Error{Unreachable, ..}` |
  | `cargando_proceso_termina` | `Starting{8080}` | `ProcessExited{Some(1)}` | `Error{Exited, "…código 1…"}` |
  | `listo_proceso_termina` | `Ready{8080}` | `ProcessExited{None}` | `Error{Exited, ..}` |
  | `cargando_se_para` | `Starting{8080}` | `StopRequested` | `Stopping` |
  | `listo_se_para` | `Ready{8080}` | `StopRequested` | `Stopping` |
  | `parando_completa` | `Stopping` | `StopCompleted` | `Stopped` |
  | `parando_ignora_salida_del_proceso` | `Stopping` | `ProcessExited{Some(1)}` | `Stopping` (salida esperada) |
  | `listo_no_reinicia` | `Ready{8080}` | `Start{9000}` | `Err(InvalidTransition)` |
  | `cargando_no_reinicia` | `Starting{8080}` | `Start{9000}` | `Err(InvalidTransition)` |
  | `parado_no_para` | `Stopped` | `StopRequested` | `Err(InvalidTransition)` |
  | `parado_ignora_health` | `Stopped` | `HealthReady` | `Err(InvalidTransition)` |
  | `fallo_antes_de_lanzar` | `Stopped` | `Fail{PortBusy, ..}` | `Error{PortBusy, ..}` |

### T6 — Búfer de logs acotado
- **Ficheros/módulos:** `crates/core/server/` (`logs.rs`)
- **Depende de:** —
- **Cubre:** CA9
- **Subtareas:**
  - [ ] T6.1 `LogLine { seq: u64, stream: Stream (Stdout|Stderr), text: String }`.
  - [ ] T6.2 `LogBuffer::new(capacity, max_line_bytes)`: `push(stream, text) -> LogLine` (asigna `seq` creciente y trunca en un límite de carácter UTF-8 válido, añadiendo `…`), `snapshot() -> Vec<LogLine>`, `dropped() -> u64`, `tail(n) -> Vec<LogLine>` y `clear()`. Constantes por defecto: 2000 líneas y 4096 bytes.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `conserva_orden_de_insercion` | 3 líneas | `snapshot` en el mismo orden, `seq` 0, 1 y 2 |
  | `descarta_las_mas_antiguas` | capacidad 2000, 2500 líneas | 2000 líneas, la primera con `seq` 500, `dropped() == 500` |
  | `capacidad_exacta_no_descarta` | capacidad 3, 3 líneas | 3 líneas, `dropped() == 0` |
  | `trunca_linea_larga` | línea de 5000 bytes ASCII, límite 4096 | 4096 bytes como máximo, termina en `…` |
  | `trunca_sin_partir_utf8` | `"é"` repetido, límite impar | `String` válido, sin pánico |
  | `linea_vacia_se_guarda` | `""` | se guarda la línea vacía |
  | `tail_devuelve_las_ultimas` | 30 líneas, `tail(20)` | las 20 últimas, en orden |
  | `tail_con_menos_lineas` | 5 líneas, `tail(20)` | las 5 |
  | `clear_mantiene_la_secuencia` | 3 líneas, `clear`, 1 línea | `snapshot` con 1 línea, `seq` 3 |
  | `distingue_streams` | `Stdout` y `Stderr` | cada `LogLine` conserva su `stream` |

### T7 — Detección de puerto ocupado
- **Ficheros/módulos:** `crates/core/server/` (`port.rs`)
- **Depende de:** —
- **Cubre:** CA7
- **Subtareas:**
  - [ ] T7.1 `trait PortChecker { fn is_free(&self, port: u16) -> bool }`. `TcpPortChecker` intenta `std::net::TcpListener::bind(("127.0.0.1", port))` y suelta el listener.
  - [ ] T7.2 `suggest_free_port(start: u16, checker) -> Option<u16>`: el primer puerto libre entre `start + 1` y `start + 20`, sin pasar de 65535.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `puerto_ocupado_se_detecta` | `TcpListener` de test en `127.0.0.1:0`, se consulta su puerto | `is_free == false` |
  | `puerto_liberado_esta_libre` | el mismo puerto tras soltar el listener | `is_free == true` |
  | `sugiere_el_siguiente_libre` | checker falso: ocupados 8081 y 8082 | `suggest_free_port(8080) == Some(8083)` |
  | `sin_puertos_libres_en_rango` | checker falso: todos ocupados | `None` |
  | `no_desborda_en_65535` | `start = 65530`, todos ocupados | `None`, sin pánico; solo consulta de 65531 a 65535 |

### T8 — Abstracción de proceso e implementación con tokio
- **Ficheros/módulos:** `crates/core/server/` (`process.rs`)
- **Depende de:** T6
- **Cubre:** CA6, CA9, CA11
- **Subtareas:**
  - [ ] T8.1 Traits `ProcessSpawner { fn spawn(&self, program: &Path, args: &[OsString]) -> Result<Box<dyn ServerProcess>, SpawnError> }` y `ServerProcess { fn take_output(&mut self) -> mpsc::Receiver<(Stream, String)>; async fn wait(&mut self) -> Option<i32>; async fn kill(&mut self) -> io::Result<()>; fn id(&self) -> Option<u32> }`.
  - [ ] T8.2 `TokioSpawner`: `tokio::process::Command` con `kill_on_drop(true)`, `stdin(null)`, `stdout`/`stderr` en `piped`, y `creation_flags(CREATE_NO_WINDOW)` en `cfg(windows)`. Lee los dos streams línea a línea (con `lines()` y tolerando UTF-8 inválido con `from_utf8_lossy`) y los envía al canal.
  - [ ] T8.3 `SpawnError { NotFound(PathBuf), Io(io::Error) }`: si el binario no existe, `NotFound` antes de intentar lanzarlo.
  - [ ] T8.4 `FakeProcess` y `FakeSpawner` en un módulo `testing` (`#[cfg(test)]` o `feature = "testing"` para usarlos desde T10): líneas guionizadas, salida bajo control del test y registro de `kill()`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `binario_inexistente_da_not_found` | ruta `tmp/no-existe` | `Err(SpawnError::NotFound(ruta))` |
  | `fake_emite_lineas_en_orden` | `FakeProcess` con 3 líneas de `stdout` y 1 de `stderr` | el receptor recibe las 4, en orden dentro de cada stream |
  | `fake_kill_provoca_salida` | `kill()` | `wait()` devuelve `None` y `killed() == true` |
  | `fake_salida_con_codigo` | el test fija la salida `Some(1)` | `wait() == Some(1)` |

  `TokioSpawner` se verifica en T14 (manual) y de forma indirecta con `binario_inexistente_da_not_found`. No se lanzan procesos reales en los tests.

### T9 — Job Object en Windows (sin huérfanos)
- **Ficheros/módulos:** `crates/core/server/` (`platform/windows.rs`, `cfg(windows)`); dependencia `windows-sys` (features `Win32_System_JobObjects`, `Win32_Foundation`, `Win32_System_Threading`)
- **Depende de:** T8
- **Cubre:** CA12
- **Subtareas:**
  - [ ] T9.1 `KillOnCloseJob::new() -> io::Result<Self>`: `CreateJobObjectW` + `SetInformationJobObject` con `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. El handle se cierra en `Drop`.
  - [ ] T9.2 `assign(&self, pid: u32) -> io::Result<()>` (`OpenProcess` + `AssignProcessToJobObject`). `TokioSpawner` asigna cada hijo justo después del `spawn`; si falla, se registra en el log del servidor y el arranque continúa (sigue habiendo `kill_on_drop` y la parada al salir).
  - [ ] T9.3 Es el único `unsafe` del plan: cada bloque `unsafe` lleva un comentario `// SAFETY:` que justifica sus invariantes.
- **Tests unitarios** (`cfg(windows)`; en macOS el módulo no se compila):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `job_se_crea_con_kill_on_close` | `KillOnCloseJob::new()` | `Ok`; `QueryInformationJobObject` devuelve `LimitFlags` con `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` |
  | `assign_pid_inexistente_falla` | `assign(0)` | `Err` |

### T10 — Supervisor del ciclo de vida
- **Ficheros/módulos:** `crates/core/server/` (`supervisor.rs`)
- **Depende de:** T3, T4, T5, T6, T7, T8
- **Cubre:** CA5, CA6, CA7, CA8, CA9, CA11, CA13
- **Subtareas:**
  - [ ] T10.1 `ServerSupervisor::new(spawner, probe, ports, sleeper, sink)`, donde `sink: Arc<dyn ServerSink>` tiene `on_state(&ServerState)` y `on_log(&LogLine)`. `trait Sleeper { async fn sleep(&self, d: Duration) }`. El estado y el `LogBuffer` están detrás de un `tokio::sync::Mutex`.
  - [ ] T10.2 `start(binary, model, profile) -> Result<(), StartError>`: aplica `Start` (o `AlreadyRunning`); comprueba el puerto (`PortBusy` con la sugerencia de T7); lanza el proceso; crea una tarea que pasa las líneas al `LogBuffer` y al `sink`; crea otra que sondea `/health` cada 500 ms (`HealthLoading`/`HealthReady`, y `Fail{Unreachable}` tras 120 sondeos `Unreachable` seguidos) y otra que espera la salida del proceso (`ProcessExited`, con las 20 últimas líneas en el mensaje).
  - [ ] T10.3 `stop() -> Result<(), StopError>`: `StopRequested` → `kill()` → espera como mucho 5 s (10 sondeos del `Sleeper`) → `StopCompleted`; aborta las tareas de sondeo.
  - [ ] T10.4 `state()`, `logs() -> (Vec<LogLine>, dropped)`, y `shutdown()`, que para si hay algo en marcha y no hace nada si no (para la salida de la app).
- **Tests unitarios** (con `FakeSpawner`, una `HealthProbe` falsa con respuestas guionizadas, un `PortChecker` falso, un `Sleeper` instantáneo y un `sink` que registra lo recibido):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `arranque_cargando_y_listo` | sonda: `Loading`, `Loading`, `Ready` | estados recibidos por el sink: `Starting`, `Ready{8080}` |
  | `arranque_pasa_args_de_build_args` | perfil por defecto | el `FakeSpawner` recibe el binario y exactamente `build_args(model, profile)` |
  | `arranque_tolera_unreachable_inicial` | sonda: 5 × `Unreachable`, luego `Ready` | termina en `Ready` |
  | `arranque_unreachable_persistente` | sonda: 120 × `Unreachable` | `Error{Unreachable}`; se llama a `kill()` del proceso |
  | `carga_larga_no_caduca` | sonda: 1000 × `Loading`, luego `Ready` | termina en `Ready` |
  | `health_failed_da_error` | sonda: `Failed("boom")` | `Error{HealthFailed, "boom"}` y `kill()` |
  | `puerto_ocupado_no_lanza` | `PortChecker`: 8080 ocupado y 8081 libre | `Err(PortBusy{port: 8080, suggestion: Some(8081)})`, estado `Error{PortBusy}` y el spawner no se llama |
  | `binario_inexistente` | spawner devuelve `NotFound` | `Err(BinaryNotFound)` y estado `Error{BinaryNotFound}` |
  | `proceso_termina_durante_carga` | el fake sale con `Some(1)` tras emitir 30 líneas | `Error{Exited}`; el mensaje incluye "código 1" y las 20 últimas líneas |
  | `proceso_termina_estando_listo` | `Ready` y luego salida `Some(139)` | `Error{Exited}` |
  | `segundo_arranque_rechazado` | `start` en `Ready` | `Err(AlreadyRunning)`; el spawner se llamó una sola vez |
  | `parar_desde_listo` | `Ready` y `stop()` | `kill()` llamado; estados `Stopping` y `Stopped` |
  | `parar_durante_carga` | sonda siempre `Loading`, `stop()` | `Stopped`; el sondeo se detiene (la sonda no recibe más llamadas tras `Stopped`) |
  | `parar_sin_servidor` | `Stopped` y `stop()` | `Err(NotRunning)` |
  | `shutdown_sin_servidor_es_noop` | `Stopped` y `shutdown()` | `Ok`, ningún evento |
  | `shutdown_con_servidor_para` | `Ready` y `shutdown()` | `kill()` y `Stopped` |
  | `logs_llegan_al_sink_y_al_buffer` | el fake emite 3 líneas | el sink recibe 3 `LogLine` y `logs()` devuelve las mismas |
  | `reinicio_tras_error` | `Error{Exited}` y nuevo `start` | `Starting` y una segunda llamada al spawner |
  | `reinicio_limpia_logs` | segundo arranque | `logs()` solo contiene las líneas del segundo proceso; `seq` sigue creciendo |

### T11 — Integración en Tauri: comandos, eventos y cierre de la app
- **Ficheros/módulos:** `src-tauri/src/commands/server.rs`, `src-tauri/src/commands/profile.rs`, `src-tauri/src/lib.rs` (registro y `RunEvent`), `src-tauri/capabilities/`
- **Depende de:** T2, T10 (y las interfaces de 004 y 007)
- **Cubre:** CA2, CA3, CA5–CA9, CA11–CA13
- **Subtareas:**
  - [ ] T11.1 Estado gestionado: `ServerSupervisor` con `TokioSpawner`, `HttpHealthProbe`, `TcpPortChecker`, un `Sleeper` de tokio y un `TauriSink` que emite `server-status` (`ServerState`) y `server-log` (`LogLine`); y `ProfileStore` en `<datos de la app>/profiles.json`.
  - [ ] T11.2 Comandos: `profile_get(model_id) -> ModelProfile`, `profile_save(model_id, profile) -> Result<(), Vec<FieldError>>`, `profile_reset(model_id) -> ModelProfile`, `server_start(model_id) -> Result<(), StartErrorDto>`, `server_stop()`, `server_status() -> ServerState`, `server_logs() -> LogsDto { lines, dropped }` y `server_endpoint() -> Option<String>` (solo en `Ready`).
  - [ ] T11.3 `hints_from_library(&LocalModel) -> ModelHints` (adaptador de los tipos de 007) y búsqueda del binario de 004; sin runtime → `StartErrorDto::NoRuntime`.
  - [ ] T11.4 En `RunEvent::ExitRequested`, `block_on(supervisor.shutdown())` con un timeout de 5 s antes de dejar salir a la app.
  - [ ] T11.5 Los errores se serializan como `{ kind, message, field? }` estables para la UI.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `hints_desde_modelo_completo` | `LocalModel` con `context_length = 32768`, `block_count = 32` y plantilla | `ModelHints { Some(32768), Some(32), true }` |
  | `hints_contexto_fuera_de_u32` | `context_length = u64::MAX` | `context_length = Some(u32::MAX)` (saturado) |
  | `hints_sin_metadatos` | todo `None` | `ModelHints { None, None, false }` |
  | `start_error_dto_puerto_ocupado` | `StartError::PortBusy{8080, Some(8081)}` | JSON `{"kind":"portBusy","port":8080,"suggestion":8081,…}` |
  | `start_error_dto_sin_runtime` | `NoRuntime` | `{"kind":"noRuntime",…}` |
  | `server_state_serializa_para_ui` | `Ready{8080}` | `{"state":"ready","port":8080}` |

### T12 — UI: cliente, stores y lógica de presentación
- **Ficheros/módulos:** `ui/src/lib/server/api.ts`, `ui/src/lib/server/logStore.ts`, `ui/src/lib/server/status.ts`
- **Depende de:** T11
- **Cubre:** CA5, CA9, CA10, CA13
- **Subtareas:**
  - [ ] T12.1 `api.ts`: wrappers tipados de `invoke` para los comandos de T11 y de `listen` para `server-status` y `server-log`.
  - [ ] T12.2 `logStore.ts`: store de Svelte 5 acotado a 2000 líneas, con `dropped`, `hydrate(snapshot)` (al entrar en la pantalla) y `append(line)`, que descarta los `seq` ya vistos (por la carrera entre el snapshot y los eventos).
  - [ ] T12.3 `status.ts`: `statusView(state) -> { label, tone, canStart, canStop, endpoint }` (Detenido / Cargando / Listo / Parando / Error) y `endpointUrl(port)`.
- **Tests unitarios (Vitest):**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `log_store_acotado` | 2500 `append` | 2000 líneas, `dropped === 500` |
  | `log_store_ignora_seq_duplicados` | `hydrate([0..9])` y `append(seq 5)` | 10 líneas |
  | `log_store_hydrate_reemplaza` | `hydrate` dos veces | solo el último snapshot |
  | `status_detenido` | `{state:"stopped"}` | `canStart` true, `canStop` false, `endpoint` null |
  | `status_cargando` | `{state:"starting",port:8080}` | label "Cargando", `canStart` false, `canStop` true, `endpoint` null |
  | `status_listo_con_endpoint` | `{state:"ready",port:8080}` | label "Listo", `endpoint === "http://127.0.0.1:8080/v1"` |
  | `status_error_permite_reiniciar` | `{state:"error",kind:"exited",message:"…"}` | `canStart` true, `canStop` false, `tone` "error" |
  | `status_parando` | `{state:"stopping"}` | `canStart` false, `canStop` false |

### T13 — UI: pantalla Ejecutar
- **Ficheros/módulos:** `ui/src/routes/run/` (o la ruta que defina 003), `ui/src/lib/server/ProfileForm.svelte`, `ui/src/lib/server/LogConsole.svelte`, `ui/src/lib/server/EndpointBox.svelte`
- **Depende de:** T12
- **Cubre:** CA1, CA3, CA5–CA10, CA13
- **Subtareas:**
  - [ ] T13.1 Selector de modelo (lista de la biblioteca de 007) y `ProfileForm` con los 9 campos. `n-gpu-layers`: `auto` / `all` / número. `threads`: `auto` / número. `flash-attn`: `auto` / `on` / `off`. Plantilla: "La del modelo" y una lista de nombres integrados (`chatml`, `llama3`, `mistral-v7`, `gemma`, `phi4`, `deepseek3`). Botones Guardar y Restablecer, y errores por campo devueltos por `profile_save`.
  - [ ] T13.2 Iniciar y Parar según `statusView`; insignia de estado; en Error, el mensaje y, si es `portBusy` con sugerencia, un botón "Usar puerto N" que rellena el campo. Aviso de "sin runtime" con enlace a Ajustes. Aviso "Los cambios se aplicarán al reiniciar el servidor" si se guarda el perfil con el servidor en marcha.
  - [ ] T13.3 `EndpointBox`: URL en solo lectura y botón Copiar (`navigator.clipboard.writeText`), con la confirmación "Copiado".
  - [ ] T13.4 `LogConsole`: fuente monoespaciada, `stderr` diferenciado, desplazamiento automático salvo que el usuario haya subido, contador de descartadas y botón "Limpiar vista" (solo vacía la vista, no el búfer del backend).
- **Tests unitarios (Vitest + Testing Library, `invoke` y `clipboard` simulados):**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `formulario_muestra_defectos` | `profile_get` devuelve el perfil por defecto | los campos muestran 8192, auto, auto, 2048, 8080, auto, "La del modelo", 0.8 y 0.95 |
  | `formulario_muestra_error_de_campo` | `profile_save` rechaza con `[{field:"top_p"}]` | mensaje de error junto a top-p |
  | `iniciar_desactivado_sin_runtime` | `server_start` no llamado; estado sin runtime | botón Iniciar `disabled` y aviso visible |
  | `copiar_endpoint` | estado `ready` en 8080, clic en Copiar | `writeText("http://127.0.0.1:8080/v1")` |
  | `endpoint_oculto_si_no_listo` | estado `starting` | no hay `EndpointBox` |
  | `consola_muestra_descartadas` | store con `dropped` 500 | texto "500 líneas descartadas" |
  | `usar_puerto_sugerido` | error `portBusy` con `suggestion` 8081, clic | el campo puerto vale 8081 |

### T14 — Verificación manual en Windows y macOS
- **Ficheros/módulos:** ninguno (el resultado se anota en la memoria del plan)
- **Depende de:** T13
- **Cubre:** CA5, CA6, CA7, CA11, CA12
- **Subtareas:**
  - [ ] T14.1 Con un modelo pequeño real (≤ 1 GB), arrancar y comprobar Cargando → Listo, y `curl http://127.0.0.1:8080/v1/models` desde fuera de la app.
  - [ ] T14.2 Ocupar el 8080 con otro proceso y comprobar CA7.
  - [ ] T14.3 Parar y comprobar con `tasklist` / `pgrep` que no queda ningún proceso en 5 s como máximo (CA11).
  - [ ] T14.4 Cerrar la app con el servidor en Listo y comprobar CA12. En Windows, repetirlo matando la app desde el Administrador de tareas.
  - [ ] T14.5 Comprobar que en Windows no se abre ninguna ventana de consola al iniciar.
- **Tests unitarios:** no aplica (verificación manual; los tests automáticos están en T1–T13).

## Fuera de alcance

- Varios servidores a la vez (MVP §6).
- Exponer el servidor a la red local (`--host 0.0.0.0`), API key (`--api-key`) y TLS.
- Plantilla de chat libre (texto Jinja o `--chat-template-file`): solo se ofrecen "la del modelo" y una lista de nombres integrados.
- Parámetros avanzados (`--parallel`, `--mlock`, `--cache-type-k`, `--rope-*`, multimodal `mmproj`, etc.).
- Reiniciar el servidor en caliente al cambiar el perfil.
- Persistir los logs en un fichero o exportarlos.
- Recoger procesos huérfanos tras un cuelgue de la app en macOS (ver Preguntas abiertas).
- Chat (Fase 6, plan propio).
- Estimación de VRAM para elegir `n-gpu-layers` de forma automática.

## Riesgos

| Riesgo | Mitigación |
|---|---|
| El usuario elige en 004 un binario antiguo que no acepta `--flash-attn on\|off` ni `--n-gpu-layers all` | Los valores `auto` no generan flag (decisión 2); si el proceso termina por argumentos inválidos, CA6 muestra el log con la causa |
| Los flags de llama.cpp cambian en futuras versiones (el puerto por defecto ya pasó de 8080 a 9931) | Puerto y host siempre explícitos; flags centralizados en `build_args` con tests de tabla; README verificado en la fecha del plan |
| Carrera entre la comprobación del puerto y el `bind` de `llama-server` | Si otro proceso lo ocupa entre medias, `llama-server` termina y CA6 lo muestra como Error con el log |
| Inundación de eventos `server-log` con logs muy verbosos | Búfer acotado en backend y UI; si en T14 se ve lentitud, agrupar las líneas en lotes en un plan posterior |
| En macOS, si la app se cuelga o muere con `kill -9`, `llama-server` queda huérfano | Fuera de alcance de este plan; propuesta en Preguntas abiertas |
| `unsafe` del Job Object en Windows | Confinado a `platform/windows.rs`, con comentarios `// SAFETY:` y tests `cfg(windows)` en la CI de Windows |
| `navigator.clipboard` falla en algún webview | La URL está en un campo de solo lectura que se puede seleccionar y copiar a mano; si falla en T14, se cambia a `tauri-plugin-clipboard-manager` |
| Las interfaces finales de 004 y 007 difieren de las supuestas | `core` define sus propios tipos (`ModelHints`, `Path` del binario); solo cambia el adaptador de T11.3 |
| Un modelo dividido (`-00001-of-0000N.gguf`) | Se pasa la ruta de la primera parte (contrato de 007); llama.cpp carga el resto solo |

## Preguntas abiertas

1. **Tope de `ctx-size` por defecto:** se propone `min(context_length, 8192)`. ¿Vale 8192 o preferís otro valor (4096 o 16384)?
2. **Puerto por defecto:** el MVP dice 8080, pero upstream ha pasado a 9931. Se propone 8080 (más conocido para clientes OpenAI locales). ¿Confirmáis?
3. **Huérfanos en macOS tras un cuelgue de la app:** ¿basta con cubrir el cierre normal (CA12) o queréis un plan aparte con fichero de PID y limpieza al arrancar (comprobando el nombre del proceso para no matar uno ajeno)?
4. **Plantilla de chat:** ¿basta con "la del modelo" más una lista cerrada de nombres integrados, o hace falta un campo de texto libre ya en el MVP?
5. **Contratos de 004 y 007:** los nombres concretos (función del runtime, `LocalModel` y sus campos) se ajustarán cuando esos planes estén aprobados; si cambian mucho, habrá que retocar T11.
