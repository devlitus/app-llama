# 006 — Gestor de descargas (cola, progreso, reanudación y verificación)

- **Tipo:** feature
- **Estado:** borrador
- **Rama:** feat/006-gestor-descargas
- **Fase del roadmap:** 3. Descargas ([docs/MVP.md](../MVP.md), §2.B y §4)

## Objetivo

Que el usuario pueda descargar modelos GGUF de Hugging Face de cualquier tamaño (también de más de 50 GB y divididos en partes) desde una cola. Verá el progreso con bytes, velocidad y ETA, y podrá pausar, reanudar (también tras cerrar la app) y cancelar. Ningún fichero se da por bueno sin verificar su tamaño y su SHA256.

## Contexto

### Precondiciones (se dan por hechas)

- **003 — Esqueleto:** workspace Cargo con `crates/core` (sin Tauri), `src-tauri/` (Tauri 2) y `ui/` (Svelte 5 + TypeScript, con `lint`, `check` y `test` con Vitest). El quality gate ya ejecuta los pasos de Rust y de UI.
- **004 — Runtime llama.cpp:** descarga la release de llama.cpp con una descarga simple (sin reanudación ni cola). Este plan ofrece una fachada `download_one` y migra ese punto de llamada (T15).
- **005 — Explorador HF:** cliente de la API de HF en `crates/core/src/hf/`, token de HF en el llavero (`keyring`) y botón "Descargar" en la ficha del modelo, sin lógica detrás. Ese botón es el punto de integración (T14).

> Este plan no puede leer el código de 003–005 porque se escriben en paralelo. Los nombres concretos de módulos de esos planes (`hf::Client`, el proveedor del token, la llamada de descarga de 004) se ajustan a lo que exista cuando empiece cada tarea. Lo que no se puede ajustar sin decidir algo va a **Preguntas abiertas**.

### Ubicación del código

- **Lógica:** módulo `crates/core/src/download/` del crate `core`, según la arquitectura de [docs/MVP.md](../MVP.md) §3 ("crates/core/download"). No depende de Tauri (R7). Si 003 hubiera creado un crate por módulo, se usa `crates/download/` con la misma estructura interna (ver Preguntas abiertas).
- **Puente:** `src-tauri/src/downloads.rs`, con los comandos `#[tauri::command]` y la implementación de `EventSink` sobre `AppHandle::emit`.
- **UI:** `ui/src/lib/downloads/` (store, formato y componentes) y la vista "Descargas".

### APIs externas verificadas (2026-11-26)

- **Árbol del repositorio:** `GET https://huggingface.co/api/models/{repo}/tree/{rev}` devuelve, por fichero, `{ type, oid, size, path }`. Los ficheros LFS (todos los `.gguf`) añaden `lfs: { oid, size, pointerSize }` y `xetHash`. **`lfs.oid` es el SHA256 en hex (64 caracteres) del contenido**, y `lfs.size` es su tamaño en bytes. Ejemplo real de `Qwen/Qwen2.5-0.5B-Instruct-GGUF`, fichero `qwen2.5-0.5b-instruct-fp16.gguf`: `size: 1266425696`, `lfs.oid: "8e0ae260…cf3fc"`, `lfs.size: 1266425696`. El `oid` de primer nivel es el SHA1 del blob git (el puntero), **no** sirve para verificar. Sin `lfs`, no hay SHA256 publicado y solo se verifica el tamaño (R6: "SHA256 cuando HF lo publica").
- **Descarga:** `GET https://huggingface.co/{repo}/resolve/{rev}/{path}`. Para LFS/Xet responde con una redirección 302 a una URL firmada y temporal de una CDN de otro host (cdn-lfs / cas-bridge.xethub). La CDN admite `Range: bytes=N-` y responde `206` con `Content-Range: bytes N-M/TOTAL`. La firma caduca, así que **cada intento (y cada reanudación) vuelve a pedir `/resolve/`** y nunca se persiste la URL de la CDN.
- **Gated / privados:** sin token, o con un token sin acceso, `/resolve/` responde `401` o `403`. Con `Authorization: Bearer <token>` responde la redirección. La CDN no necesita el token. `reqwest` quita `Authorization` al seguir una redirección a otro host (se comprueba con un test en T9).
- **Revisión:** para que un fichero no cambie entre la pausa y la reanudación, `rev` es el **commit `sha`** del modelo (la ficha de `GET /api/models/{repo}` lo trae) y no `main`. Si 005 no lo expone, se usa `main` (ver Preguntas abiertas).

### Diseño

1. **Unidad de descarga (`DownloadUnit`):** un modelo, que puede tener varios ficheros (las partes `-0000k-of-0000N.gguf`). Tiene un `DownloadId` (u64 creciente, persistido), el `repo` y un `Vec<FileSpec>`. Cada `FileSpec` lleva `url`, `relative_path`, `expected_size: u64`, `sha256: Option<String>` y su estado (`pending | done_unverified | verified | preexisting`). La unidad tiene un estado único: `Queued | Downloading | Verifying | Paused | Completed | Failed { kind, message } | Cancelled`.
2. **Ficheros en disco:** cada fichero se escribe en `<dest>/<relative_path>.part`. **La longitud del `.part` es la única fuente de verdad del desplazamiento**: el offset no se persiste. Cuando todas las partes de la unidad están verificadas, se renombran todas a su nombre final, en orden. Así una unidad dividida nunca deja a la vista un conjunto incompleto de `.gguf`.
3. **Transferencia de un fichero:** `GET` con `Range: bytes=<len(.part)>-` si el offset es mayor que 0, y escritura en modo append.
   - `206` con `Content-Range` que empieza en el offset y cuyo total es `expected_size`: se continúa.
   - `200`: el servidor ignora el `Range`, así que se trunca el `.part` y se empieza de 0.
   - `416` con `.part` de tamaño igual a `expected_size`: el fichero ya está completo.
   - `416` en cualquier otro caso: se trunca el `.part` y se reinicia.
   - `206` con un inicio o un total incoherentes: error `UnexpectedContentRange` y se reinicia desde 0 una sola vez.
   - `401`/`403`: `Unauthorized`/`Gated`, sin reintento. `404`: `NotFound`, sin reintento.
   - `5xx`, `429`, conexión cortada o timeout de lectura: error transitorio, con reintento.
4. **Reintentos:** hasta 5 intentos consecutivos **sin progreso** por fichero, con esperas de 1, 2, 4 y 8 s entre ellos mediante un `Sleeper` inyectado. Si un intento escribe bytes, el contador vuelve a 0. Agotados los intentos: `Failed { kind: Network }`. El `.part` se conserva, así que "Reintentar" continúa desde donde iba.
5. **Verificación:** al terminar cada fichero, se comprueba que `len(.part) == expected_size` y, si hay `sha256`, se calcula el SHA256 del `.part` en streaming (bloques de 1 MiB) y se compara sin distinguir mayúsculas. La unidad está en `Verifying` y emite el progreso de la verificación. No se guarda el estado del hash entre reanudaciones: se hashea entero al final. Para 50 GB son unos minutos, lo que es aceptable. Si el SHA256 no coincide: se borra ese `.part` y la unidad pasa a `Failed { kind: ChecksumMismatch }`.
6. **Progreso:** `ProgressTracker` con un `Clock` inyectado. Calcula la velocidad sobre una **ventana deslizante de 5 s**, y la ETA como `(total - hechos) / velocidad` (`None` si la velocidad es 0). `Throttle` emite **como máximo 1 evento de progreso cada 250 ms por unidad** (4 Hz). Los cambios de estado se emiten siempre y al momento, sin limitar.
7. **Espacio en disco:** antes de pasar a `Downloading`, se calcula `necesario = Σ (expected_size − len(.part))` de los ficheros no verificados, más un margen fijo de **1 GiB**. Si `DiskSpace::available(dest)` es menor que eso: `Failed { kind: InsufficientSpace { needed, available } }` sin abrir ninguna conexión. Se usa `u64` en toda la cadena.
8. **Cola:** FIFO con un máximo de **1 unidad activa** a la vez (parámetro del constructor). Las partes de una unidad se descargan en orden. Órdenes:
   - **Pausar:** cancela la transferencia y conserva los `.part`. → `Paused`.
   - **Reanudar:** → `Queued`.
   - **Cancelar:** aborta y borra los `.part` y los ficheros finales que haya creado la unidad (nunca los `preexisting`). → `Cancelled`.
   - **Reintentar:** desde `Failed`. → `Queued`.
   - **Quitar de la lista:** solo en estado terminal.

   La cancelación usa `tokio_util::sync::CancellationToken`.
9. **Persistencia:** `downloads.json` en el directorio de datos de la app, con `schema: 1`, `next_id` y las unidades (sin bytes, sin URL de CDN y **sin token**). Se escribe de forma atómica (fichero temporal + rename) en cada cambio de estado, nunca por bytes.
   - Al arrancar, las unidades `Downloading`/`Verifying`/`Queued` se restauran como `Paused` (ver Preguntas abiertas sobre la reanudación automática).
   - Si el JSON está corrupto o su `schema` es desconocido, se renombra a `downloads.json.corrupt` y se arranca con la cola vacía.
10. **Token:** trait `TokenProvider` (`fn token(&self) -> Option<String>`), que se implementa en `src-tauri` con el llavero de 005. La cabecera `Authorization` solo se añade si el host de la URL está en `AuthPolicy.hosts` (por defecto `["huggingface.co"]`). Así el token no se manda nunca a otros hosts. El token no aparece en logs, errores, eventos ni en `downloads.json`.
11. **Seguridad de rutas:** `relative_path` viene de HF. Se rechaza si es absoluta, si tiene componentes `..`, prefijos de unidad (`C:`) o raíz, o si está vacía (`UnsafePath`). Se construye con `Path::join` por componente (R3).
12. **Eventos Tauri:** `downloads:state` con payload `DownloadSnapshot` (la unidad completa) y `downloads:progress` con payload `{ id, downloaded_bytes, total_bytes, bytes_per_sec, eta_secs: number | null, phase: "download" | "verify" }`.

### Dependencias nuevas

`core`: `reqwest` (features `stream`, `rustls-tls`, sin las features por defecto), `tokio` (`fs`, `io-util`, `time`, `sync`, `macros`, `rt`), `tokio-util`, `futures-util`, `sha2`, `hex`, `serde`, `serde_json`, `thiserror`, `fs4` (espacio libre) y `url`. `dev-dependencies`: `wiremock` y `tempfile`. `Cargo.lock` versionado (R3).

## Criterios de aceptación

- **CA1 — Progreso.** Dado un fichero de 10 MiB servido por un servidor HTTP de prueba y un reloj falso que avanza 50 ms por bloque de 64 KiB, cuando se descarga, entonces:
  - se emiten como máximo `ceil(duración / 250 ms) + 2` eventos de progreso;
  - el último evento indica `downloaded_bytes == total_bytes == 10485760`;
  - `bytes_per_sec` coincide con la velocidad simulada (±1 %);
  - `eta_secs` es `0` al final.
- **CA2 — Reanudación con `Range`.** Dado un `.part` con los primeros 4 MiB de un fichero de 10 MiB, cuando se reanuda, entonces la petición lleva `Range: bytes=4194304-`, solo se reciben los 6 MiB restantes y el fichero final es idéntico byte a byte al original.
- **CA3 — Cortes de red.** Dado un servidor que cierra la conexión tras enviar 1 MiB en cada respuesta, cuando se descarga un fichero de 3 MiB, entonces la descarga termina con 3 intentos encadenados por `Range`, sin intervención, y el SHA256 coincide. Dado un servidor que corta antes de enviar ningún byte 5 veces seguidas, la unidad acaba en `Failed { kind: Network }` y el `.part` se conserva.
- **CA4 — Servidor sin `Range`.** Dado un `.part` de 4 MiB y un servidor que responde `200` ignorando el `Range`, cuando se reanuda, entonces el `.part` se trunca, se descarga desde 0 y el resultado es correcto.
- **CA5 — Pausa, reanudación y cancelación.**
  - Dada una descarga en curso, cuando se pausa, entonces en ≤ 1 bloque deja de escribirse, el estado es `Paused` y el `.part` se conserva.
  - Cuando se reanuda, continúa desde `len(.part)`.
  - Cuando se cancela, el estado es `Cancelled` y no queda ningún `.part` ni ningún fichero final de esa unidad. Los ficheros `preexisting` siguen intactos.
- **CA6 — Modelos divididos.**
  - Dado un modelo en 3 partes, cuando se encola cualquiera de ellas desde la UI, entonces se encola una única unidad con las 3, y ningún `.gguf` final aparece en disco hasta que las 3 estén verificadas.
  - El progreso agregado cubre la suma de los 3 tamaños.
  - Si falta una parte en la especificación, se rechaza con `IncompleteSplit`.
- **CA7 — Espacio en disco.** Dado un modelo de 60 GiB (64424509440 bytes) y un `DiskSpace` falso con 40 GiB libres, cuando se va a iniciar, entonces la unidad pasa a `Failed { InsufficientSpace { needed: 60 GiB + 1 GiB, available: 40 GiB } }` y el servidor de prueba recibe 0 peticiones.
- **CA8 — Verificación.**
  - Dado un fichero con el `sha256` correcto, termina en `Completed` y se renombra sin `.part`.
  - Con un `sha256` incorrecto, termina en `Failed { ChecksumMismatch }` y no queda ni el fichero final ni el `.part`.
  - Con un tamaño recibido distinto de `expected_size`, termina en `Failed { SizeMismatch }`.
  - Sin `sha256` (fichero no LFS), solo se verifica el tamaño.
- **CA9 — Gated.**
  - Dado un repositorio que responde `401` sin token, termina en `Failed { Unauthorized }` sin reintentos (1 petición).
  - Con `403`, termina en `Failed { Gated }`.
  - Con token, la petición a `huggingface.co` (host de prueba configurado) lleva `Authorization: Bearer …`, y la petición tras la redirección a otro host no la lleva.
  - El token no aparece en `downloads.json` ni en los mensajes de error.
- **CA10 — Persistencia.** Dada una cola con una unidad `Downloading` y otra `Queued`, cuando se recrea el gestor desde `downloads.json`, entonces las dos aparecen como `Paused` con sus ficheros y su `next_id`. Al reanudar la primera, continúa desde `len(.part)`. Un `downloads.json` corrupto se renombra a `.corrupt` y la cola arranca vacía sin pánico.
- **CA11 — Cola.** Dadas 3 unidades encoladas con un máximo de 1 activa, cuando se procesan, entonces se descargan en orden FIFO y nunca hay 2 en `Downloading` a la vez. Si se pausa la activa, empieza la siguiente.
- **CA12 — UI.**
  - Dada la vista "Descargas", cuando llegan eventos `downloads:state` y `downloads:progress`, entonces cada unidad muestra el nombre, la barra de progreso, los bytes (`1,2 GB de 4,5 GB`), la velocidad (`12,3 MB/s`), la ETA (`3 min 20 s`) y los botones válidos para su estado (Pausar, Reanudar, Cancelar, Reintentar, Quitar).
  - El botón "Descargar" de la ficha de 005 encola la descarga y lleva a la vista.
  - Los tests de Vitest de formato y del store pasan.
- **CA13 — Migración del runtime.** Dado el flujo de instalación de llama.cpp de 004, cuando se instala, entonces usa `download::download_one` (reanudación y verificación de tamaño) y sus tests siguen en verde.
- **CA14 — Comprobación en vivo** (manual, T16). En Windows y en macOS, la descarga real de `Qwen/Qwen2.5-0.5B-Instruct-GGUF` / `qwen2.5-0.5b-instruct-q4_k_m.gguf`, con una pausa a mitad, cerrar la app, abrirla y reanudar, termina en `Completed` con el SHA256 igual a su `lfs.oid`.
- **CA15 — Gate.** `node scripts/quality-gate.mjs` en verde, sin red en los tests (todos los HTTP contra `127.0.0.1`), sin reloj real ni `sleep` real.

## Tareas

> Convención de tests: todos los tests de red usan `wiremock` o el servidor TCP de `test_support` en `127.0.0.1` con puerto efímero. El tiempo se inyecta (`FakeClock`, `FakeSleeper`) y los ficheros van en `tempfile::TempDir` (R3).

### T1 — Tipos de dominio, errores y rutas seguras
- **Ficheros/módulos:** `crates/core/src/download/mod.rs`, `model.rs`, `error.rs`, `paths.rs`; `crates/core/src/lib.rs`; `crates/core/Cargo.toml`
- **Depende de:** —
- **Cubre:** CA8, CA9 (tipos), base de todas
- **Subtareas:**
  - [ ] T1.1 `DownloadId`, `FileSpec`, `FileStatus`, `DownloadUnit`, `UnitState`, `FailureKind` (`Network`, `Unauthorized`, `Gated`, `NotFound`, `ChecksumMismatch`, `SizeMismatch`, `InsufficientSpace { needed, available }`, `UnexpectedContentRange`, `Io`, `UnsafePath`, `IncompleteSplit`, `DestinationExists`), todos con `serde`.
  - [ ] T1.2 `DownloadError` con `thiserror`. Su `Display` nunca incluye cabeceras ni el token.
  - [ ] T1.3 `safe_join(base, relative) -> Result<PathBuf, DownloadError>` y `part_path(final) -> PathBuf` (añade `.part`).
  - [ ] T1.4 Transiciones válidas: `UnitState::can_transition(to) -> bool`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `safe_join_ruta_simple` | base `tmp`, `"a/b.gguf"` | `tmp/a/b.gguf` construido por componentes |
  | `safe_join_rechaza_padre` | `"../x.gguf"` | `Err(UnsafePath)` |
  | `safe_join_rechaza_padre_intermedio` | `"a/../../x"` | `Err(UnsafePath)` |
  | `safe_join_rechaza_absoluta` | `"/etc/x"` y `"C:\\x"` | `Err(UnsafePath)` en los dos |
  | `safe_join_rechaza_vacia` | `""` | `Err(UnsafePath)` |
  | `part_path_anade_sufijo` | `m.gguf` | `m.gguf.part` |
  | `transicion_queued_a_downloading` | `Queued → Downloading` | `true` |
  | `transicion_completed_es_terminal` | `Completed → Queued` | `false` |
  | `transicion_failed_a_queued` | `Failed → Queued` (reintentar) | `true` |
  | `unit_state_serde_ida_vuelta` | `Failed { InsufficientSpace { needed: 65498251264, available: 1 } }` | JSON y vuelta iguales, `u64` sin pérdida |
  | `error_display_sin_token` | `Unauthorized` creado desde una petición con token `hf_abc` | `to_string()` no contiene `hf_abc` |

### T2 — Modelos divididos como una unidad
- **Ficheros/módulos:** `crates/core/src/download/split.rs`
- **Depende de:** T1
- **Cubre:** CA6
- **Subtareas:**
  - [ ] T2.1 `parse_split(name) -> Option<SplitInfo { stem, index, total }>` para `^(.*)-(\d{5})-of-(\d{5})\.gguf$`.
  - [ ] T2.2 `expand_split(selected, all_paths) -> Result<Vec<String>, DownloadError>`: dado un fichero seleccionado y la lista del árbol, devuelve todas las partes ordenadas, o `IncompleteSplit` si falta alguna. Un fichero sin partes devuelve solo ese.
  - [ ] T2.3 `validate_unit_files(files)`: rechaza unidades con partes incompletas, duplicadas o mezcladas de dos `stem`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `parse_split_valido` | `"m-Q4_K_M-00002-of-00003.gguf"` | `Some { stem: "m-Q4_K_M", index: 2, total: 3 }` |
  | `parse_split_no_dividido` | `"m-Q4_K_M.gguf"` | `None` |
  | `parse_split_indice_cero` | `"m-00000-of-00003.gguf"` | `None` |
  | `parse_split_indice_mayor_que_total` | `"m-00004-of-00003.gguf"` | `None` |
  | `expand_split_desde_parte_media` | seleccionada parte 2, árbol con 1..3 desordenadas | `[p1, p2, p3]` |
  | `expand_split_falta_parte` | árbol con 1 y 3 de 3 | `Err(IncompleteSplit)` |
  | `expand_split_en_subcarpeta` | `"Q8_0/m-00001-of-00002.gguf"` | partes de esa subcarpeta, no de otras |
  | `expand_no_dividido` | `"m.gguf"` | `["m.gguf"]` |
  | `validate_rechaza_stems_mezclados` | partes de `a` y de `b` | `Err(IncompleteSplit)` |
  | `validate_rechaza_duplicados` | `p1, p1, p2` de 2 | `Err(IncompleteSplit)` |

### T3 — Reloj, progreso (velocidad, ETA) y limitador de eventos
- **Ficheros/módulos:** `crates/core/src/download/clock.rs`, `progress.rs`
- **Depende de:** T1
- **Cubre:** CA1
- **Subtareas:**
  - [ ] T3.1 Trait `Clock { fn now(&self) -> Duration }` (monotónico), `SystemClock` (`Instant`) y `FakeClock` (solo en `cfg(test)` o en el módulo `test_support`).
  - [ ] T3.2 `ProgressTracker::new(total, already_done, clock)`, `record(bytes)` y `snapshot() -> Progress { downloaded, total, bytes_per_sec, eta_secs }`. Ventana deslizante de 5 s. Los bytes `already_done` (reanudación) no cuentan para la velocidad.
  - [ ] T3.3 `Throttle::new(interval = 250 ms, clock)` y `should_emit() -> bool`. `force()` para el evento final.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `velocidad_constante` | 1 MiB cada 100 ms durante 2 s | `bytes_per_sec == 10485760` |
  | `velocidad_ventana_5s_olvida_pasado` | 10 MiB/s durante 5 s, después 1 MiB/s durante 6 s | `bytes_per_sec == 1048576` (±1 %) |
  | `velocidad_sin_muestras` | recién creado | `bytes_per_sec == 0`, `eta_secs == None` |
  | `eta_calculada` | total 100 MiB, hechos 40, 10 MiB/s | `eta_secs == Some(6)` |
  | `eta_cero_al_terminar` | hechos == total | `eta_secs == Some(0)` |
  | `reanudacion_no_infla_velocidad` | `already_done` 4 GiB, después 1 MiB/s | `bytes_per_sec ≈ 1048576`, `downloaded` incluye los 4 GiB |
  | `progreso_mas_de_50gb` | total 64424509440, hechos 60000000000 | sin desbordamiento, ETA correcta |
  | `throttle_limita_a_4hz` | `should_emit` cada 10 ms durante 1 s | 4 o 5 `true` en total |
  | `throttle_primer_evento_inmediato` | primera llamada | `true` |
  | `throttle_force_siempre` | `force()` justo después de emitir | `true` |

### T4 — Cabeceras `Range`/`Content-Range` y clasificación de respuestas
- **Ficheros/módulos:** `crates/core/src/download/http.rs`
- **Depende de:** T1
- **Cubre:** CA2, CA4, CA9 (lógica pura)
- **Subtareas:**
  - [ ] T4.1 `range_header(offset) -> Option<String>` (`None` si es 0).
  - [ ] T4.2 `parse_content_range(&str) -> Result<ContentRange { start, end, total: Option<u64> }>`.
  - [ ] T4.3 `classify(status, content_range, offset, expected) -> ResponseAction` (`Append`, `RestartFromZero`, `AlreadyComplete`, `Fail(kind)`, `Retry`).
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `range_offset_cero` | 0 | `None` |
  | `range_offset_grande` | 60000000000 | `"bytes=60000000000-"` |
  | `content_range_valido` | `"bytes 100-199/1000"` | `{100, 199, Some(1000)}` |
  | `content_range_total_desconocido` | `"bytes 100-199/*"` | `total: None` |
  | `content_range_malformado` | `"bytes=100-199/1000"`, `"items 1-2/3"`, `""` | `Err` |
  | `content_range_mas_de_4gb` | `"bytes 5000000000-64424509439/64424509440"` | parseo correcto en `u64` |
  | `classify_206_coherente` | 206, inicio == offset, total == expected | `Append` |
  | `classify_206_inicio_distinto` | 206, inicio != offset | `Fail(UnexpectedContentRange)` |
  | `classify_206_total_distinto` | 206, total != expected | `Fail(SizeMismatch)` |
  | `classify_200_con_offset` | 200, offset 4 MiB | `RestartFromZero` |
  | `classify_200_sin_offset` | 200, offset 0 | `Append` |
  | `classify_416_completo` | 416, offset == expected | `AlreadyComplete` |
  | `classify_416_incompleto` | 416, offset != expected | `RestartFromZero` |
  | `classify_401` / `classify_403` / `classify_404` | — | `Fail(Unauthorized)` / `Fail(Gated)` / `Fail(NotFound)` |
  | `classify_5xx_y_429` | 500, 503, 429 | `Retry` |

### T5 — Transferencia de un fichero con `.part`, `Range` y cancelación
- **Ficheros/módulos:** `crates/core/src/download/transfer.rs`, `crates/core/src/download/test_support.rs` (solo en `cfg(test)`)
- **Depende de:** T3, T4
- **Cubre:** CA2, CA4, CA5 (parte de un fichero)
- **Subtareas:**
  - [ ] T5.1 `HttpFetcher` sobre `reqwest::Client`: timeout de conexión de 30 s, `read_timeout` de 60 s, sigue redirecciones (máximo 10).
  - [ ] T5.2 `transfer_once(fetcher, spec, part, cancel, on_bytes) -> Result<TransferOutcome>`: abre el `.part` en append, manda `Range` según su longitud, aplica `classify` y escribe por bloques comprobando `cancel` entre bloques. `flush` al salir, también si se cancela.
  - [ ] T5.3 `TransferOutcome`: `Completed`, `Cancelled`, `Interrupted { bytes_written }`.
- **Tests unitarios** (wiremock):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `descarga_completa_desde_cero` | 256 KiB, `200` | `.part` idéntico, sin cabecera `Range` en la petición |
  | `reanuda_con_range` | `.part` de 100 KiB de 256 KiB; mock que exige `Range: bytes=102400-` y responde 206 | `.part` final idéntico al original |
  | `servidor_ignora_range_trunca` | `.part` de 100 KiB, servidor `200` con el cuerpo completo | `.part` == original (sin duplicar bytes) |
  | `range_416_ya_completo` | `.part` completo, servidor `416` | `Completed` sin escribir |
  | `content_range_incoherente_falla` | 206 con `bytes 0-…` cuando el offset es 100 KiB | `Err(UnexpectedContentRange)` |
  | `cancelacion_entre_bloques` | `cancel` disparado en el callback tras el bloque 1 | `Cancelled`, `.part` con ≤ 2 bloques y bytes válidos |
  | `sigue_redireccion` | mock A 302 → mock B con el cuerpo | descarga correcta desde B |
  | `respuesta_404` | 404 | `Err(NotFound)` |

### T6 — Reintentos ante cortes con backoff inyectado
- **Ficheros/módulos:** `crates/core/src/download/retry.rs`, `transfer.rs`, `test_support.rs` (servidor TCP que corta)
- **Depende de:** T5
- **Cubre:** CA3
- **Subtareas:**
  - [ ] T6.1 Trait `Sleeper { async fn sleep(&self, d: Duration) }`, `TokioSleeper` y `FakeSleeper` (registra las duraciones y vuelve al momento).
  - [ ] T6.2 `RetryPolicy { max_attempts_without_progress: 5, backoff: [1, 2, 4, 8] s }` y `transfer_with_retry`.
  - [ ] T6.3 `test_support::cutting_server`: un `tokio::net::TcpListener` en `127.0.0.1:0` que responde con `Content-Length`/`Content-Range` completos y cierra el socket tras N bytes, y admite `Range`. Hace falta porque wiremock no puede cortar a mitad del cuerpo.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `cortes_con_progreso_terminan` | 3 MiB, corte cada 1 MiB | `Completed` en 3 peticiones; los `Range` son `-`, `1048576-` y `2097152-` |
  | `cortes_sin_progreso_agotan` | 5 cortes antes del primer byte | `Err(Network)` tras 5 intentos; `FakeSleeper` registra `[1,2,4,8]` s; `.part` conservado |
  | `progreso_reinicia_contador` | 4 cortes sin bytes, 1 con bytes, 4 sin bytes, éxito | `Completed` (el contador se reinició) |
  | `no_reintenta_401` | 401 | 1 petición, `Err(Unauthorized)` |
  | `reintenta_503` | 503, 503, 200 | `Completed`, sleeps `[1,2]` s |
  | `cancel_durante_backoff` | `cancel` mientras duerme | `Cancelled`, sin más peticiones |

### T7 — Verificación de tamaño y SHA256 y renombrado final
- **Ficheros/módulos:** `crates/core/src/download/verify.rs`
- **Depende de:** T1, T3
- **Cubre:** CA8
- **Subtareas:**
  - [ ] T7.1 `sha256_file(path, cancel, on_bytes) -> Result<String>`: streaming de 1 MiB, devuelve hex en minúsculas, cancelable.
  - [ ] T7.2 `verify_file(part, expected_size, sha256: Option<&str>) -> Result<(), DownloadError>`.
  - [ ] T7.3 `finalize_unit(files)`: renombra todos los `.part` verificados a su nombre final. Si un destino ya existe: `DestinationExists`, sin sobrescribir.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `sha256_vector_conocido` | fichero `"abc"` | `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad` |
  | `sha256_fichero_vacio` | 0 bytes | `e3b0c442…b855` |
  | `sha256_mayor_que_bloque` | 3 MiB + 7 bytes deterministas | igual que `Sha256::digest` en memoria |
  | `verify_ok` | tamaño y hash correctos | `Ok(())` |
  | `verify_hash_mayusculas` | hash esperado en mayúsculas | `Ok(())` |
  | `verify_tamano_distinto` | 1 byte de menos | `Err(SizeMismatch)` sin calcular el hash |
  | `verify_hash_distinto` | hash de otro contenido | `Err(ChecksumMismatch)` |
  | `verify_sin_sha_solo_tamano` | `sha256: None`, tamaño correcto | `Ok(())` |
  | `finalize_renombra_todas` | 3 `.part` verificados | 3 ficheros finales, 0 `.part` |
  | `finalize_no_sobrescribe` | destino ya existente | `Err(DestinationExists)`, destino intacto |
  | `sha256_cancelable` | `cancel` a mitad | `Err` de cancelación, sin pánico |

### T8 — Comprobación de espacio en disco
- **Ficheros/módulos:** `crates/core/src/download/disk.rs`
- **Depende de:** T1
- **Cubre:** CA7
- **Subtareas:**
  - [ ] T8.1 Trait `DiskSpace { fn available(&self, path: &Path) -> io::Result<u64> }`, `Fs4DiskSpace` (con `fs4::available_space`, que pregunta por el primer ancestro existente de `dest`) y `FakeDiskSpace`.
  - [ ] T8.2 `required_bytes(files, part_lens) -> u64` y `check_space(disk, dest, files) -> Result<(), DownloadError>`, con un margen de `SPACE_MARGIN = 1 GiB`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `required_sin_parciales` | 3 × 20 GiB | 60 GiB |
  | `required_descuenta_part` | 60 GiB, `.part` de 10 GiB | 50 GiB |
  | `required_ignora_verificados` | una parte `verified` | no cuenta |
  | `espacio_insuficiente_60gb` | necesita 60 GiB, libres 40 GiB | `Err(InsufficientSpace { needed: 65498251264, available: 42949672960 })` |
  | `espacio_justo_con_margen` | libres == necesario + 1 GiB | `Ok(())` |
  | `espacio_justo_sin_margen` | libres == necesario | `Err(InsufficientSpace)` |
  | `fs4_directorio_inexistente_usa_ancestro` | `tmp/no/existe` | `Ok(n)` con `n > 0` (lee el disco real de `TempDir`, sin red) |

### T9 — Token de HF y política de autenticación
- **Ficheros/módulos:** `crates/core/src/download/auth.rs`, `transfer.rs`
- **Depende de:** T5
- **Cubre:** CA9
- **Subtareas:**
  - [ ] T9.1 Trait `TokenProvider { fn token(&self) -> Option<String> }` y `AuthPolicy { hosts: Vec<String> }` (por defecto `["huggingface.co"]`).
  - [ ] T9.2 `auth_header_for(url, policy, provider) -> Option<HeaderValue>`, que marca la cabecera como sensible (`set_sensitive(true)`).
  - [ ] T9.3 Comprobar que la redirección a otro host no reenvía `Authorization`. Si `reqwest` lo hiciera, se usa una política de redirección propia que la quite.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `cabecera_solo_para_host_hf` | `https://huggingface.co/x`, token `hf_t` | `Some("Bearer hf_t")` |
  | `sin_cabecera_otro_host` | `https://cdn-lfs.example/x` | `None` |
  | `sin_cabecera_host_parecido` | `https://huggingface.co.evil.test/x` | `None` |
  | `sin_token_sin_cabecera` | provider `None` | `None` |
  | `peticion_lleva_bearer` | wiremock (host de la política = `127.0.0.1`) que exige `Authorization` | 200 y descarga correcta |
  | `redireccion_no_filtra_token` | mock A (`127.0.0.1`) 302 → mock B (`localhost`, otro puerto) | B no recibe `Authorization` |
  | `gated_401_sin_reintento` | 401 | `Err(Unauthorized)`, 1 petición |
  | `gated_403` | 403 | `Err(Gated)` |

### T10 — Persistencia de la cola
- **Ficheros/módulos:** `crates/core/src/download/store.rs`
- **Depende de:** T1
- **Cubre:** CA10
- **Subtareas:**
  - [ ] T10.1 `QueueFile { schema: 1, next_id, units }` con `serde`.
  - [ ] T10.2 `QueueStore::load(path) -> LoadResult` (normaliza `Downloading`/`Verifying`/`Queued` a `Paused`, y si está corrupto lo renombra a `.corrupt`) y `save(&QueueFile)` atómico (`.tmp` en el mismo directorio + `rename`).
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `guardar_y_cargar_ida_vuelta` | 2 unidades y `next_id` 7 | estructura igual tras cargar |
  | `carga_normaliza_activos_a_paused` | `Downloading`, `Queued`, `Verifying`, `Completed` | `Paused`, `Paused`, `Paused`, `Completed` |
  | `fichero_inexistente_cola_vacia` | sin fichero | cola vacía, `next_id` 1 |
  | `json_corrupto_se_aparta` | `"{no json"` | cola vacía y existe `downloads.json.corrupt` |
  | `schema_desconocido_se_aparta` | `schema: 99` | igual que corrupto |
  | `guardado_atomico_no_deja_tmp` | `save` | no queda ningún `.tmp` y el contenido es válido |
  | `no_persiste_token_ni_url_cdn` | unidad creada con token `hf_secret` en el provider | el JSON no contiene `hf_secret` ni `cas-bridge` |

### T11 — Gestor: cola FIFO, ejecución de unidades y eventos
- **Ficheros/módulos:** `crates/core/src/download/manager.rs`, `events.rs`
- **Depende de:** T2, T6, T7, T8, T9, T10
- **Cubre:** CA1, CA6, CA7, CA8, CA11
- **Subtareas:**
  - [ ] T11.1 Trait `EventSink { fn state(&self, &DownloadSnapshot); fn progress(&self, &ProgressEvent) }` y `RecordingSink` para los tests.
  - [ ] T11.2 `DownloadManager::new(deps: ManagerDeps { fetcher, clock, sleeper, disk, tokens, store, sink, max_active })` y `enqueue(UnitRequest) -> Result<DownloadId>`. Valida las rutas (T1) y las partes (T2), rechaza destinos que ya están en la cola y marca como `preexisting` los ficheros finales que ya existen con el tamaño y el hash correctos.
  - [ ] T11.3 Planificador: toma la siguiente `Queued` si hay hueco, comprueba el espacio (T8), descarga las partes en orden (T6), las verifica (T7), finaliza la unidad y emite los eventos (con `Throttle`) y persiste en cada cambio de estado.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `unidad_simple_completa` | 1 fichero de 1 MiB con sha correcto | `Completed`; eventos de estado `Queued → Downloading → Verifying → Completed` |
  | `progreso_limitado_en_frecuencia` | 10 MiB, `FakeClock` +50 ms por bloque de 64 KiB | eventos de progreso ≤ `ceil(duración/250 ms)+2`; el último con `downloaded == total` |
  | `dividido_agrega_progreso` | 3 partes de 1 MiB | `total_bytes == 3145728`; ningún final en disco antes de verificar la tercera |
  | `fifo_y_un_activo` | 3 unidades, `max_active` 1 | orden de `Downloading` 1, 2, 3; nunca 2 activas a la vez (comprobado en el `RecordingSink`) |
  | `sin_espacio_no_pide_nada` | 60 GiB, disco falso con 40 GiB | `Failed(InsufficientSpace)`; wiremock con 0 peticiones |
  | `checksum_malo_borra_part` | sha incorrecto | `Failed(ChecksumMismatch)`, ni `.part` ni final |
  | `preexistente_no_se_descarga` | final ya en disco con hash correcto | `Completed`, 0 peticiones |
  | `enqueue_duplicado_rechazado` | el mismo destino dos veces | segundo `Err(DestinationExists)` |
  | `enqueue_ruta_insegura` | `relative_path` `"../x"` | `Err(UnsafePath)` |
  | `persiste_en_cambios_de_estado` | unidad completa | `downloads.json` refleja `Completed`; nº de guardados == nº de cambios de estado |

### T12 — Órdenes: pausar, reanudar, cancelar, reintentar y quitar
- **Ficheros/módulos:** `crates/core/src/download/manager.rs`
- **Depende de:** T11
- **Cubre:** CA5, CA10, CA11
- **Subtareas:**
  - [ ] T12.1 `pause(id)`, `resume(id)`, `cancel(id)`, `retry(id)` y `remove(id)`, con validación por `can_transition` (`InvalidTransition` si no vale).
  - [ ] T12.2 Cancelar borra los `.part` y los finales creados por la unidad, y nunca los `preexisting`.
  - [ ] T12.3 `DownloadManager::restore(store)`: carga la cola persistida (todo en `Paused` salvo los terminales).
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `pausa_conserva_part` | pausa tras 2 bloques | `Paused`, `.part` con ≥ 2 bloques, sin más escrituras |
  | `reanuda_desde_part` | `resume` tras pausar | la siguiente petición lleva `Range: bytes=<len>-`; `Completed` |
  | `pausar_activa_arranca_siguiente` | 2 unidades, pausa la 1 | la 2 pasa a `Downloading` |
  | `pausar_en_cola` | `Queued` → `pause` | `Paused` sin peticiones |
  | `cancelar_borra_todo_lo_suyo` | dividido con la parte 1 verificada y la 2 en curso | `Cancelled`; ningún `.part` ni final de la unidad |
  | `cancelar_respeta_preexistentes` | parte 1 `preexisting` | la parte 1 sigue en disco |
  | `reintentar_tras_fallo_red` | `Failed(Network)` con `.part` de 1 MiB | `retry` → `Queued` → continúa desde 1 MiB |
  | `quitar_solo_terminales` | `remove` en `Downloading` | `Err(InvalidTransition)` |
  | `restaurar_y_reanudar` | `downloads.json` con `Downloading` y `.part` de 4 MiB | `restore` → `Paused`; `resume` → `Range: bytes=4194304-` |
  | `orden_sobre_id_inexistente` | `pause(999)` | `Err(UnknownDownload)` |

### T13 — Puente Tauri: comandos, eventos y especificación desde HF
- **Ficheros/módulos:** `src-tauri/src/downloads.rs`, `src-tauri/src/lib.rs` (registro), `crates/core/src/download/hf_spec.rs`
- **Depende de:** T12; módulo `hf` de 005
- **Cubre:** CA6, CA9, CA12 (backend)
- **Subtareas:**
  - [ ] T13.1 `hf_spec::build_unit_request(repo, revision, selected_path, tree_entries, models_dir) -> Result<UnitRequest>`: expande las partes (T2) y toma `expected_size` de `lfs.size` (o de `size` si no hay `lfs`) y `sha256` de `lfs.oid`. La URL es `https://huggingface.co/{repo}/resolve/{revision}/{path}`, con cada segmento codificado y `/` conservada. El destino es `models_dir/<owner>/<repo>/<path>`.
  - [ ] T13.2 `TauriEventSink`: emite `downloads:state` y `downloads:progress`. `KeyringTokenProvider` reutiliza el acceso al llavero de 005.
  - [ ] T13.3 Comandos `downloads_enqueue_hf { repo, revision, path }`, `downloads_list`, `downloads_pause`, `downloads_resume`, `downloads_cancel`, `downloads_retry` y `downloads_remove`. El gestor vive en el `State` de Tauri, creado en `setup` con `restore`.
- **Tests unitarios** (los de `hf_spec` están en `core`, porque los comandos son una capa fina):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `spec_usa_lfs_oid_como_sha256` | entrada `{ size, oid: sha1, lfs: { oid: "8e0a…", size } }` | `sha256 == Some("8e0a…")`, nunca el `oid` SHA1 |
  | `spec_sin_lfs_sin_sha` | entrada sin `lfs` | `sha256 == None`, `expected_size == size` |
  | `spec_expande_divididos` | selección de la parte 1 de 3 | `UnitRequest` con 3 ficheros |
  | `spec_url_codificada` | path `"sub dir/m+1.gguf"` | `…/resolve/<rev>/sub%20dir/m%2B1.gguf` |
  | `spec_destino_por_repo` | repo `"org/model"` | `models_dir/org/model/<path>`, construido con `Path` |
  | `spec_rev_commit` | revision `"abc123"` | la URL contiene `/resolve/abc123/` |
  | `spec_ruta_insegura` | path `"../../x.gguf"` | `Err(UnsafePath)` |

### T14 — UI de descargas e integración con el botón "Descargar"
- **Ficheros/módulos:** `ui/src/lib/downloads/format.ts`, `store.svelte.ts`, `api.ts`, `DownloadsView.svelte`, `DownloadItem.svelte`; navegación de `ui/src/`; el botón "Descargar" de la ficha de 005
- **Depende de:** T13
- **Cubre:** CA12
- **Subtareas:**
  - [ ] T14.1 `format.ts`: `formatBytes` (base 1000, una decimal, coma decimal `es-ES`), `formatSpeed` y `formatEta` (`null` → `—`).
  - [ ] T14.2 `store.svelte.ts`: estado con runas de Svelte 5. `applyState(snapshot)`, `applyProgress(event)` y `actionsFor(state)` como funciones puras. Se suscribe a los eventos con `@tauri-apps/api/event`.
  - [ ] T14.3 Vista "Descargas" con barra, bytes, velocidad, ETA, error legible por `FailureKind` y botones según `actionsFor`. Botón "Descargar" de la ficha → `downloads_enqueue_hf` → navegar a "Descargas". Un error de encolado (`IncompleteSplit`, `DestinationExists`) se muestra en la ficha.
- **Tests unitarios** (Vitest):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `formatBytes_unidades` | 0, 999, 1500, 4500000000, 64424509440 | `"0 B"`, `"999 B"`, `"1,5 kB"`, `"4,5 GB"`, `"64,4 GB"` |
  | `formatSpeed_mb_s` | 12345678 | `"12,3 MB/s"` |
  | `formatEta_casos` | `null`, 0, 59, 200, 7322 | `"—"`, `"0 s"`, `"59 s"`, `"3 min 20 s"`, `"2 h 2 min"` |
  | `applyProgress_actualiza_unidad` | estado con id 1 + progreso id 1 | bytes y velocidad actualizados |
  | `applyProgress_id_desconocido` | progreso id 9 sin snapshot | estado sin cambios |
  | `applyState_inserta_y_reemplaza` | snapshot nuevo y después uno actualizado | 1 unidad con el último estado |
  | `actions_por_estado` | `Downloading`, `Paused`, `Failed`, `Completed`, `Queued` | `[pause,cancel]`, `[resume,cancel]`, `[retry,cancel,remove]`, `[remove]`, `[pause,cancel]` |
  | `mensaje_error_por_tipo` | `InsufficientSpace { needed, available }` | texto con las dos cifras formateadas |

### T15 — Fachada `download_one` y migración del runtime (004)
- **Ficheros/módulos:** `crates/core/src/download/single.rs`; el punto de descarga de `crates/core/src/runtime/` que deje 004
- **Depende de:** T6, T7, T9
- **Cubre:** CA13
- **Subtareas:**
  - [ ] T15.1 `download_one(fetcher, url, dest, expected_size: Option<u64>, sha256: Option<&str>, cancel, on_progress) -> Result<PathBuf>`: reutiliza `transfer_with_retry` y `verify_file`, con `.part` y renombrado, y sin cola, persistencia ni token.
  - [ ] T15.2 Sustituir la descarga simple de 004 por `download_one`, adaptando sus tests sin cambiar su comportamiento visible.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `download_one_completa` | wiremock, 512 KiB | fichero final correcto, sin `.part` |
  | `download_one_sin_tamano_conocido` | `expected_size: None`, sin `Content-Length` | completa; sin verificar el tamaño |
  | `download_one_reanuda_part_previo` | `.part` de 100 KiB | petición con `Range` |
  | `download_one_sha_incorrecto` | sha incorrecto | `Err(ChecksumMismatch)`, sin final |
  | tests existentes de 004 | — | siguen en verde |

### T16 — Comprobación en vivo
- **Ficheros/módulos:** sección "Verificación en vivo" de este plan (resultado anotado al cerrarlo)
- **Depende de:** T14, T15
- **Cubre:** CA14
- **Subtareas:**
  - [ ] T16.1 En Windows y en macOS: descargar `qwen2.5-0.5b-instruct-q4_k_m.gguf`, pausar a mitad, cerrar la app, abrirla, reanudar y comprobar `Completed`, que no queda `.part` y que el SHA256 (`certutil -hashfile` / `shasum -a 256`) es igual al `lfs.oid` del árbol.
  - [ ] T16.2 Con un modelo dividido real pequeño: una sola unidad, ningún final antes de terminar.
  - [ ] T16.3 Con un repositorio gated sin aceptar la licencia: mensaje `Gated` en la UI, y el token no aparece en `downloads.json`.
- **Tests unitarios:** no aplica (comprobación manual). Los equivalentes automáticos están en T5–T14.

## Fuera de alcance

- Descargas en paralelo de varias partes o por segmentos (varias conexiones `Range` por fichero). El máximo es 1 unidad activa.
- El protocolo Xet nativo (deduplicación por chunks). Se usa la descarga HTTP de `/resolve/`.
- Limitar el ancho de banda y programar descargas.
- Reanudación automática de la cola al arrancar (ver Preguntas abiertas). En este plan, se restaura en `Paused`.
- Guardar el estado del SHA256 para no rehashear al final.
- Elegir el directorio de modelos desde la UI y escanear la biblioteca (Fase 4). Aquí se usa el `models_dir` por defecto de la configuración existente o `directories` + `models/`.
- Descargas de fuentes que no sean HF, salvo la fachada `download_one` para el runtime.
- Notificaciones del sistema al terminar una descarga.
- Modelos `mmproj` como parte de la unidad (MVP §6).

## Riesgos

| Riesgo | Mitigación |
|---|---|
| La URL firmada de la CDN caduca durante una descarga larga o una pausa | Nunca se persiste. Cada intento vuelve a pedir `/resolve/`, y un 403 de la CDN tras un corte se trata como transitorio en el primer reintento (test en T6 si aparece en T16) |
| `reqwest` reenvía `Authorization` en una redirección | Test explícito en T9. Si falla, una política de redirección propia quita la cabecera |
| El fichero cambia en HF entre la pausa y la reanudación | `rev` = commit `sha`. Además, el `Content-Range` total debe ser igual a `expected_size` y el SHA256 final lo detecta |
| Hashear 50+ GB tarda minutos y parece colgado | Estado `Verifying` con progreso propio (`phase: "verify"`) en la UI |
| El espacio libre cambia durante la descarga (otro proceso llena el disco) | El error de E/S de escritura se mapea a `Failed { Io }` con el mensaje del sistema. El `.part` se conserva para reintentar |
| Rutas largas en Windows (>260 caracteres) con repositorios anidados | `safe_join` + `Path`; se documenta en T16 si aparece. Si se reproduce, issue aparte (prefijo `\\?\`) |
| Antivirus de Windows que bloquea el `rename` del `.part` | Reintento del `rename` hasta 3 veces con `Sleeper` (en T7 si aparece en T16) |
| Duplicar la detección de partes con 005 | `split.rs` vive en `download`. Si 005 ya tiene un parser equivalente, T2 lo reutiliza en lugar de crear otro |
| `wiremock` no puede cortar una respuesta a mitad | Servidor TCP mínimo propio en `test_support` (T6.3) |
| Tests lentos por tamaños grandes | Los tests de más de 50 GB solo usan tamaños declarados (aritmética y cabeceras), nunca datos reales; los cuerpos reales son ≤ 10 MiB |

## Preguntas abiertas

1. **Crate o módulo:** ¿`download` es un módulo de `crates/core` (`crates/core/src/download/`, la opción de este plan) o un crate propio? Depende de cómo lo deje 003.
2. **Reanudación automática al arrancar:** ¿la cola restaurada debe reanudarse sola o quedarse en `Paused` hasta que el usuario pulse "Reanudar"? El plan elige `Paused` para no consumir red sin permiso.
3. **Descargas simultáneas:** ¿basta con 1 unidad activa, o se quiere un ajuste (2–3)? El gestor lo admite como parámetro, pero no hay UI para cambiarlo.
4. **Margen de espacio:** ¿vale 1 GiB fijo, o se prefiere un porcentaje (p. ej. 5 % del tamaño)?
5. **Revisión pinneada:** ¿expone 005 el commit `sha` del modelo y las entradas del árbol con `lfs.oid`/`lfs.size`? Si no, T13 tiene que añadirlos al cliente `hf` (un cambio pequeño en un módulo de 005) o usar `main`.
6. **Directorio de modelos:** hasta la Fase 4, ¿`<app_data>/models/<owner>/<repo>/` es aceptable como destino por defecto?
7. **Migración de 004:** ¿se hace en este plan (T15.2) o en un chore aparte, cuando 004 esté integrado? Si 004 no está en `main` al llegar a T15, T15.2 se mueve a un plan propio.
