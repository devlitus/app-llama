# 006 — Gestor de descargas (cola, progreso, reanudación y verificación)

- **Tipo:** feature
- **Estado:** aprobado
- **Rama:** feat/006-gestor-descargas
- **Fase del roadmap:** 3. Descargas ([docs/MVP.md](../MVP.md), §2.B y §4)

## Objetivo

Que el usuario pueda descargar modelos GGUF de Hugging Face de cualquier tamaño (también de más de 50 GB y divididos en partes) desde una cola. Verá el progreso con bytes, velocidad y ETA, y podrá pausar, reanudar (también tras cerrar la app) y cancelar. Ningún fichero se da por bueno sin verificar su tamaño y su SHA256.

## Contexto

### Lo que ya existe en `main` (003, 004 y 005 hechos)

- **Organización:** un único crate `app-llama-core` (`crates/core`) con módulos (`hf/`, `runtime/`, `download/`, `config/`…). `crates/core/src/download/mod.rs` existe y solo tiene el comentario de módulo. `src-tauri/` (crate `app-llama`, lib `app_llama_lib`) depende de `app-llama-core`. La UI está en `ui/` (Svelte 5 + TypeScript, Vitest).
- **Dependencias del workspace** (`Cargo.toml` raíz): `reqwest 0.13` (`json`, `stream`), `tokio` (`fs`, `io-util`, `sync`, `process`, `time`, `rt`), `futures-util`, `async-trait`, `sha2`, `hex`, `serde`, `serde_json`, `thiserror`, `keyring`; `dev`: `wiremock 0.6`, `tempfile`. `reqwest::Url` cubre el parseo de URLs (no hace falta el crate `url`).
- **HF (005), `crates/core/src/hf/`:**
  - `client.rs` → `HfClient { http, base_url, token: Option<HfToken> }`. Solo envía el token a destinos con el mismo origen que `base_url` (`url::is_trusted_target`). `tree(&RepoId)` pide siempre `tree/main`.
  - `url.rs` → `RepoId::parse` (valida `owner/name` sin `..`), `endpoint(base, segments, query)` (añade los segmentos uno a uno y la URL los codifica), `pub(super) const DEFAULT_REVISION: &str = "main"`, `tree_url(base, repo)` y `model_info_url(base, repo)` con `MODEL_INFO_EXPANDS` (sin `sha`).
  - `model.rs` → `ModelInfo { id, gated, downloads, likes, gguf, license }` **sin `sha`**; `TreeEntry { kind, path, size, lfs: Option<LfsInfo { oid, size }> }`.
  - `files.rs` → `GgufPart { path, size, sha256: Option<String> }` (con `size = lfs.size` y `sha256 = lfs.oid`), `GgufVariant { name, quant, parts, total_bytes, complete }`, `DownloadRequest { repo_id, revision, parts: Vec<GgufPart> }` (solo `Serialize`) y `GgufVariant::download_request(&RepoId)`, que fija `DEFAULT_REVISION` y hoy solo usan los tests.
  - `quant.rs` → `strip_gguf_extension` y `parse_split_suffix(stem) -> Option<SplitSuffix { prefix_len, index, total }>`, ambos `pub(super)`. `files.rs` ya agrupa las partes de un split con ellos.
  - `token.rs` → `HfToken` (Debug/Display redactados, `expose()` es `pub(crate)`), trait `SecretStore`, `KeyringStore` (llavero, **síncrono**) y, en tests, `MemoryStore` y `FailingStore`.
  - `service.rs` → `HfService { http, base_url, store, client: RwLock<HfClient> }`. Lee el llavero una vez en `new` y guarda el token en el `HfClient` en memoria; `save_token`/`delete_token` lo sustituyen. `model_detail(repo, budget) -> ModelDetail { info, ctx, budget, variants }`.
- **Runtime (004), `crates/core/src/runtime/`:** `fetch.rs` define `trait ArtifactFetcher { async fn fetch(FetchRequest, progress, &CancelToken) }`, `FetchRequest { url, dest, expected_size: u64, sha256: Option<[u8; 32]> }`, `CancelToken(Arc<AtomicBool>)` y `SimpleHttpFetcher` (sin reanudación; su doc dice "El plan 006 aportará una implementación con reanudación"). `install.rs` lo usa a través de `RuntimeDeps.fetcher`. Los tests de `install.rs` usan `FakeFetcher` (`runtime/test_support.rs`), así que no dependen de la implementación HTTP. `state.rs` tiene el patrón de persistencia a seguir: `runtime.json` con `schema`, escritura `.tmp` + `rename`, y un fichero ilegible (no UTF-8, JSON roto o `schema` distinto) se copia a `.bak` y se arranca vacío.
- **Puente Tauri:** `src-tauri/src/commands/` con `error.rs` (`ErrorPayload { code: &'static str, message: String }`), `hf.rs` (`HfAppState { service: HfService, budget }`, `production_state()`, `From<HfError> for ErrorPayload`, tests `json_*_para_la_ui` que fijan el JSON que espeja `ui/src/lib/hf/types.ts`) y `runtime.rs` (`RuntimeAppState`, evento `runtime://progress`, cliente con `read_timeout` de 60 s y sin timeout total). `src-tauri/src/lib.rs` registra los comandos en `register()` y crea el estado gestionado en `setup` con `app.manage(...)`.
- **UI:** `ui/src/lib/hf/VariantRow.svelte` tiene el botón "Descargar" **deshabilitado** ("Disponible próximamente") y construye `{ repo_id, revision: DEFAULT_REVISION, parts }` con su propia `const DEFAULT_REVISION = "main"`. `ModelDetail.svelte` reenvía la prop `onDownload`, pero `views/Explore.svelte` no se la pasa. La navegación (`ui/src/lib/navigation.ts`, `App.svelte`) tiene 5 secciones y las vistas no reciben props. Formato de tamaños en GiB con coma decimal (`formatGiB` en `ui/src/lib/hf/view-model.ts`). `ui/src/lib/errors.ts` → `errorMessage(payload)`.

### Issue #55: forma del puente y fuente única de la revisión

La ficha emite `DownloadRequest { repo_id, revision, parts: GgufPart[] }` (la unidad completa) y la versión anterior de este plan proponía `downloads_enqueue_hf { repo, revision, path }` (un solo fichero, con el backend volviendo a pedir el árbol). Además, `"main"` está duplicada en `VariantRow.svelte` y en `hf/url.rs`. **Decisión:**

1. El comando `downloads_enqueue_hf` recibe **el `DownloadRequest` completo** (`request: DownloadRequest`). El diseño trata los modelos divididos como una unidad y la ficha ya los tiene agrupados, así que no hace falta volver a pedir el árbol ni `expand_split`.
2. El backend **no confía a ciegas** en lo que llega de la webview: valida el `repo_id` (`RepoId::parse`), la revisión (`"main"` o 40 hex en minúsculas), cada ruta (`safe_join`, extensión `.gguf`), cada `sha256` (exactamente 32 bytes en hex, decodificado con `hex::decode_to_slice`: lección `validacion-hex-laxa` de 004), `size > 0` y que las partes formen un split completo. La URL de descarga la construye siempre el backend sobre `base_url` (nunca llega de la UI).
3. **Fuente única de la revisión:** el backend la decide en `HfService::model_detail`. Pide `expand[]=sha` en la ficha, pide el árbol **en ese commit** (para que `size` y `lfs.oid` correspondan al mismo commit que se descargará) y la expone en `ModelDetail.revision`. La UI pasa `detail.revision` a `VariantRow` y se borra su constante. `DEFAULT_REVISION` queda solo en `hf/url.rs` como respaldo si HF no devuelve `sha`.

### APIs externas verificadas (2026-11-28)

- **Commit de la ficha:** `GET https://huggingface.co/api/models/{repo}?expand[]=...` solo devuelve los campos pedidos (más `_id` e `id`). Sin `expand[]=sha` **no** viene `sha`; con él, sí (p. ej. `Qwen/Qwen2.5-0.5B-Instruct-GGUF` → `"sha": "9217f5db79a29953eb74d5343926648285ec7e67"`).
- **Árbol en un commit:** `GET /api/models/{repo}/tree/{sha}?recursive=true` responde igual que con `main`. Para `qwen2.5-0.5b-instruct-q4_k_m.gguf` en ese commit: `size: 491400032`, `lfs.oid: "74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db"`, `lfs.size: 491400032`. **`lfs.oid` es el SHA256 del contenido**; el `oid` de primer nivel es el SHA1 del puntero git y no sirve para verificar. Sin `lfs` no hay SHA256 publicado y solo se verifica el tamaño (R6: "SHA256 cuando HF lo publica").
- **Descarga:** `GET https://huggingface.co/{repo}/resolve/{rev}/{path}` funciona con `rev` = commit (comprobado con `README.md` en ese commit). Para LFS/Xet responde con un 302 a una URL firmada y temporal de una CDN de otro host (cdn-lfs / cas-bridge.xethub), que admite `Range: bytes=N-` y responde `206` con `Content-Range: bytes N-M/TOTAL`. La firma caduca, así que **cada intento vuelve a pedir `/resolve/`** y nunca se persiste la URL de la CDN.
- **Gated / privados:** sin token, o con un token sin acceso, `/resolve/` responde `401` o `403`. Con `Authorization: Bearer <token>` responde la redirección. La CDN no necesita el token. `reqwest 0.13` quita `Authorization` al seguir una redirección a otro origen (comprobado en 005, decisión `origen-estricto-link`); aquí se vuelve a fijar con un test (T9).

### Decisiones del usuario aplicadas (2026-11-26)

Un crate `app-llama-core` con módulos → `crates/core/src/download/`. `wiremock` en los tests. Tamaños en **GiB** en la UI. La cola se **restaura en pausa** al reiniciar, con **1 descarga activa** a la vez y un margen de disco de **1 GiB**. Token de HF en Ajustes (lo gestiona 005; aquí solo se lee). **Sin pruebas manuales en Mac**: la comprobación en vivo (T19) es solo en Windows y macOS se cubre por CI.

### Decisión del usuario: directorio por defecto hasta la Fase 4 (2026-11-28)

Hasta que la Fase 4 permita elegir el directorio de modelos (decisión del usuario, 2026-11-28):

- **Modelos:** `app_local_data_dir()/models/<owner>/<name>/`.
- **Cola:** `app_local_data_dir()/downloads.json`.
- En Windows `app_local_data_dir()` es `%LOCALAPPDATA%\io.github.devlitus.appllama\`. **No** se usa `app_data_dir()` (`%APPDATA%`, *Roaming*, el que usa hoy el runtime), para que un perfil itinerante no sincronice decenas de GB. En macOS ambos son `~/Library/Application Support/io.github.devlitus.appllama/`.

### Ubicación del código

- **Lógica:** `crates/core/src/download/` (sin Tauri, R7). Ajustes pequeños en `crates/core/src/hf/` (T2, T9, T11, T14) y en `crates/core/src/runtime/fetch.rs` (T18).
- **Puente:** `src-tauri/src/commands/downloads.rs` (comandos, `DownloadsAppState`, `TauriEventSink`, mapeo de errores a `ErrorPayload`), registrado en `src-tauri/src/commands.rs` y `src-tauri/src/lib.rs`.
- **UI:** `ui/src/lib/downloads/` (tipos, API, formato, store y componentes) y la vista `ui/src/views/Downloads.svelte`.

### Diseño

1. **Unidad de descarga (`DownloadUnit`):** una variante del modelo, que puede tener varios ficheros (las partes `-0000k-of-0000N.gguf`). Tiene un `DownloadId(u64)` creciente y persistido, `repo_id`, `revision`, `display_name` (la ruta de la primera parte sin el sufijo de split ni `.gguf`) y un `Vec<FileSpec>`. Cada `FileSpec` lleva `relative_path` (la ruta de HF), `url`, `dest` (ruta final), `expected_size: u64`, `sha256: Option<String>` y `status: FileStatus` (`Pending | Verified | Preexisting`). La unidad tiene un estado único `UnitState`: `Queued | Downloading | Verifying | Paused | Completed | Failed(FailureKind) | Cancelled`.
2. **Ficheros en disco:** cada fichero se escribe en `<dest>.part`. **La longitud del `.part` es la única fuente de verdad del desplazamiento**: el offset no se persiste. Cuando todas las partes de la unidad están verificadas, se renombran a su nombre final, en orden. Así una unidad dividida nunca deja a la vista un conjunto incompleto de `.gguf`.
3. **Transferencia de un fichero:** `GET` con `Range: bytes=<len(.part)>-` si el offset es mayor que 0, y escritura en modo append. Cada lectura del cuerpo compite (`tokio::select!`) con `cancel.cancelled()`, así que pausar o cancelar no espera al `read_timeout` aunque la conexión esté parada.
   - `206` con `Content-Range` que empieza en el offset y cuyo total es `expected_size`: se continúa.
   - `200` con offset > 0: el servidor ignora el `Range`, así que se trunca el `.part` y se empieza de 0.
   - `416` con `.part` de tamaño igual a `expected_size`: el fichero ya está completo.
   - `416` con offset > 0 y distinto de `expected_size`: se trunca el `.part` y se reinicia.
   - `416` con offset 0: `Fail(UnexpectedContentRange)`. Sin `Range` en la petición, un `416` es incoherente, y reiniciar desde 0 repetiría la misma petición.
   - `206` con un inicio incoherente: `UnexpectedContentRange`; se trunca y se reinicia desde 0 **una sola vez** por fichero; si se repite, `Failed(UnexpectedContentRange)`.
   - **Límite de reinicios:** todo reinicio desde 0 (`Restart`: `200` con offset > 0, `416` incompleto o el reinicio único por `UnexpectedContentRange`) cuenta como **intento sin progreso** (§4): consume uno de los 5 intentos, espera su backoff y nunca pone el contador a 0. Así un servidor que ignora siempre el `Range` no puede provocar un bucle infinito de reinicios.
   - `206` con un total distinto de `expected_size`, o un cuerpo que supera `expected_size`: `Failed(SizeMismatch)` y se borra el `.part`. Se corta **antes** de escribir el exceso (lección de `SimpleHttpFetcher`: un servidor que envía de más no debe llenar el disco).
   - `401`/`403` cuya URL final (`Response::url()`) tiene el origen de HF: `Unauthorized`/`Gated`, sin reintento. `403` desde otro origen (la CDN con la firma caducada): transitorio, con reintento, porque el siguiente intento vuelve a pedir `/resolve/`. `404`: `NotFound`, sin reintento.
   - **Valor por defecto seguro de `from_hf_origin`:** solo vale `false` cuando se puede afirmar que la URL final tiene un origen distinto del de HF. Si no se puede determinar (`hf_origin: None`, como en el `ResumableHttpFetcher` de T18, o un origen opaco), vale `true`: un `401`/`403` acaba en `Unauthorized`/`Gated` sin reintento, nunca en `Failed(Network)` tras 5 intentos.
   - `5xx`, `429`, conexión cortada o timeout de lectura: transitorio, con reintento.
   - **Errores de `reqwest` sin URL:** el `Display` de `reqwest::Error` incluye la URL, que puede ser la URL firmada de la CDN (`?X-Amz-Signature=…`), y `FailureKind` se guarda en `downloads.json` y llega a la UI. Regla: **nunca se construye `FailureKind::Io { message }`, `FailureKind::Network` ni `DownloadError::Io` con `reqwest::Error::to_string()`** (tampoco envolviéndolo en un `std::io::Error`). Se usa `reqwest::Error::without_url()` antes de formatearlo o un mensaje propio. La conversión vive en un único punto (T5.4) y la usan T5 y T6.
4. **Reintentos:** hasta 5 intentos consecutivos **sin progreso** por fichero, con esperas de 1, 2, 4 y 8 s entre ellos mediante un `Sleeper` inyectado (la espera también compite con `cancel.cancelled()`). Hay progreso cuando, al acabar un intento, `len(.part)` **supera la marca máxima** alcanzada antes en esa misma llamada (no basta con escribir bytes: tras un reinicio, volver a descargar lo que ya se tenía no es progreso). Solo entonces el contador vuelve a 0. Un `Restart` siempre cuenta como intento sin progreso (§3). Agotados los intentos: `Failed(Network)`, también si el último fue un `Restart`. El `.part` se conserva, así que "Reintentar" continúa desde donde iba.
5. **Verificación:** al terminar cada fichero se comprueba que `len(.part) == expected_size` y, si hay `sha256`, se calcula el SHA256 del `.part` en bloques de 1 MiB dentro de `spawn_blocking` (para no bloquear el runtime de tokio durante minutos), comprobando la cancelación entre bloques, y se compara sin distinguir mayúsculas. Cada parte **descargada** se verifica justo después de descargarse, con la unidad todavía en `Downloading`; la unidad solo pasa a `Verifying` al final, para verificar las **candidatas** a `Preexisting` (§9) sobre su final. En los dos casos el hash emite progreso con `phase: "Verify"` (contrato en §6). No se guarda el estado del hash entre reanudaciones: se hashea entero al final. Si el SHA256 no coincide: se borra ese `.part` y la unidad pasa a `Failed(ChecksumMismatch)`.
6. **Progreso:** `ProgressTracker` con un `Clock` inyectado. Calcula la velocidad sobre una **ventana deslizante de 5 s** y la ETA como `ceil((total − hechos) / velocidad)` (`None` si la velocidad es 0; `Some(0)` al terminar). `Throttle` deja pasar **como máximo 1 evento de progreso cada 250 ms por unidad**, más el primero y el final forzado. `ProgressReporter` combina los dos. Los cambios de estado se emiten siempre y al momento. Cuando `transfer_with_retry` trunca un `.part` (`Restart`), avisa con `on_restart` (campo de `TransferObserver`, junto a `on_bytes`) y el reporter se rebobina a la cantidad absoluta ya descargada de la unidad (T12.4 y T12.5), así `downloaded_bytes` nunca supera `total_bytes`.
   - **Contrato de `downloads://progress` (implementado en T12, `ProgressEmitter` de `runner.rs`):**
     - **Un solo `ProgressReporter` y un solo `Throttle` por unidad**, compartidos por las dos fases. `total_bytes` es constante durante la ejecución y siempre `downloaded_bytes <= total_bytes`.
     - **`phase: Download`:** bytes recibidos más los iniciales (longitud de los `.part` al empezar y `expected_size` de las partes `Verified`, `Preexisting` y candidatas). Es monótono salvo en un `rewind` (§3, T12.5), que se emite como evento forzado.
     - **`phase: Verify`:** parte de los bytes que no requieren hash (partes sin `sha256` o ya `Verified`/`Preexisting`) y suma los hasheados acumulados. Es monótono.
     - **Intercalado:** cada parte se verifica justo después de descargarse, con la unidad todavía en `Downloading` (§5), así que los eventos `Download` y `Verify` se intercalan y `downloaded_bytes` **puede bajar al cambiar de fase**. La UI trata la barra de cada `phase` como **independiente** (T16.3, T17.2); nunca compara valores de fases distintas. El progreso guardado en la UI solo sobrevive a las transiciones entre estados en ejecución (`Downloading` ↔ `Verifying`, incluido el mismo estado); al pasar a `Queued`, `Paused`, `Failed`, `Cancelled` o `Completed` se borra y la vista usa los datos del snapshot (T16.3, T17.2).
     - **Fin:** el último evento de una unidad que se completa es `Verify` con `downloaded_bytes == total_bytes`.
     - **Eventos forzados** (sin pasar por el limitador): `rewind`, `finish_download` (una vez, tras descargar y verificar la última parte pendiente) y `finish_verify` (una vez, al acabar `Verifying`, tras las candidatas). El resto pasa por el limitador de 250 ms.
     - **Velocidad y ETA:** en un evento `Verify`, `bytes_per_sec` y `eta_secs` se refieren al hash, no a la red.
7. **Espacio en disco:** con la unidad aún en `Queued`, antes de pasar a `Downloading`, `necesario = Σ (expected_size − len(.part))` de los ficheros que **se van a descargar**, más `SPACE_MARGIN = 1 GiB` (1073741824). Si `DiskSpace::available(dest)` es menor: `Queued → Failed(InsufficientSpace { needed, available })` sin pasar por `Downloading` y sin abrir ninguna conexión. `u64` en toda la cadena. **Orden en el runner (#73):**
   0. **Devolver a `Pending` las partes `Verified` que no tienen ni `.part` ni un destino de `expected_size`** (ficheros borrados por el usuario en pausa; T13.4). Va antes que todo lo demás para que esas partes cuenten en `required_bytes`.
   1. **Clasificar las candidatas a `Preexisting` (§9) antes de comprobar el espacio.** Es una comprobación barata: solo `metadata`, sin hash.
   2. **Calcular `required_bytes` solo sobre las partes `Pending` que no son candidatas.** Una candidata nunca se descarga: si su hash coincide pasa a `Preexisting`, y si no, la unidad acaba en `Failed(DestinationExists)`. Por eso no necesita espacio, salga como salga el hash.
   3. **Si `required_bytes == 0`, no se llama a `check_space`.** No hay nada que escribir, así que no se exige el margen. Una unidad formada solo por candidatas o partes `Verified` que conservan su `.part` o su final se completa aunque el disco esté lleno.
   4. **El hash de las candidatas se calcula después, en `Verifying` (§5).** No se calcula en `Queued`: hashear 60 GiB tarda minutos y no puede quedar oculto tras un estado `Queued` sin progreso.

   `disk.rs` (T8) no cambia: `required_bytes` ya cuenta solo las `Pending`, y es el runner quien le pasa la lista de ficheros a descargar sin las candidatas.
8. **Cola:** FIFO con un máximo de **1 unidad activa** (`max_active`, parámetro del constructor; en producción, 1, sin UI para cambiarlo). Las partes de una unidad se descargan en orden. Órdenes y estados de origen válidos:

   | Orden | Desde | Efecto |
   |---|---|---|
   | Pausar | `Queued`, `Downloading` | Cancela la transferencia y conserva los `.part`. → `Paused` |
   | Reanudar | `Paused` | → `Queued` |
   | Cancelar | `Queued`, `Downloading`, `Verifying`, `Paused` | Aborta y borra los `.part` y los finales que haya creado la unidad (los de sus partes `Verified` ya renombradas, T13.4), nunca los `Preexisting` ni los ficheros protegidos por otra unidad (#72, abajo). → `Cancelled`. Si el borrado falla, la unidad pasa igualmente a `Cancelled` y la orden devuelve el error; los `.part` que queden se limpian con «Quitar» (T13.6). Un final ya renombrado cuyo borrado falle se queda en disco (coste aceptado, #87) |
   | Reintentar | `Failed` | → `Queued` (los `.part` se conservan) |
   | Quitar | `Completed`, `Failed`, `Cancelled` | La saca de la lista. En `Failed` borra además sus `.part` y los finales de sus partes `Verified` ya renombradas (una unidad no terminada no deja un split incompleto a la vista, §2; T13.5). En `Cancelled` vuelve a intentar borrar **solo sus `.part`** (salvo los protegidos por otra unidad), por si falló el borrado de «Cancelar», y **nunca borra finales**: `cancel` no devuelve a `Pending` sus partes, así que una parte que estaba `Verified` como `.part` sigue `Verified` en la `Cancelled` tras borrarse ese `.part`, y la regla de «ya renombrada» tomaría como propio el final que haya escrito después otra unidad de la misma variante ya quitada (#87; T13.6). Si el borrado falla, devuelve el error y la unidad sigue en la lista para reintentarlo. Nunca borra los finales de una `Completed`, ni los `Preexisting`, ni los ficheros protegidos por otra unidad (#72, abajo). No emite ningún evento (`EventSink` no cambia): la UI quita la unidad al recibir el `Ok` del comando y al fusionar una lista completa (T15.3, T16.3) |

   Transiciones internas (sin orden del usuario): `Queued → Downloading → Verifying → Completed`, `Queued → Failed(_)` (comprobación de espacio, §7), `Downloading|Verifying → Failed(_)` y `Downloading → Queued` (devolver la unidad activa a la cola sin pausarla; ninguna orden de esta tabla la usa).

   **Ficheros compartidos entre unidades (#72).** `enqueue` impide que una unidad comparta destino con otra no terminal o `Failed` (§9). Aun así, como defensa en profundidad, `cancel` y `remove` no borran un fichero que otra unidad de la cola sigue necesitando. Lo que protege cada unidad depende de su estado y del de cada parte, y la clave es siempre `dest_key(spec.dest)` (T11.5), calculada sobre el destino final:
   - **`.part`:** lo protege otra unidad en un estado no terminal (`Queued`, `Downloading`, `Verifying`, `Paused`) o `Failed`, si tiene un `FileSpec` con ese destino, esté como esté esa parte. Es el mismo conjunto de estados que usa `AlreadyQueued` (§9).
   - **Final:** lo protege otra unidad en cualquier estado salvo `Cancelled`, si su `FileSpec` con ese destino está `Preexisting`, o `Verified` y ya renombrado (regla de T13.4: sin `.part` y con el destino de `expected_size` en disco).
   - **`Completed` y `Cancelled`:** una `Completed` no protege ningún `.part`, solo sus finales presentes. Una `Cancelled` no protege nada.

   Los ficheros protegidos se dejan en disco sin error (T13.2). Así, cancelar una unidad encolada de nuevo tras otra `Cancelled`, o tras una `Completed` cuyo final se borró, sí borra su `.part` (CA5). Única excepción a CA5: si el proceso muere a mitad de `finalize_unit` de la nueva unidad y se cancela después, el final que ya renombró coincide con el que la `Completed` da por suyo (mismo destino y tamaño) y se conserva, porque no se puede saber cuál de las dos lo escribió.

   La cancelación usa `tokio_util::sync::CancellationToken`.
9. **Alta (`enqueue`):** valida la petición (Contexto, #55) y construye los `FileSpec`. Devuelve `AlreadyQueued` si algún destino ya lo usa una unidad no terminal **o una unidad `Failed`** (#72): mientras exista una unidad fallida con ese destino, hay que reintentarla o quitarla antes. Solo se puede volver a encolar un destino cuya unidad anterior esté en `Completed` o `Cancelled`.
   - **Final con el tamaño esperado:** la parte queda candidata a `Preexisting`. Su hash se verifica al ejecutarse, sin descargarla ni tocarla; si no coincide, `Failed(DestinationExists)`.
   - **Final con otro tamaño:** `Err(DestinationExists)` en el alta.
   - **Qué es una candidata.** No es un estado persistido. Es una parte con `status == Pending` cuyo destino final existe con longitud igual a `expected_size`. El runner la reconoce al empezar cada ejecución (helper `is_preexisting_candidate(&FileSpec) -> bool`, que solo mira `metadata`). Si el usuario borra el final antes de que la unidad se ejecute, la parte vuelve a ser una `Pending` normal y se descarga.
   - **Orden en el runner:** devolver a `Pending` las partes `Verified` sin ficheros (T13.4), clasificar las candidatas, comprobar el espacio sin contarlas (§7), descargar las demás y, ya en `Verifying`, verificar el hash de las candidatas y marcarlas `Preexisting`.
   - **Resultado de verificar una candidata** (sobre su final, sin tocarlo): `Ok` → `Preexisting`. `Failed(SizeMismatch)` o `Failed(ChecksumMismatch)` → `Failed(DestinationExists)`, y el final no se borra nunca. Si el final ha desaparecido durante la descarga del resto (`Io` al abrirlo) → `Failed(Io { message })` con mensaje fijo (T12.6). Al reintentar, la parte ya no es candidata: es una `Pending` normal, cuenta en el espacio (§7) y se descarga.
10. **Persistencia:** `downloads.json` en `app_local_data_dir()` (decisión del usuario, 2026-11-28; ver Contexto), con `schema: 1`, `next_id` y las unidades (sin bytes, sin URL de CDN y **sin token**). Se escribe de forma atómica (`downloads.json.tmp` + `rename`) en cada cambio de estado, nunca por bytes. Sigue el patrón de `runtime/state.rs`:
    - Al arrancar, las unidades `Downloading`/`Verifying`/`Queued` se restauran como `Paused` (decisión del usuario).
    - Un fichero ilegible (no UTF-8, JSON roto o `schema` desconocido) se copia a `downloads.json.bak` y se arranca con la cola vacía. Otro error de lectura (permisos) se devuelve como error sin tocar nada.
11. **Token:** trait `TokenProvider { fn token(&self) -> Option<HfToken> }`. Lo implementa `HfService` leyendo **el token en memoria** de su `HfClient` (el que cargó `new` o puso `save_token`), **nunca el llavero**: así la descarga no hace llamadas síncronas al llavero dentro de código async (riesgo de la issue #45). La cabecera `Authorization` solo se añade si la URL tiene el mismo origen (esquema, host y puerto) que la `base_url` de HF, reutilizando `hf::url::is_trusted_target` (pasa a `pub(crate)`). El token no aparece en logs, errores, eventos ni en `downloads.json`. El token se lee en cada intento, así que un cambio en Ajustes se aplica en el siguiente intento: `transfer_with_retry` no recibe una cabecera ya calculada, sino un closure que la calcula antes de cada `transfer_once` (T9.4).
12. **Seguridad de rutas:** `relative_path` viene de HF a través de la UI. Se rechaza si es absoluta, si tiene componentes `..`, `.`, prefijos de unidad (`C:`) o raíz, barras invertidas, o si está vacía (`UnsafePath`). Se construye con `Path::join` por componente (R3). El destino es `models_dir/<owner>/<name>/<relative_path>`, con `owner`/`name` de un `RepoId` ya validado.
13. **Eventos Tauri** (mismo estilo que `runtime://progress`): `downloads://state` con un `DownloadSnapshot` y `downloads://progress` con un `DownloadProgress`. `DownloadSnapshot` lleva `version: u64`, un contador global creciente que el gestor incrementa en cada cambio de estado (no se persiste). La UI descarta un snapshot con `version` menor que el que ya tiene, así una respuesta tardía de `downloads_list` no pisa un evento más nuevo (lección de la carrera de paginación de 005, #49).
14. **Serialización:** la de serde por defecto, sin `rename_all`, como el resto de tipos de `hf` y `runtime` que ve la UI. El JSON exacto lo fijan los tests `json_*_para_la_ui` de T15, y `ui/src/lib/downloads/types.ts` lo espeja.

### Dependencias nuevas

`app-llama-core`: `tokio-util` (`CancellationToken`) y `fs4` (espacio libre; el developer fija la versión y la feature que expone `available_space`). Se declaran en `[workspace.dependencies]`. `Cargo.lock` versionado (R3). El resto ya está en el workspace.

## Criterios de aceptación

- **CA1 — Progreso.** Dado un `ProgressReporter` con un fichero de 10 MiB y un reloj falso que avanza 50 ms por bloque de 64 KiB (160 bloques, 8 s), cuando se registran todos los bloques, entonces:
  - se emiten como máximo `ceil(8000 / 250) + 2 = 34` eventos de progreso;
  - el último evento indica `downloaded_bytes == total_bytes == 10485760`;
  - `bytes_per_sec` es 1310720 (±1 %);
  - `eta_secs` es `Some(0)` al final.
- **CA2 — Reanudación con `Range`.** Dado un `.part` con los primeros 4 MiB de un fichero de 10 MiB, cuando se reanuda, entonces la petición lleva `Range: bytes=4194304-`, solo se reciben los 6 MiB restantes y el fichero final es idéntico byte a byte al original.
- **CA3 — Cortes de red.** Dado un servidor que cierra la conexión tras enviar 1 MiB en cada respuesta, cuando se descarga un fichero de 3 MiB, entonces la descarga termina con 3 peticiones encadenadas por `Range`, sin intervención, y el SHA256 coincide. Dado un servidor que corta antes de enviar ningún byte 5 veces seguidas, la transferencia acaba en `Failed(Network)`, el `FakeSleeper` registra `[1, 2, 4, 8]` s y el `.part` se conserva.
- **CA4 — Servidor sin `Range`.** Dado un `.part` de 100 KiB y un servidor que responde `200` ignorando el `Range`, cuando se reanuda, entonces el `.part` se trunca, se descarga desde 0 y el resultado es idéntico al original (sin bytes duplicados).
- **CA5 — Pausa, reanudación y cancelación.**
  - Dada una descarga en curso, cuando se pausa, entonces no se escribe ningún bloque más tras la orden, el estado es `Paused` y el `.part` se conserva.
  - Cuando se reanuda, la siguiente petición lleva `Range: bytes=<len(.part)>-`.
  - Cuando se cancela, el estado es `Cancelled` y no queda ningún `.part` ni ningún fichero final de esa unidad. Los ficheros `Preexisting` siguen intactos. Esto vale también si antes hubo otra unidad de la misma variante en `Cancelled`, o en `Completed` con el final borrado. La única excepción es la caída a mitad de `finalize_unit` descrita en el Diseño §8 (#72).
- **CA6 — Modelos divididos.**
  - Dado un `DownloadRequest` con las 3 partes de un modelo, cuando se encola, entonces se crea una única unidad con las 3, y ningún `.gguf` final aparece en disco hasta que las 3 estén verificadas.
  - El progreso agregado cubre la suma de los 3 tamaños.
  - Si falta una parte, hay duplicadas o se mezclan dos splits, se rechaza con `IncompleteSplit` y no se crea ninguna unidad.
- **CA7 — Espacio en disco.** Dado un modelo de 60 GiB (64424509440 bytes) y un `DiskSpace` falso con 40 GiB libres, cuando se va a iniciar, entonces la unidad pasa a `Failed(InsufficientSpace { needed: 65498251264, available: 42949672960 })` y el servidor de prueba recibe 0 peticiones.
- **CA8 — Verificación.**
  - Dado un fichero con el `sha256` correcto, termina en `Completed` y se renombra sin `.part`.
  - Con un `sha256` incorrecto, termina en `Failed(ChecksumMismatch)` y no queda ni el fichero final ni el `.part`.
  - Con un tamaño recibido distinto de `expected_size`, termina en `Failed(SizeMismatch)`.
  - Sin `sha256` (fichero no LFS), solo se verifica el tamaño.
- **CA9 — Gated y token.**
  - Dado un repositorio que responde `401` sin token, termina en `Failed(Unauthorized)` con 1 sola petición; con `403` desde el origen de HF, en `Failed(Gated)`.
  - Con token, la petición al origen de HF (el `MockServer` hace de HF) lleva `Authorization: Bearer …`, y la petición tras la redirección a otro origen no la lleva.
  - El token no aparece en `downloads.json`, en los `ErrorPayload` ni en los eventos.
  - Leer el token para descargar no consulta el `SecretStore` (comprobado borrando el almacén por detrás: el token en memoria sigue en uso).
- **CA10 — Persistencia.** Dada una cola con una unidad `Downloading` y otra `Queued`, cuando se recrea el gestor desde `downloads.json`, entonces las dos aparecen como `Paused` con sus ficheros y su `next_id`. Al reanudar la primera, continúa desde `len(.part)`. Un `downloads.json` ilegible se copia a `downloads.json.bak` y la cola arranca vacía sin pánico.
- **CA11 — Cola.** Dadas 3 unidades encoladas con `max_active = 1`, cuando se procesan, entonces pasan a `Downloading` en orden FIFO y nunca hay 2 en `Downloading` a la vez. Si se pausa la activa, empieza la siguiente.
- **CA12 — Revisión fijada (#55).** Dada una ficha cuyo `GET /api/models/{repo}` devuelve `sha: "<40 hex>"`, cuando se abre, entonces el árbol se pide en `tree/<sha>`, `ModelDetail.revision == "<sha>"`, y al pulsar "Descargar" el `DownloadRequest` lleva esa revisión y las URLs de descarga contienen `/resolve/<sha>/`. Sin `sha` en la respuesta, todo usa `"main"`. En `ui/src/` no queda ningún literal `"main"` de revisión.
- **CA13 — UI.**
  - Dada la vista "Descargas", cuando llegan eventos `downloads://state` y `downloads://progress`, entonces cada unidad muestra el nombre, una barra `<progress>` (sin `style` en línea: CSP estricta), los bytes (`1,2 GiB de 4,5 GiB`), la velocidad (`12,3 MiB/s`), la ETA (`3 min 20 s`) y solo los botones válidos para su estado (tabla del Diseño §8).
  - Dado un fichero de 4,5 GiB ya descargado, cuando llega el primer evento `phase: "Verify"` con 0 bytes, entonces la barra principal sigue llena (no cae a 0) y aparece «Verificando» con su propia barra y la velocidad del hash (`Verificando… 1,5 GiB/s`) (Diseño §6, test `item_verificando_no_retrocede`).
  - Dada una unidad con progreso de `Verify` (por ejemplo, 5 de 10 bytes), cuando sale de ejecución (`Failed(ChecksumMismatch)` → `Queued` al reintentar, o `Paused` durante el hash de una parte), entonces el store borra su progreso y la vista no muestra «Verificando» ni velocidad ni ETA viejas: pinta la barra y los bytes con los datos del snapshot (tests `applyState_limpia_progreso_al_salir_de_ejecucion` e `item_sin_progreso_usa_snapshot`).
  - En la ficha de un modelo, el botón "Descargar" de una variante completa está habilitado, llama a `downloads_enqueue_hf` con el `DownloadRequest` exacto de la variante y navega a "Descargas". Un error de alta (`IncompleteSplit`, `AlreadyQueued`, `DestinationExists`…) se muestra en la ficha con `role="alert"` y no navega.
  - Los tests de Vitest de formato, store y componentes pasan.
- **CA14 — Migración del runtime.** Dado el flujo de instalación de llama.cpp de 004, cuando se instala, entonces descarga con `ResumableHttpFetcher` (que usa `download::download_one`, con reintentos y verificación de tamaño y SHA256), `SimpleHttpFetcher` deja de existir, sus tests se portan con los mismos nombres y aserciones, y los tests de `install.rs` siguen en verde sin cambios.
- **CA15 — Comprobación en vivo** (manual, T19, **solo Windows**). La descarga real de `Qwen/Qwen2.5-0.5B-Instruct-GGUF` / `qwen2.5-0.5b-instruct-q4_k_m.gguf`, con una pausa a mitad, cerrar la app, abrirla y reanudar, termina en `Completed`, sin `.part`, con el SHA256 (`certutil -hashfile <f> SHA256`) igual al `lfs.oid` del árbol en la revisión usada.
- **CA16 — Gate.** `node scripts/quality-gate.mjs` en verde en Windows y en la CI de macOS, sin red en los tests (todos los HTTP contra `127.0.0.1`), sin reloj real ni `sleep` real.

## Tareas

> **Convención de tests.** Todos los tests de red usan `wiremock` o el servidor TCP de `download::test_support` en `127.0.0.1` con puerto efímero. Para simular "conexión rechazada" se usa un `std::net::TcpListener` ligado a `127.0.0.1:0` y soltado, **nunca** el puerto de un `MockServer` soltado (wiremock lo devuelve a un pool y sigue escuchando; lección de 005). El tiempo se inyecta (`FakeClock`, `SteppingClock`, `FakeSleeper`) y los ficheros van en `tempfile::TempDir` (R3). Para esperar a que un servidor reciba una petición se usa un bucle con `tokio::task::yield_now()` sobre `received_requests()`, nunca `sleep`. Los asertos de error comparan la variante exacta con `matches!` (lección `aserto-debil`). Cada fila de las tablas es un test; si un caso se fusiona con otro, el reviewer lo trata como `reserva-sin-test`.

### T1 — Tipos de dominio, errores y rutas seguras
- **Ficheros/módulos:** `crates/core/src/download/mod.rs`, `model.rs`, `error.rs`, `paths.rs`; `crates/core/Cargo.toml` y `Cargo.toml` raíz (`tokio-util`, `fs4`)
- **Depende de:** —
- **Cubre:** CA8, CA9 (tipos), base de todas
- **Subtareas:**
  - [x] T1.1 `DownloadId(u64)`, `FileSpec`, `FileStatus` (`Pending`, `Verified`, `Preexisting`), `DownloadUnit`, `UnitState` y `FailureKind` (`Network`, `Unauthorized`, `Gated`, `NotFound`, `ChecksumMismatch`, `SizeMismatch { expected, actual }`, `InsufficientSpace { needed, available }`, `UnexpectedContentRange`, `DestinationExists`, `Io { message }`), con `Serialize`/`Deserialize` por defecto.
  - [x] T1.2 `DownloadError` con `thiserror`: `Failed(FailureKind)`, `Cancelled`, `UnsafePath`, `IncompleteSplit`, `InvalidRequest(&'static str)`, `AlreadyQueued`, `DestinationExists`, `InvalidTransition`, `UnknownDownload`, `Io(std::io::Error)`. Su `Display` nunca incluye cabeceras, URLs firmadas ni el token.
  - [x] T1.3 `safe_join(base, relative) -> Result<PathBuf, DownloadError>` y `part_path(final) -> PathBuf` (añade `.part` al nombre completo, como `runtime/fetch.rs`).
  - [x] T1.4 `UnitState::can_transition(&self, to) -> bool` según la tabla del Diseño §8, más las transiciones internas `Queued → Downloading → Verifying → Completed`, `Downloading|Verifying → Failed` y `Downloading → Queued` (devolver la unidad activa a la cola sin pausarla; reanudar tras una pausa es `Paused → Queued`, §8). `Queued → Failed` no está en T1: la añade T12.3.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `safe_join_ruta_simple` | base `tmp`, `"a/b.gguf"` | `tmp.join("a").join("b.gguf")` |
  | `safe_join_rechaza_padre` | `"../x.gguf"` | `Err(UnsafePath)` |
  | `safe_join_rechaza_padre_intermedio` | `"a/../../x"` | `Err(UnsafePath)` |
  | `safe_join_rechaza_punto` | `"./x.gguf"` y `"a/./x.gguf"` | `Err(UnsafePath)` en los dos |
  | `safe_join_rechaza_absoluta` | `"/etc/x"` y `"C:\\x"` y `"C:x"` | `Err(UnsafePath)` en los tres |
  | `safe_join_rechaza_barra_invertida` | `"a\\b.gguf"` | `Err(UnsafePath)` |
  | `safe_join_rechaza_vacia` | `""` y `"a//b"` | `Err(UnsafePath)` en los dos |
  | `part_path_anade_sufijo` | `m.gguf` | `m.gguf.part` |
  | `transicion_queued_a_downloading` | `Queued → Downloading` | `true` |
  | `transicion_completed_es_terminal` | `Completed → Queued` | `false` |
  | `transicion_failed_a_queued` | `Failed(Network) → Queued` | `true` |
  | `transicion_verifying_no_pausa` | `Verifying → Paused` | `false` |
  | `transicion_paused_a_cancelled` | `Paused → Cancelled` | `true` |
  | `unit_state_serde_ida_vuelta` | `Failed(InsufficientSpace { needed: 65498251264, available: 1 })` | JSON y vuelta iguales, `u64` sin pérdida |
  | `error_display_sin_token` | todas las variantes de `DownloadError` construidas en un test donde el token es `hf_` + 34 caracteres | ningún `to_string()` contiene el token |

### T2 — Validación de las partes de una unidad
- **Ficheros/módulos:** `crates/core/src/download/split.rs`; `crates/core/src/hf/quant.rs` (`strip_gguf_extension` y `parse_split_suffix` pasan de `pub(super)` a `pub(crate)`)
- **Depende de:** T1
- **Cubre:** CA6
- **Subtareas:**
  - [x] T2.1 `validate_parts(paths: &[&str]) -> Result<(), DownloadError>` reutilizando el parser de 005 (sin un segundo parser, para no duplicar la detección de partes): una ruta sin sufijo de split vale sola; con sufijo, todas deben compartir carpeta, prefijo y `total`, y sus índices ser exactamente `1..=total` sin duplicados. Cualquier otra cosa: `IncompleteSplit`. Lista vacía: `InvalidRequest`. Ruta sin `.gguf`: `InvalidRequest`.
  - [x] T2.2 `display_name(first_path) -> String`: la ruta sin sufijo de split ni extensión (igual que `GgufVariant::name`).
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `valida_fichero_unico` | `["m-Q4_K_M.gguf"]` | `Ok(())` |
  | `valida_split_completo` | `["m-00001-of-00003.gguf", "m-00002-of-00003.gguf", "m-00003-of-00003.gguf"]` | `Ok(())` |
  | `valida_split_desordenado` | partes 3, 1, 2 de 3 | `Ok(())` |
  | `rechaza_parte_que_falta` | partes 1 y 3 de 3 | `Err(IncompleteSplit)` |
  | `rechaza_duplicados` | partes 1, 1, 2 de 2 | `Err(IncompleteSplit)` |
  | `rechaza_prefijos_mezclados` | `a-00001-of-00002.gguf`, `b-00002-of-00002.gguf` | `Err(IncompleteSplit)` |
  | `rechaza_carpetas_mezcladas` | `x/m-00001-of-00002.gguf`, `y/m-00002-of-00002.gguf` | `Err(IncompleteSplit)` |
  | `rechaza_totales_mezclados` | `m-00001-of-00002.gguf`, `m-00002-of-00003.gguf` | `Err(IncompleteSplit)` |
  | `rechaza_unico_mas_split` | `m.gguf`, `m-00001-of-00001.gguf` | `Err(IncompleteSplit)` |
  | `rechaza_lista_vacia` | `[]` | `Err(InvalidRequest(_))` |
  | `rechaza_no_gguf` | `["README.md"]` | `Err(InvalidRequest(_))` |
  | `display_name_split_en_carpeta` | `"BF16/Qwen3.8-27B-BF16-00001-of-00002.gguf"` | `"BF16/Qwen3.8-27B-BF16"` |
  | `display_name_unico` | `"m-Q4_0.gguf"` | `"m-Q4_0"` |

### T3 — Reloj, progreso (velocidad, ETA) y limitador de eventos
- **Ficheros/módulos:** `crates/core/src/download/clock.rs`, `progress.rs`
- **Depende de:** T1
- **Cubre:** CA1
- **Subtareas:**
  - [x] T3.1 Trait `Clock: Send + Sync { fn now(&self) -> Duration }` (monotónico desde un origen), `SystemClock` (`Instant`), y en `cfg(test)`: `FakeClock` (avanza con `advance(d)`) y `SteppingClock` (avanza un paso fijo en cada `now()`).
  - [x] T3.2 `ProgressTracker::new(total, already_done)`, `record(now, bytes)` y `snapshot(now) -> Progress { downloaded, total, bytes_per_sec, eta_secs: Option<u64> }`. Ventana deslizante de 5 s. Los bytes `already_done` no cuentan para la velocidad.
  - [x] T3.3 `Throttle::new(250 ms)`, `should_emit(now) -> bool` (el primero siempre) y `force()` para el evento final.
  - [x] T3.4 `ProgressReporter { tracker, throttle, clock }` con `record(bytes) -> Option<Progress>` y `finish() -> Progress`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `velocidad_constante` | 1 MiB cada 100 ms durante 2 s | `bytes_per_sec == 10485760` |
  | `velocidad_ventana_5s_olvida_pasado` | 10 MiB/s durante 5 s, después 1 MiB/s durante 6 s | `bytes_per_sec == 1048576` (±1 %) |
  | `velocidad_sin_muestras` | recién creado | `bytes_per_sec == 0`, `eta_secs == None` |
  | `eta_calculada` | total 100 MiB, hechos 40 MiB, 10 MiB/s | `eta_secs == Some(6)` |
  | `eta_redondea_hacia_arriba` | faltan 15 MiB a 10 MiB/s | `Some(2)` |
  | `eta_cero_al_terminar` | hechos == total | `eta_secs == Some(0)` |
  | `reanudacion_no_infla_velocidad` | `already_done` 4 GiB, después 1 MiB/s durante 2 s | `bytes_per_sec ≈ 1048576`, `downloaded` incluye los 4 GiB |
  | `progreso_mas_de_50gb` | total 64424509440, hechos 60000000000, 100 MiB/s | sin desbordamiento, `eta_secs == Some(43)` |
  | `throttle_limita_a_4hz` | `should_emit` cada 10 ms durante 1 s | 4 o 5 `true` en total |
  | `throttle_primer_evento_inmediato` | primera llamada | `true` |
  | `throttle_force_siempre` | `force()` justo después de emitir | `true` |
  | `reporter_ca1_10mib` | 160 × 64 KiB, `FakeClock` +50 ms por bloque, `finish()` | ≤ 34 eventos; último `downloaded == total == 10485760`, `bytes_per_sec` 1310720 ±1 %, `eta_secs == Some(0)` |

### T4 — Cabeceras `Range`/`Content-Range` y clasificación de respuestas
- **Ficheros/módulos:** `crates/core/src/download/http.rs`
- **Depende de:** T1
- **Cubre:** CA2, CA4, CA9 (lógica pura)
- **Subtareas:**
  - [x] T4.1 `range_header(offset) -> Option<String>` (`None` si es 0).
  - [x] T4.2 `parse_content_range(&str) -> Result<ContentRange { start, end, total: Option<u64> }, InvalidContentRange>`. `InvalidContentRange` es un struct unitario con `thiserror` (no `()`, para no caer en `clippy::result_unit_err`).
  - [x] T4.3 `classify(ResponseInfo { status, content_range, from_hf_origin }, offset, expected) -> ResponseAction` (`Append`, `RestartFromZero`, `AlreadyComplete`, `Fail(FailureKind)`, `Retry`). Un `416` con offset 0 da `Fail(UnexpectedContentRange)`, no `RestartFromZero` (Diseño §3).
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `range_offset_cero` | 0 | `None` |
  | `range_offset_grande` | 60000000000 | `Some("bytes=60000000000-")` |
  | `content_range_valido` | `"bytes 100-199/1000"` | `{100, 199, Some(1000)}` |
  | `content_range_total_desconocido` | `"bytes 100-199/*"` | `total: None` |
  | `content_range_malformado` | `"bytes=100-199/1000"`, `"items 1-2/3"`, `""`, `"bytes 200-100/1000"` | `Err` en los cuatro |
  | `content_range_mas_de_4gb` | `"bytes 5000000000-64424509439/64424509440"` | parseo correcto en `u64` |
  | `classify_206_coherente` | 206, inicio == offset, total == expected | `Append` |
  | `classify_206_inicio_distinto` | 206, inicio != offset | `Fail(UnexpectedContentRange)` |
  | `classify_206_total_distinto` | 206, total != expected | `Fail(SizeMismatch { .. })` |
  | `classify_206_sin_content_range` | 206 sin cabecera | `Fail(UnexpectedContentRange)` |
  | `classify_200_con_offset` | 200, offset 4 MiB | `RestartFromZero` |
  | `classify_200_sin_offset` | 200, offset 0 | `Append` |
  | `classify_416_completo` | 416, offset == expected | `AlreadyComplete` |
  | `classify_416_incompleto` | 416, offset 500, expected 1000 | `RestartFromZero` |
  | `classify_416_offset_cero_no_reinicia` | 416, offset 0, expected 1000 | `Fail(UnexpectedContentRange)` |
  | `classify_401_hf` | 401, `from_hf_origin` | `Fail(Unauthorized)` |
  | `classify_403_hf` | 403, `from_hf_origin` | `Fail(Gated)` |
  | `classify_403_cdn_reintenta` | 403, no `from_hf_origin` | `Retry` |
  | `classify_404` | 404 | `Fail(NotFound)` |
  | `classify_5xx_y_429` | 500, 503, 429 | `Retry` en los tres |

### T5 — Transferencia de un fichero con `.part`, `Range` y cancelación
- **Ficheros/módulos:** `crates/core/src/download/transfer.rs`, `crates/core/src/download/test_support.rs` (`#[cfg(test)]`)
- **Depende de:** T3, T4
- **Cubre:** CA2, CA4, CA5 (parte de un fichero), CA8 (exceso de tamaño)
- **Subtareas:**
  - [x] T5.1 `HttpFetcher { client: reqwest::Client, hf_origin: Option<Url> }`. El `Client` lo construye quien llama: en producción, `connect_timeout` 30 s, `read_timeout` 60 s, **sin timeout total** (el cliente de `commands/hf.rs` tiene un timeout total de 30 s y **no** sirve para descargas), redirecciones por defecto (máximo 10). `from_hf_origin` se calcula sobre la URL final (`Response::url()`) con valor por defecto seguro (Diseño §3): `true` salvo que `hf_origin` sea `Some` y el origen de la URL final sea distinto y no opaco. Así un `401`/`403` sin origen de HF conocido acaba en `Unauthorized`/`Gated`, nunca en `Failed(Network)` tras 5 reintentos.
  - [x] T5.2 `transfer_once(fetcher, spec, auth: Option<HeaderValue>, cancel, on_bytes) -> Result<TransferOutcome, DownloadError>`: crea el directorio padre, abre el `.part` en append, manda `Range` según su longitud, aplica `classify`, escribe por bloques con `select!` contra `cancel.cancelled()`, corta antes de superar `expected_size` y hace `flush` al salir, también si se cancela.
  - [x] T5.3 `TransferOutcome`: `Completed`, `Cancelled`, `Interrupted { bytes_written }`, `Restart` (tras `RestartFromZero`).
  - [x] T5.4 Conversión única de `reqwest::Error` a `FailureKind`/`DownloadError` según la regla del Diseño §3 ("Errores de `reqwest` sin URL"): llama a `without_url()` antes de formatear o usa un mensaje propio; nunca `to_string()` sobre el error con URL. T6 la reutiliza.
- **Tests unitarios** (wiremock):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `descarga_completa_desde_cero` | 256 KiB, `200` | `.part` idéntico, sin cabecera `Range` en la petición |
  | `reanuda_con_range` | `.part` de 100 KiB de 256 KiB; mock que exige `Range: bytes=102400-` y responde 206 con `Content-Range` correcto | `.part` final idéntico al original |
  | `servidor_ignora_range_trunca` | `.part` de 100 KiB, servidor `200` con el cuerpo completo | `.part` == original (sin duplicar bytes) |
  | `range_416_ya_completo` | `.part` completo, servidor `416` | `Completed` sin escribir |
  | `content_range_incoherente_falla` | 206 con `bytes 0-…` cuando el offset es 100 KiB | `Err(Failed(UnexpectedContentRange))` |
  | `servidor_envia_de_mas_corta` | `expected` 1000, servidor `200` con 1200 bytes | `Err(Failed(SizeMismatch { expected: 1000, .. }))`; el `.part` nunca supera 1000 bytes |
  | `cancelacion_entre_bloques` | `cancel` disparado en `on_bytes` tras el primer bloque | `Cancelled`; ningún byte escrito después de la orden; el `.part` es prefijo del original |
  | `cancelacion_durante_espera_de_respuesta` | mock con `set_delay(60 s)`; se cancela cuando `received_requests()` tiene 1 | `Cancelled` sin esperar al retraso |
  | `sigue_redireccion` | mock `/a` 302 → `/b` con el cuerpo | descarga correcta desde `/b` |
  | `respuesta_404` | 404 | `Err(Failed(NotFound))` |
  | `sin_origen_hf_401_es_unauthorized` | `HttpFetcher` con `hf_origin: None`, mock 401 | `Err(Failed(Unauthorized))` |
  | `sin_origen_hf_403_es_gated` | `HttpFetcher` con `hf_origin: None`, mock 403 | `Err(Failed(Gated))` |
  | `crea_directorio_padre` | `dest` en `tmp/x/y/m.gguf` sin crear | `.part` creado en `tmp/x/y/` |
  | `error_reqwest_no_filtra_url_firmada` | `reqwest::Error` real obtenido sin red: petición a `http://127.0.0.1:<puerto de un TcpListener soltado>/x?X-Amz-Signature=secreto123`, pasado por la conversión de T5.4 | ni el `Display`/`Debug` del `DownloadError` ni el JSON de `serde_json` del `FailureKind` resultante contienen `Signature`, `secreto123` ni la URL (`127.0.0.1:<puerto>/x`) |

### T6 — Reintentos ante cortes con backoff inyectado
- **Ficheros/módulos:** `crates/core/src/download/retry.rs`, `transfer.rs`, `test_support.rs` (servidor TCP que corta)
- **Depende de:** T5
- **Cubre:** CA3
- **Subtareas:**
  - [x] T6.1 Trait `Sleeper: Send + Sync { async fn sleep(&self, d: Duration) }` (con `async-trait`, como `ArtifactFetcher`), `TokioSleeper` y `FakeSleeper` (registra las duraciones y vuelve al momento).
  - [x] T6.2 `RetryPolicy { max_attempts_without_progress: 5, backoff: [1, 2, 4, 8] s }` y `transfer_with_retry(...)`: reintenta `Retry`/`Interrupted`, reinicia el contador solo si hubo progreso (`len(.part)` supera la marca máxima de la llamada, Diseño §4), gestiona `Restart` y el reinicio único por `UnexpectedContentRange`, y la espera compite con `cancel.cancelled()`. **Todo `Restart`** (incluido el reinicio único por `UnexpectedContentRange`) cuenta como intento sin progreso: suma 1 al contador, espera el backoff que toque y no lo pone a 0. Agotados los 5 intentos, `Failed(Network)`, sin bucle infinito. El `Failed(Network)` final y cualquier error de `reqwest` se construyen con la conversión de T5.4 (regla del Diseño §3: nunca `reqwest::Error::to_string()`).
  - [x] T6.3 `test_support::cutting_server(body, cut_after: Vec<Option<usize>>)`: un `tokio::net::TcpListener` en `127.0.0.1:0` que responde a cada conexión con `Content-Length`/`Content-Range` completos, admite `Range` y cierra el socket tras N bytes (`None` = no corta). Con la opción `ignore_range: true` responde siempre `200` con el cuerpo desde 0, aunque llegue un `Range`. Registra las cabeceras `Range` recibidas. Hace falta porque wiremock no puede cortar a mitad del cuerpo.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `cortes_con_progreso_terminan` | 3 MiB, corte cada 1 MiB | `Completed` en 3 peticiones; `Range` recibidos: ninguno, `bytes=1048576-`, `bytes=2097152-` |
  | `cortes_sin_progreso_agotan` | 5 cortes antes del primer byte | `Err(Failed(Network))` tras 5 peticiones; `FakeSleeper` registra `[1, 2, 4, 8]` s; `.part` conservado |
  | `progreso_reinicia_contador` | 4 cortes sin bytes, 1 con bytes, 4 sin bytes, éxito | `Completed` |
  | `no_reintenta_401` | 401 | 1 petición, `Err(Failed(Unauthorized))` |
  | `reintenta_503` | 503, 503, 200 | `Completed`, sleeps `[1, 2]` s |
  | `content_range_incoherente_reinicia_una_vez` | 1.ª respuesta 206 incoherente, 2.ª 200 correcta | `Completed`; la 2.ª petición sin `Range` |
  | `content_range_incoherente_dos_veces_falla` | dos 206 incoherentes | `Err(Failed(UnexpectedContentRange))` |
  | `reinicios_repetidos_acaban_fallando` | `cutting_server` de 3 MiB con `ignore_range: true` que corta cada respuesta tras 1 MiB (siempre `200`) | `Err(Failed(Network))` tras exactamente 6 peticiones (la 1.ª con progreso; después reinicio, re-descarga de 1 MiB sin superar la marca, reinicio…); `FakeSleeper` registra `[1, 2, 4, 8]` s; el test termina sin bucle infinito |
  | `respuesta_416_tras_reinicio_falla` | `.part` de 100 KiB; mock que responde siempre `416` | 1.ª `Restart` (sleep `[1]`), 2.ª petición sin `Range` → `Err(Failed(UnexpectedContentRange))`; 2 peticiones |
  | `sin_origen_hf_401_sin_reintento` | `HttpFetcher` con `hf_origin: None`, mock 401 | `Err(Failed(Unauthorized))` con 1 sola petición y `FakeSleeper` vacío |
  | `cdn_403_tras_redireccion_reintenta` | `hf_origin` = mock A; A 302 → mock B (otro puerto), B siempre `403` | `Err(Failed(Network))` tras 5 peticiones a A (no `Gated`), sleeps `[1, 2, 4, 8]` s |
  | `conexion_rechazada_es_transitoria` | URL a un puerto de un `TcpListener` soltado | `Err(Failed(Network))` tras 5 intentos |
  | `fallo_red_agotado_no_filtra_url_firmada` | URL `http://127.0.0.1:<puerto de un TcpListener soltado>/x?X-Amz-Signature=secreto123`, 5 intentos | `Err(Failed(Network))`; ni el `Display`/`Debug` del error ni el JSON del `FailureKind` contienen `Signature`, `secreto123` ni la URL |
  | `cancel_durante_backoff` | `FakeSleeper` que dispara `cancel` al dormir | `Cancelled`, sin más peticiones |

### T7 — Verificación de tamaño y SHA256 y renombrado final
- **Ficheros/módulos:** `crates/core/src/download/verify.rs`
- **Depende de:** T1
- **Cubre:** CA8
- **Subtareas:**
  - [x] T7.1 `sha256_file(path, cancel, on_bytes) -> Result<String, DownloadError>`: `spawn_blocking`, bloques de 1 MiB, hex en minúsculas, cancelable entre bloques.
  - [x] T7.2 `verify_file(part, expected_size, sha256: Option<&str>, cancel, on_bytes) -> Result<(), DownloadError>`: comprueba primero el tamaño (sin hashear si no cuadra).
  - [x] T7.3 `finalize_unit(files)`: renombra todos los `.part` `Verified` a su nombre final. Si un destino ya existe: `DestinationExists`, sin sobrescribir.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `sha256_vector_conocido` | fichero `"abc"` | `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad` |
  | `sha256_fichero_vacio` | 0 bytes | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
  | `sha256_mayor_que_bloque` | 3 MiB + 7 bytes deterministas (`i % 251`) | igual que `Sha256::digest` en memoria |
  | `verify_ok` | tamaño y hash correctos | `Ok(())` |
  | `verify_hash_mayusculas` | hash esperado en mayúsculas | `Ok(())` |
  | `verify_tamano_distinto` | 1 byte de menos | `Err(Failed(SizeMismatch { .. }))` y `on_bytes` nunca llamado |
  | `verify_hash_distinto` | hash de otro contenido | `Err(Failed(ChecksumMismatch))` |
  | `verify_sin_sha_solo_tamano` | `sha256: None`, tamaño correcto | `Ok(())` |
  | `finalize_renombra_todas` | 3 `.part` verificados | 3 ficheros finales, 0 `.part` |
  | `finalize_no_sobrescribe` | destino ya existente | `Err(DestinationExists)`, destino intacto |
  | `sha256_cancelable` | `cancel` disparado en `on_bytes` tras el primer bloque | `Err(Cancelled)`, sin pánico |

### T8 — Comprobación de espacio en disco
- **Ficheros/módulos:** `crates/core/src/download/disk.rs`
- **Depende de:** T1
- **Cubre:** CA7
- **Subtareas:**
  - [x] T8.1 Trait `DiskSpace: Send + Sync { fn available(&self, path: &Path) -> io::Result<u64> }`, `Fs4DiskSpace` (pregunta por el primer ancestro existente de `path`) y `FakeDiskSpace` en `cfg(test)`.
  - [x] T8.2 `required_bytes(files, part_lens) -> u64` y `check_space(disk, dest, required) -> Result<(), DownloadError>`, con `SPACE_MARGIN = 1 GiB`.
- **Nota (#73):** `disk.rs` no distingue las candidatas a `Preexisting`, porque llegan como `Pending`. Las excluye el runner antes de llamar a `required_bytes`, y es también el runner quien omite `check_space` cuando el resultado es 0 (Diseño §7; T12.2). T8 no cambia por esto.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `required_sin_parciales` | 3 × 20 GiB | 64424509440 |
  | `required_descuenta_part` | 60 GiB, `.part` de 10 GiB | 53687091200 |
  | `required_ignora_verificados_y_preexistentes` | una parte `Verified` y otra `Preexisting` | no cuentan |
  | `espacio_insuficiente_60gb` | necesita 60 GiB, libres 40 GiB | `Err(Failed(InsufficientSpace { needed: 65498251264, available: 42949672960 }))` |
  | `espacio_justo_con_margen` | libres == necesario + 1 GiB | `Ok(())` |
  | `espacio_justo_sin_margen` | libres == necesario | `Err(Failed(InsufficientSpace { .. }))` |
  | `fs4_directorio_inexistente_usa_ancestro` | `tmp/no/existe` | `Ok(n)` con `n > 0` (lee el disco real de `TempDir`, sin red) |

### T9 — Token de HF y política de origen
- **Ficheros/módulos:** `crates/core/src/download/auth.rs`, `transfer.rs`, `retry.rs` (T9.4); `crates/core/src/hf/service.rs` (`impl TokenProvider for HfService` y `current_token()`), `crates/core/src/hf/client.rs` (`pub(crate) fn token(&self) -> Option<&HfToken>`), `crates/core/src/hf/url.rs` (`is_trusted_target` pasa a `pub(crate)`)
- **Depende de:** T5, T6 (T9.4 cambia la firma de `transfer_with_retry`)
- **Cubre:** CA9
- **Subtareas:**
  - [x] T9.1 Trait `TokenProvider: Send + Sync { fn token(&self) -> Option<HfToken> }`. `HfService::current_token()` clona el token del `HfClient` en memoria (lectura del `RwLock`, sin `await` y sin tocar el `SecretStore`).
  - [x] T9.2 `auth_header_for(url, hf_origin, provider) -> Option<HeaderValue>` con `is_trusted_target(url, hf_origin)`, `Bearer <token>` y `set_sensitive(true)`.
  - [x] T9.3 Test de que la redirección a otro origen no reenvía `Authorization`. Si fallara con `reqwest 0.13`, se añade una política de redirección propia que la quite.
  - [x] T9.4 **Token leído en cada intento (Diseño §11).** `transfer_with_retry` (`retry.rs`) cambia `auth: Option<HeaderValue>` por `auth: &(dyn Fn() -> Option<HeaderValue> + Send + Sync)` y lo evalúa justo antes de **cada** llamada a `transfer_once` (también tras un `Restart` o un backoff), nunca una sola vez por fichero. `transfer_once` sigue recibiendo `Option<HeaderValue>`. `auth.rs` añade `auth_source(url, hf_origin, provider) -> impl Fn() -> Option<HeaderValue> + Send + Sync`, que llama a `auth_header_for` en cada invocación: es lo que usa T12.2. Los tests existentes de `retry.rs` pasan `&|| None` (o el closure que corresponda) sin cambiar sus asertos, y T18 (`download_one`, sin token) pasa `&|| None`. (T12.4 agrupa después `on_bytes` y `on_restart` en `TransferObserver`; `auth` no cambia.)
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `cabecera_solo_para_origen_hf` | `https://huggingface.co/x`, origen `https://huggingface.co`, token válido | `Some("Bearer hf_…")`, `is_sensitive()` |
  | `sin_cabecera_otro_host` | `https://cdn-lfs.example/x` | `None` |
  | `sin_cabecera_host_parecido` | `https://huggingface.co.evil.test/x` | `None` |
  | `sin_cabecera_otro_puerto_o_esquema` | `http://huggingface.co/x` y `https://huggingface.co:8443/x` | `None` en los dos |
  | `sin_token_sin_cabecera` | provider sin token | `None` |
  | `peticion_lleva_bearer` | wiremock (origen HF = `MockServer`) que exige `Authorization` | 200 y descarga correcta |
  | `redireccion_no_filtra_token` | mock A (`127.0.0.1`) 302 → mock B (otro `MockServer`, otro puerto) | B no recibe `Authorization` |
  | `gated_401_sin_reintento` | 401 desde el origen HF | `Err(Failed(Unauthorized))`, 1 petición |
  | `gated_403` | 403 desde el origen HF | `Err(Failed(Gated))` |
  | `hf_service_token_en_memoria` (en `service.rs`) | `HfService` con `MemoryStore::with_token`; después `service.store.delete()` por detrás | `current_token()` sigue devolviendo el token: no consulta el almacén |
  | `hf_service_token_tras_borrar` (en `service.rs`) | `delete_token()` | `current_token() == None` |
  | `token_se_relee_en_cada_intento` (en `retry.rs`) | `TokenProvider` de prueba con `Mutex<Option<HfToken>>` y token `hf_A…`; `auth_source` sobre la URL del `MockServer` (origen HF); mock que responde `503` la 1.ª vez y `200` después; un `Sleeper` de prueba que, al dormir, cambia el token del provider a `hf_B…` | `Completed`; `received_requests()` tiene 2 peticiones: la 1.ª con `Authorization: Bearer hf_A…` y la 2.ª con `Authorization: Bearer hf_B…` |
  | `token_borrado_entre_intentos` (en `retry.rs`) | igual, pero el `Sleeper` pone el token del provider a `None` | 2 peticiones: la 1.ª con `Authorization`, la 2.ª **sin** `Authorization` |
  | `auth_source_evalua_en_cada_llamada` (en `auth.rs`) | `auth_source` con el provider de prueba; se llama, se cambia el token, se vuelve a llamar | la 1.ª llamada devuelve `Bearer hf_A…` y la 2.ª `Bearer hf_B…` |

### T10 — Persistencia de la cola
- **Ficheros/módulos:** `crates/core/src/download/store.rs`
- **Depende de:** T1
- **Cubre:** CA10
- **Subtareas:**
  - [x] T10.1 `QueueFile { schema: 1, next_id, units }` con `serde`.
  - [x] T10.2 Trait `QueuePersistence: Send + Sync { fn load(&self) -> Result<QueueFile, DownloadError>; fn save(&self, &QueueFile) -> Result<(), DownloadError> }` y `JsonQueueStore::new(dir)`, con el patrón de `runtime/state.rs`: `load` normaliza `Downloading`/`Verifying`/`Queued` a `Paused`; fichero ilegible → copia a `downloads.json.bak` y cola vacía; otro error de E/S → `Err` sin tocar nada; `save` crea el directorio y escribe `.tmp` + `rename`. `RecordingStore` (en memoria, cuenta los `save`) en `cfg(test)`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `guardar_y_cargar_ida_vuelta` | 2 unidades `Completed` y `Failed(Network)`, `next_id` 7 | estructura igual tras cargar |
  | `carga_normaliza_activos_a_paused` | `Downloading`, `Queued`, `Verifying`, `Completed` | `Paused`, `Paused`, `Paused`, `Completed` |
  | `fichero_inexistente_cola_vacia` | sin fichero | cola vacía, `next_id` 1, sin crear ningún fichero |
  | `json_corrupto_se_aparta` | `"{no json"` | cola vacía y `downloads.json.bak` con ese contenido |
  | `bytes_no_utf8_se_apartan` | `[0xff, 0xfe, 0x00]` | cola vacía y `.bak` con esos bytes |
  | `schema_desconocido_se_aparta` | `schema: 99` | igual que corrupto |
  | `guardado_atomico_no_deja_tmp` | `save` | no queda `downloads.json.tmp` y el contenido es válido |
  | `no_persiste_token_ni_url_cdn` | unidad completa guardada en una prueba donde el provider tiene un token `hf_…` | el JSON no contiene el token ni `cas-bridge` ni `Authorization` |

### T11 — Gestor: alta de unidades y consulta
- **Ficheros/módulos:** `crates/core/src/download/manager.rs`, `events.rs`, `request.rs`
- **Depende de:** T2, T7, T8, T9, T10
- **Cubre:** CA6, CA9 (validación)
- **Subtareas:**
  - [x] T11.1 Trait `EventSink: Send + Sync { fn state(&self, &DownloadSnapshot); fn progress(&self, &DownloadProgress) }` y `RecordingSink` en `cfg(test)`. `DownloadSnapshot { id, version, repo_id, revision, display_name, state, total_bytes, files: Vec<FileSnapshot { relative_path, size, status }> }`; `DownloadProgress { id, downloaded_bytes, total_bytes, bytes_per_sec, eta_secs: Option<u64>, phase: Download | Verify }`.
  - [x] T11.2 `UnitRequest { repo: RepoId, revision, parts: Vec<PartRequest { relative_path, size, sha256 }> }` y `ManagerDeps { fetcher, clock, sleeper, disk, tokens, store, sink, models_dir, hf_base_url, max_active }`.
  - [x] T11.3 `hf::url::resolve_url(base, repo, revision, path) -> Url` con `endpoint` (cada segmento de `path` codificado, `/` conservada).
  - [x] T11.4 `DownloadManager::new(deps) -> Result<Arc<Self>, DownloadError>` (carga la cola con `store.load()`) y `enqueue(UnitRequest) -> Result<DownloadId, DownloadError>`: valida las partes (T2) y las rutas (T1), construye la URL con `resolve_url` sobre `hf_base_url` y el destino, rechaza `AlreadyQueued`/`DestinationExists` (Diseño §9), persiste y emite `Queued`. `list() -> Vec<DownloadSnapshot>`.
  - [x] T11.5 `AlreadyQueued` no compara `PathBuf` en bruto, sino una clave normalizada `dest_key(&Path) -> String` (en todas las plataformas, también en la CI de Linux/macOS): cada componente pasado a mayúsculas carácter a carácter, sin reglas de contexto (`chars().flat_map(char::to_uppercase)`), y sin los puntos ni espacios finales, porque Windows y macOS no distinguen mayúsculas y Windows quita `.` y ` ` finales (`x.gguf.` acaba en `x.gguf`). **No se usa `str::to_lowercase`** (#79): aplica la regla de la sigma final (`Σ` pasa a `ς` o a `σ` según el contexto), mientras que NTFS compara con la mayúscula simple de cada carácter, así que `QΣ/m.gguf` y `qσ/m.gguf` son el mismo fichero en Windows pero tendrían claves distintas y volvería a darse #72. Con mayúsculas, `ß` acaba en `SS`, lo que solo puede provocar un rechazo de más (`AlreadyQueued` innecesario), que es seguro. La comprobación se aplica entre las partes de la propia petición y frente a las unidades no terminales **y las `Failed`** (Diseño §9, #72). Solo quedan fuera las unidades `Completed` y `Cancelled`. `dest_key` es `pub(super)` porque T13.2 y T13.5 la reutilizan para la defensa de borrado (Diseño §8). Ese mismo conjunto de estados (no terminales o `Failed`) es el que protege los `.part` en T13.2; conviene exponerlo como un único predicado (`UnitState::blocks_destination()` o similar) que usen las dos tareas.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `enqueue_crea_unidad_queued` | 1 parte válida | `Ok(DownloadId(1))`; 1 evento `Queued`; `list()` con 1 unidad; `RecordingStore` con 1 `save` |
  | `enqueue_ids_crecientes` | 2 altas | ids 1 y 2; `next_id` persistido 3 |
  | `enqueue_dividido_una_unidad` | 3 partes de 1 MiB | 1 unidad, 3 `files`, `total_bytes == 3145728` |
  | `enqueue_split_incompleto` | partes 1 y 3 de 3 | `Err(IncompleteSplit)`, `list()` vacía, 0 `save` |
  | `enqueue_ruta_insegura` | `relative_path` `"../x.gguf"` | `Err(UnsafePath)` |
  | `enqueue_duplicado_rechazado` | el mismo destino dos veces (la 1.ª en `Queued`) | 2.ª `Err(AlreadyQueued)` |
  | `enqueue_duplicado_mayusculas_rechazado` | 1.ª `"Q4/M.gguf"` en `Queued`; 2.ª `"q4/m.GGUF"` del mismo repo | 2.ª `Err(AlreadyQueued)` |
  | `enqueue_duplicado_punto_final_rechazado` | 1.ª `"Q4/m.gguf"` en `Queued`; 2.ª `"Q4./m.gguf"` y 3.ª `"Q4 /m.gguf"` (en la carpeta, porque `"m.gguf."` ya lo rechaza T2 por no acabar en `.gguf`) | 2.ª y 3.ª `Err(AlreadyQueued)` |
  | `dest_key_normaliza` | `a/B.gguf`, `A/b.gguf.`, `a /b.gguf ` | las tres claves iguales (`"A/B.GGUF"`, en mayúsculas) |
  | `dest_key_sigma_final` | `QΣ/m.gguf`, `qσ/m.gguf`, `qς/m.gguf` | las tres claves iguales (`"QΣ/M.GGUF"`) |
  | `enqueue_duplicado_sigma_rechazado` | 1.ª `"QΣ/m.gguf"` en `Queued`; 2.ª `"qσ/m.gguf"` del mismo repo | 2.ª `Err(AlreadyQueued)` |
  | `enqueue_tras_completed_permitido` | mismo destino con la 1.ª `Completed` y el final borrado | `Ok` |
  | `enqueue_rechaza_destino_de_unidad_fallida` | `RecordingStore` precargado con una unidad `Failed(Network)` de `"Q4/m-00001-of-00002.gguf"` y `"Q4/m-00002-of-00002.gguf"`; alta de la misma variante | `Err(AlreadyQueued)`; `list()` sigue con 1 unidad; 0 `save` nuevos |
  | `enqueue_rechaza_destino_fallido_con_otra_grafia` | unidad `Failed(Network)` con `"Q4/m.gguf"`; alta de `"q4/M.gguf"` del mismo repo | `Err(AlreadyQueued)` |
  | `enqueue_tras_cancelled_permitido` | mismo destino con la 1.ª `Cancelled` | `Ok` |
  | `enqueue_final_otro_tamano` | final en disco con 10 bytes, `size` 20 | `Err(DestinationExists)`, fichero intacto |
  | `enqueue_final_mismo_tamano_candidato` | final en disco con el tamaño esperado | `Ok`; la parte queda candidata a `Preexisting` |
  | `enqueue_destino_por_repo` | repo `"org/model"`, path `"Q8_0/m.gguf"` | `dest == models_dir.join("org").join("model").join("Q8_0").join("m.gguf")` |
  | `snapshot_version_crece` | alta y después otro cambio de estado | `version` estrictamente creciente |
  | `resolve_url_codifica` (en `hf/url.rs`) | repo `a/b`, path `"sub dir/m#1?.gguf"` | ruta `/a/b/resolve/<rev>/sub%20dir/m%231%3F.gguf` |
  | `resolve_url_conserva_puerto` (en `hf/url.rs`) | base `http://127.0.0.1:8080/` | empieza por `http://127.0.0.1:8080/a/b/resolve/` |
  | `enqueue_url_usa_revision` | revisión `9217f5db79a29953eb74d5343926648285ec7e67` | `url` contiene `/resolve/9217f5db79a29953eb74d5343926648285ec7e67/` |

### T12 — Gestor: planificador, ejecución de unidades y eventos
- **Ficheros/módulos:** `crates/core/src/download/manager.rs`, `runner.rs`; `retry.rs` y `progress.rs` (T12.4); `verify.rs` (T12.6)
- **Depende de:** T6, T7, T9 (T9.4), T11
- **Cubre:** CA1, CA6, CA7, CA8, CA11
- **Subtareas:**
  - [x] T12.1 Planificador: tras cada cambio, si hay hueco (`max_active`), toma la siguiente `Queued` (FIFO por id) y lanza su ejecución en una tarea de tokio con su `CancellationToken`.
  - [x] T12.2 Ejecución, **en este orden** (Diseño §7 y §9, #73): (0) devuelve a `Pending` las partes `Verified` sin `.part` ni destino de `expected_size` y persiste el cambio (lo implementa T13.4, que añade el paso en este punto; en T12 el hueco queda vacío), de modo que esas partes ya cuentan en los pasos (1) y (2); (1) clasifica las candidatas a `Preexisting` con `is_preexisting_candidate` (solo `metadata`, sin hash); (2) con la unidad en `Queued`, calcula `required_bytes` sobre las partes `Pending` **no candidatas** y, solo si el resultado es mayor que 0, llama a `check_space` (T8, sin cambios en `disk.rs`); (3) descarga las partes `Pending` no candidatas en orden (T6) pasando a `transfer_with_retry` el closure `auth_source(&spec.url, hf_origin, tokens)` de T9.4 (el token se lee en cada intento, nunca se calcula la cabecera una vez por fichero), y verifica cada una (T7); (4) ya en `Verifying`, verifica el tamaño y el SHA de las candidatas sobre su final, sin tocarlo, y las marca `Preexisting` (si el tamaño o el hash no coinciden, `Failed(DestinationExists)`; si el final ha desaparecido, `Failed(Io { message })` con mensaje fijo; Diseño §9); (5) finaliza la unidad (T7) y emite el progreso con `ProgressReporter` (T3). Persiste y emite `downloads://state` en cada cambio de estado. `Failed(SizeMismatch)` y `Failed(ChecksumMismatch)` de una parte **descargada** borran su `.part`; en una candidata nunca se borra el final.
  - [x] T12.3 La comprobación de espacio se hace con la unidad en `Queued` (Diseño §7). Para ello `UnitState::can_transition` (`crates/core/src/download/model.rs`) admite `Queued → Failed(_)` (una línea) y se añade el test `transicion_queued_a_failed` junto a los de T1.
  - [x] T12.4 **Aviso de reinicio y rebobinado del progreso.** `transfer_with_retry` (`retry.rs`) gana el callback `on_restart: &mut (dyn FnMut() + Send)`. Para no llegar a 8 parámetros (clippy `too_many_arguments`, y `#[allow]` está prohibido), `on_bytes` y `on_restart` se agrupan en `pub struct TransferObserver<'a> { pub on_bytes: &'a mut (dyn FnMut(u64) + Send), pub on_restart: &'a mut (dyn FnMut() + Send) }`, que se pasa **por valor** como último parámetro. Firma resultante: `transfer_with_retry(fetcher, spec, auth: &(dyn Fn() -> Option<HeaderValue> + Send + Sync), policy, sleeper, cancel, observer: TransferObserver<'_>)`. La semántica de cada callback no cambia. `on_restart` se llama **una vez por cada `Restart`** (el de `transfer_once` por `200` con offset > 0 o `416` incompleto, y el reinicio único por `UnexpectedContentRange` de `resolve_attempt`), justo después de truncar el `.part` y antes del backoff y del siguiente intento. No se llama en `Interrupted`, `Retry` ni en los fallos. `on_bytes` sigue recibiendo solo deltas. `progress.rs` añade `ProgressTracker::rewind(now, downloaded)`, que fija los bytes descargados al valor **absoluto** `downloaded` (≤ `total`), conserva `total` y vacía la ventana de velocidad (lo re-descargado no cuenta como velocidad nueva hasta que llegan muestras), y `ProgressReporter::rewind(downloaded) -> Progress`, que lo aplica y devuelve el evento **forzado** (sin pasar por el `Throttle`), para que la UI baje la barra al momento. Los tests existentes de `retry.rs` pasan `TransferObserver { on_bytes: &mut |_| {}, on_restart: &mut || {} }` (o con el `on_bytes` que ya usaban) sin cambiar sus asertos; T18 (`download_one`) construye su propio `TransferObserver` y en `on_restart` rebobina su progreso.
  - [x] T12.5 El runner conecta los dos: en el `on_restart` del `TransferObserver` llama a `reporter.rewind(base)` y emite el `Progress` devuelto por el `EventSink`. `base` es la suma de lo realmente hecho en las **demás** partes de la unidad (la parte actual tiene el `.part` a 0 tras el reinicio), con cada término limitado a su `expected_size` (`min(término, expected_size)`):
    - partes `Verified` y `Preexisting`: su `expected_size`;
    - candidatas a `Preexisting` (Diseño §9): su `expected_size`;
    - demás partes `Pending`: la longitud de su `.part` al empezar la ejecución.

    Así la base es coherente con el `downloaded` inicial del reporter, el último evento llega a `total` y `downloaded <= total` se cumple siempre (desviación aceptada en revisión respecto a la versión inicial, que solo contaba `Verified` y `Preexisting`).
  - [x] T12.6 **Defensa contra `Pending` en `finalize_unit` (`verify.rs`).** Si alguna parte de la lista está `Pending`, `finalize_unit` devuelve `Err(DownloadError::InvalidRequest("parte sin verificar"))` **antes de tocar nada**: ni comprueba destinos ni renombra ninguna parte, así que nunca deja a la vista un split incompleto (Diseño §2). Sustituye al comportamiento de T7 de ignorar las `Pending`; las `Preexisting` se siguen ignorando (lo cubre ya `finalize_ignora_los_preexistentes`). Si algún test de T7 pasaba partes `Pending`, se ajusta. **Reflejo en `UnitState`:** `InvalidRequest` no es un `FailureKind`, así que el runner traduce los errores con una única función pura, `unit_failure(err: DownloadError) -> FailureKind` (`runner.rs`):
    - `Failed(kind)` → `kind`.
    - `InvalidRequest(UNVERIFIED_PART)` → `Io { message: "parte sin verificar al finalizar la unidad" }`, con mensaje fijo. `UNVERIFIED_PART = "parte sin verificar"` es una constante `pub(super)` de `verify.rs` que usan `finalize_unit` y `unit_failure`, para no comparar literales sueltos. No hace falta ninguna variante nueva. Los `.part` se conservan, y «Reintentar» vuelve a ejecutar la unidad y descarga las `Pending`.
    - Cualquier otro `InvalidRequest(msg)` → `Io { message: format!("petición no válida: {msg}") }`. Hoy solo puede ser `"sha256 no válido"` de `verify_file`, con un `downloads.json` editado a mano (T14.3 ya valida el hash en el alta). `msg` es un `&'static str` del propio código, sin rutas ni datos sensibles.
    - `DestinationExists` → `FailureKind::DestinationExists`.
    - `Io(_)` → `Io { message: "error de lectura o escritura en el disco" }`, un mensaje fijo (la función solo recibe el error, no sabe qué operación falló). Nunca se usa el `Display` del `io::Error`, que puede llevar rutas locales (igual que `check_space`). Cubre también el final de una candidata que desaparece durante la descarga (Diseño §9).
    - `UnsafePath`, `IncompleteSplit`, `AlreadyQueued`, `InvalidTransition`, `UnknownDownload` y `Cancelled` → `Io { message: "error interno inesperado de la descarga" }`, el mismo mensaje fijo para las seis. Son inalcanzables en el runner: las cinco primeras solo las producen `enqueue` (T11) y las órdenes (T13), nunca `transfer_with_retry`, `verify_file`, `finalize_unit`, `check_space` ni `store.save`, que son lo que llama el runner. `Cancelled` lo gestionan `pause` y `cancel` (T13) antes de llegar aquí. Se traducen igualmente, sin `panic!` ni `unreachable!`, para que una regresión acabe en una unidad `Failed` visible y no tumbe la tarea.

    El `match` es exhaustivo y **sin brazo `_`**, para que una variante nueva de `DownloadError` obligue a decidir su traducción.
- **Notas para el runner (T12.2), sin subtareas:**
  - **Progreso en la fase Verify.** El `on_bytes` de `verify_file`/`sha256_file` es `F: FnMut(u64) + Send + 'static` porque se mueve a `spawn_blocking`, así que no puede tomar prestado el `ProgressReporter`: hay que compartirlo (por ejemplo `Arc<Mutex<ProgressReporter>>`, junto con el `EventSink` en `Arc`) o enviar los deltas por un canal que consuma la tarea del runner. En cambio, el `on_bytes` de `transfer_with_retry` va en el `TransferObserver` (T12.4, `&'a mut (dyn FnMut(u64) + Send)`, junto a `on_restart`) y puede usar el reporter por referencia. **Implementado** con `ProgressEmitter` (`runner.rs`): un único `ProgressReporter` y un único limitador por unidad tras un `Arc<Mutex<_>>`, compartidos por las dos fases. El contrato de los eventos es el del Diseño §6: `Download` cuenta los bytes recibidos más los iniciales y solo baja en un `rewind`; `Verify` parte de los bytes que no requieren hash y suma los hasheados, y es monótono; como cada parte se hashea justo después de descargarse (unidad aún en `Downloading`), las dos fases se intercalan y el valor puede bajar entre fases, así que la UI trata cada `phase` como una barra independiente; el último evento es `Verify` con `downloaded == total`; los forzados son `rewind`, `finish_download` y `finish_verify`; en `Verify`, `bytes_per_sec` y `eta_secs` son del hash.
  - **Cancelación siempre con el token.** El runner (y `pause`/`cancel` de T13) cancelan **siempre** con el `CancellationToken` y esperan a que la tarea termine; nunca con `JoinHandle::abort()` ni soltando el futuro. Si se suelta el futuro de `sha256_file` sin cancelar el token, el hilo de `spawn_blocking` sigue hasheando un fichero que puede pesar decenas de GB, y además retiene el `.part` abierto (en Windows impide borrarlo al cancelar).
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `unidad_simple_completa` | 1 fichero de 1 MiB con sha correcto | `Completed`; estados emitidos `Queued → Downloading → Verifying → Completed`; final en disco sin `.part` |
  | `progreso_limitado_en_frecuencia` | 10 MiB, `SteppingClock` de 50 ms | eventos de progreso ≤ `ceil(t_final / 250 ms) + 2`; el último con `downloaded == total` |
  | `dividido_agrega_progreso` | 3 partes de 1 MiB | último progreso `total_bytes == 3145728`; ningún final en disco al emitirse `Verifying` de la tercera |
  | `fifo_y_un_activo` | 3 unidades, `max_active` 1 | `Downloading` en orden 1, 2, 3; nunca 2 activas a la vez en la secuencia del `RecordingSink` |
  | `sin_espacio_no_pide_nada` | 60 GiB declarados, disco falso con 40 GiB | estados emitidos exactamente `Queued → Failed(InsufficientSpace { needed: 65498251264, available: 42949672960 })` (nunca `Downloading`); wiremock con 0 peticiones |
  | `transicion_queued_a_failed` (en `model.rs`) | `Queued → Failed(InsufficientSpace { needed: 2, available: 1 })` | `true` |
  | `checksum_malo_borra_part` | sha incorrecto | `Failed(ChecksumMismatch)`, ni `.part` ni final |
  | `tamano_malo_borra_part` | servidor envía 1 byte de más | `Failed(SizeMismatch { .. })`, sin `.part` |
  | `preexistente_no_se_descarga` | final ya en disco con tamaño y hash correctos | `Completed`, 0 peticiones, `status == Preexisting` |
  | `preexistente_hash_distinto_falla` | final con el tamaño correcto y otro contenido | `Failed(DestinationExists)`, fichero intacto, 0 peticiones |
  | `reencolar_completado_sin_espacio_completa_sin_peticiones` | final de 1 MiB con sha correcto ya en disco, como el que deja «Quitar» sobre una `Completed`; alta de esa variante con `FakeDiskSpace(Some(0))` | la unidad acaba `Completed`, nunca `Failed(InsufficientSpace { .. })`; `status == Preexisting`; wiremock con 0 peticiones; el final está intacto. (El recorrido completo con `remove` está en T13: `quitar_completada_y_reencolar_sin_espacio`) |
  | `candidata_no_cuenta_para_el_espacio` | unidad dividida de 2 partes de 1 MiB: la 1.ª candidata (final de 1 MiB con sha correcto en disco), la 2.ª `Pending` sin `.part`; `FakeDiskSpace(Some(1048576 + 1073741824))` (exactamente 1 MiB + margen) | pasa la comprobación de espacio (sin la corrección pediría 2 MiB + margen y fallaría); `Completed`; 1 sola petición, la de la 2.ª parte |
  | `candidata_sin_espacio_para_el_resto_falla` | igual que el anterior, pero `FakeDiskSpace(Some(1048576 + 1073741823))` | estados `Queued → Failed(InsufficientSpace { needed: 1074790400, available: 1074790399 })`; 0 peticiones; el final de la 1.ª intacto |
  | `unit_failure_parte_sin_verificar_es_io` (en `runner.rs`) | `DownloadError::InvalidRequest("parte sin verificar")` | `FailureKind::Io { message: "parte sin verificar al finalizar la unidad" }` |
  | `unit_failure_conserva_failed_y_destino` (en `runner.rs`) | `Failed(ChecksumMismatch)` y `DestinationExists` | `ChecksumMismatch` y `FailureKind::DestinationExists` |
  | `unit_failure_io_sin_ruta` (en `runner.rs`) | `DownloadError::Io(io::Error::other("fallo en /secreto/ruta"))` | `FailureKind::Io { message }` con `message` que no contiene `secreto` |
  | `unit_failure_otra_peticion_invalida` (en `runner.rs`) | `DownloadError::InvalidRequest("sha256 no válido")` | `FailureKind::Io { message: "petición no válida: sha256 no válido" }` |
  | `unit_failure_variantes_inalcanzables` (en `runner.rs`) | `UnsafePath`, `IncompleteSplit`, `AlreadyQueued`, `InvalidTransition`, `UnknownDownload` y `Cancelled` | las seis dan `FailureKind::Io { message: "error interno inesperado de la descarga" }`, sin pánico |
  | `persiste_en_cambios_de_estado` | unidad completa | `RecordingStore`: nº de `save` == nº de eventos de estado; el último refleja `Completed` |
  | `fallo_red_conserva_part` | 5 cortes tras 1 MiB sin progreso posterior | `Failed(Network)`, `.part` de 1 MiB en disco |
  | `restart_avisa_on_restart` (en `retry.rs`) | `.part` con los primeros 100 KiB de un fichero de 256 KiB; `cutting_server` con `ignore_range: true` y sin cortes | `Completed`; `on_restart` llamado exactamente 1 vez; en la secuencia registrada (`Restart` / `Bytes(n)`), el aviso va antes de todos los bytes del 2.º intento y la suma de deltas tras el aviso es 262144 |
  | `restart_por_content_range_avisa` (en `retry.rs`) | 1.ª respuesta 206 incoherente con `.part` de 100 KiB, 2.ª 200 correcta | `Completed`; `on_restart` llamado exactamente 1 vez |
  | `interrupcion_no_avisa_restart` (en `retry.rs`) | 3 MiB, corte cada 1 MiB (como `cortes_con_progreso_terminan`) | `Completed`; `on_restart` llamado 0 veces |
  | `tracker_rewind_fija_absoluto` (en `progress.rs`) | `ProgressTracker::new(262144, 102400)`, `record` de 50 KiB, `rewind(now, 0)` | `snapshot.downloaded == 0`, `snapshot.total == 262144` |
  | `tracker_rewind_vacia_ventana` (en `progress.rs`) | 1 MiB/s durante 2 s, `rewind(now, 0)` sin más muestras | `bytes_per_sec == 0`, `eta_secs == None` |
  | `reporter_rewind_no_se_limita` (en `progress.rs`) | `record` que emite evento y, sin avanzar el `FakeClock`, `rewind(0)` | devuelve un `Progress` con `downloaded == 0` aunque no hayan pasado 250 ms |
  | `reinicio_no_supera_total` | unidad de 1 fichero de 256 KiB (262144 bytes) con un `.part` previo de 100 KiB; `cutting_server` con `ignore_range: true` | `Completed`; **todos** los `DownloadProgress` emitidos cumplen `downloaded_bytes <= total_bytes == 262144`; hay un evento con `downloaded_bytes == 0` tras el reinicio; el último tiene `downloaded_bytes == 262144` |
  | `reinicio_en_segunda_parte_rebobina_a_base` | unidad dividida de 2 partes de 128 KiB; la 1.ª se descarga bien; la 2.ª tiene un `.part` previo de 50 KiB y el servidor ignora `Range` | todos los `downloaded_bytes <= 262144`; el evento de rebobinado tiene `downloaded_bytes == 131072`; `Completed` |
  | `finalize_rechaza_parte_pendiente` (en `verify.rs`) | 3 partes: la 1.ª y la 3.ª `Verified` con su `.part`, la 2.ª `Pending` con su `.part` | `Err(InvalidRequest("parte sin verificar"))`; los 3 `.part` siguen en disco con su contenido y no existe ningún final |

### T13 — Órdenes: pausar, reanudar, cancelar, reintentar, quitar y restaurar
- **Ficheros/módulos:** `crates/core/src/download/manager.rs`, `runner.rs`; `verify.rs` (T13.4)
- **Depende de:** T12
- **Cubre:** CA5, CA10, CA11
- **Subtareas:**
  - [x] T13.1 `pause(id)`, `resume(id)`, `cancel(id)`, `retry(id)` y `remove(id)`, validados con `can_transition` (`InvalidTransition` si no vale, `UnknownDownload` si no existe). Pausar o cancelar la activa espera a que su tarea termine antes de devolver.
  - [x] T13.2 Cancelar borra los `.part` de la unidad y los finales que ha creado, que son los de sus partes `Verified` ya renombradas según la regla de T13.4 (sin `.part` y destino de `expected_size`). Nunca borra los `Preexisting` ni un destino que no cumpla la regla. Quitar una `Failed` borra lo mismo (T13.5). **Defensa en profundidad (#72, Diseño §8):** antes de borrar, `cancel` y `remove` llaman a un helper `protected_keys(units, except: DownloadId) -> ProtectedKeys { parts: HashSet<String>, finals: HashSet<String> }`, que recorre el resto de unidades de la cola:
    - **`parts`:** el `dest_key(spec.dest)` (T11.5) de cada `FileSpec` de las unidades en estado no terminal (`Queued`, `Downloading`, `Verifying`, `Paused`) o `Failed(_)`, esté como esté la parte. Es el mismo predicado de estado que usa `AlreadyQueued` (T11.5).
    - **`finals`:** el `dest_key(spec.dest)` de cada `FileSpec` `Preexisting`, o `Verified` y ya renombrado (regla de T13.4, que mira el disco), de las unidades en cualquier estado salvo `Cancelled`.
    - **Las `Cancelled` no aportan nada** y las `Completed` solo aportan a `finals`.

    El único helper de borrado, que usan T13.2, T13.5 y T13.6 (este último solo para los `.part`, sin finales), no borra un `.part` cuya clave esté en `parts` ni un final cuya clave esté en `finals`, y no devuelve error por ello. La clave se calcula siempre sobre el destino final del `FileSpec`, nunca sobre la ruta del `.part`. Tanto `protected_keys` como la lista de partes propias ya renombradas se calculan **antes de borrar nada**, porque borrar un `.part` cambia el resultado de la regla de «ya renombrada». La regla vive en un único helper `is_already_renamed(&FileSpec) -> bool` (`verify.rs`, `pub(super)`) que usan `finalize_unit` (T13.4), `protected_keys` y el borrado. Coste aceptado: un `.part` que se salte así queda en disco y lo reaprovecha la unidad que lo protege (continúa por `Range`).
  - [x] T13.3 Restauración: `DownloadManager::new` carga la cola normalizada (T10) y no arranca nada hasta un `resume`.
  - [x] T13.4 **Recuperación tras una caída a mitad de `finalize_unit` (`verify.rs` y `runner.rs`).** Escenario: la parte 1 ya está renombrada y la 2 sigue en `.part` cuando el proceso muere; al arrancar, T10 deja la unidad en `Paused` con las dos partes `Verified`. Hoy, al reanudar, `finalize_unit` da `DestinationExists` y la unidad acaba en `Failed`.
    - `finalize_unit` clasifica cada parte `Verified` antes de tocar nada: si **no existe su `.part`** y el destino existe con longitud **igual a `expected_size`**, cuenta como **ya renombrada** y se salta (ni comprobación de `DestinationExists` ni `rename`). El resto sigue igual que en T7: `.part` presente y destino existente → `DestinationExists` sin tocar nada; sin `.part` ni destino de ese tamaño → se intenta el `rename`, que falla con `Io` (mantiene `finalize_revierte_si_falla_un_renombrado`). Con esto `finalize_unit` es idempotente: llamarla dos veces sobre la misma unidad da `Ok` las dos.
    - Si un `rename` falla, la reversión best-effort devuelve a su `.part` también las partes contadas como ya renombradas, no solo las de esta llamada, para que no quede a la vista un split incompleto (Diseño §2).
    - El runner, al ejecutar una unidad (tras `resume` o `retry`), pasa a `Pending` cualquier parte `Verified` que no tenga ni `.part` ni un destino de `expected_size` (ficheros borrados por el usuario mientras estaba en pausa), para descargarla de nuevo en lugar de fallar con `Io` en cada reintento. Persiste el cambio. Es el paso (0) de T12.2: se hace **antes** de clasificar las candidatas y de comprobar el espacio, así esas partes cuentan en `required_bytes`.
  - [x] T13.5 **Quitar una `Failed` con partes ya renombradas.** Además de sus `.part`, `remove` en `Failed` borra el final de cada parte `Verified` que cumpla la regla de «ya renombrada» de T13.4 (sin `.part` y destino de `expected_size`). Nunca borra los `Preexisting`, ni un destino que no cumpla la regla, ni nada de una unidad `Completed` (Diseño §8); el caso `Cancelled` está en T13.6. Tampoco borra un `.part` ni un final protegidos por otra unidad según `protected_keys` (T13.2, Diseño §8, #72): los `.part` solo frente a unidades no terminales o `Failed`, y los finales solo frente a unidades no `Cancelled` cuya parte de ese destino esté `Preexisting` o `Verified` ya renombrada. Así «Quitar» no deja un `.gguf` suelto de un split que no terminó, y quitar una unidad no deja a otra en `Completed` sin modelo.
  - [x] T13.6 **Camino de limpieza tras un fallo de borrado en `cancel` (`manager/orders.rs`, `manager/cleanup.rs`).** Si `delete_unit_files` falla en `cancel`, la unidad pasa igualmente a `Cancelled` y `cancel` devuelve el error (comportamiento actual, que se mantiene). Para que el `.part` que quede no se pierda sin camino de limpieza (`cancel` ya no vale desde `Cancelled`), `remove` sobre una `Cancelled` vuelve a intentar borrar **solo los `.part` de la unidad** (`part_path(spec.dest)` de cada `FileSpec`), saltándose los que estén en `protected_keys(..).parts` calculado antes de borrar (T13.2), antes de sacarla de la lista. **Nunca borra un final**, cumpla o no la regla de «ya renombrada» (#87): `cancel` no reinicia el estado de los `FileSpec`, así que una parte `Verified` que aún era `.part` sigue `Verified` en la `Cancelled` después de borrarse ese `.part`; si luego se encola la misma variante como B (permitido, `AlreadyQueued` libera los destinos de `Cancelled`), B termina `Completed` y se quita, `protected_keys` ya no incluye a B y la regla de T13.4 tomaría el `.gguf` de B como un final propio de la `Cancelled`. Implementación: `delete_unit_files` (o el helper que calcula las rutas) recibe un modo que excluye los finales, y `remove` lo usa en `Cancelled`; `cancel` y `remove` en `Failed` (T13.5) siguen borrando `.part` y finales ya renombrados. Si ese borrado falla, `remove` devuelve el error y la unidad sigue en `list()` en `Cancelled`, igual que en `Failed`. Si no queda nada que borrar, no hay error. **Coste aceptado:** si el borrado de `cancel` falla justo en un final ya renombrado, ese final se queda en disco y «Quitar» no lo limpia; el usuario puede borrarlo a mano o reencolar la variante, que lo tomará como candidata a `Preexisting` (Diseño §7). No cambia T13.1–T13.5.
  - **Fixture aceptado para pausa y cancelación de la activa.** Donde la tabla dice «desde `on_bytes` del `RecordingSink`», se acepta la implementación con `stalling_server` (la conexión envía N bytes y se queda abierta sin enviar más) más `Harness::wait_for_part` (espera a que el `.part` tenga N bytes) antes de dar la orden, porque es determinista y no depende de en qué bloque llega el evento. Que la orden **corta de verdad la transferencia** lo fija `pausar_activa_arranca_siguiente`: la conexión de la 1.ª unidad está parada para siempre y el cliente de los tests no tiene timeout, así que `pause` solo puede devolver, y la 2.ª solo puede arrancar con `max_active = 1`, si el token cancela la lectura en curso y la tarea termina. Ese test debe conservar esas dos condiciones (conexión parada sin fin y `max_active = 1`); si alguna cambia, hay que añadir otro que lo compruebe.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `pausa_conserva_part` | pausa tras 2 bloques (desde `on_bytes` del `RecordingSink`) | `Paused`; tamaño del `.part` igual antes y después de devolver `pause` |
  | `reanuda_desde_part` | `resume` tras pausar | la siguiente petición lleva `Range: bytes=<len>-`; `Completed` |
  | `pausar_activa_arranca_siguiente` | 2 unidades, pausa la 1 | la 2 pasa a `Downloading` |
  | `pausar_en_cola` | `Queued` → `pause` | `Paused` sin peticiones |
  | `cancelar_borra_todo_lo_suyo` | dividido con la parte 1 verificada y la 2 en curso | `Cancelled`; ningún `.part` ni final de la unidad |
  | `cancelar_respeta_preexistentes` | parte 1 `Preexisting` | la parte 1 sigue en disco con su contenido |
  | `cancelar_pausada` | `Paused` con `.part` | `Cancelled`, sin `.part` |
  | `reintentar_tras_fallo_red` | `Failed(Network)` con `.part` de 1 MiB | `retry` → `Queued` → petición con `Range: bytes=1048576-` |
  | `quitar_solo_terminales` | `remove` en `Downloading` | `Err(InvalidTransition)` |
  | `quitar_fallida_borra_part` | `remove` en `Failed(Network)` con `.part` | sale de `list()`, sin `.part` |
  | `quitar_completada_conserva_final` | `remove` en `Completed` | sale de `list()`, el final sigue en disco |
  | `restaurar_y_reanudar` | `downloads.json` con `Downloading` y `.part` de 4 MiB | tras `new`: `Paused`, 0 peticiones; `resume` → `Range: bytes=4194304-` |
  | `orden_sobre_id_inexistente` | `pause(DownloadId(999))` | `Err(UnknownDownload)` |
  | `finalize_reanuda_tras_caida_a_mitad` (en `verify.rs`) | 2 partes `Verified` de 16 bytes: la 1.ª sin `.part` y con el final de 16 bytes en disco; la 2.ª con su `.part` y sin final | `Ok(())`; los 2 finales en disco con su contenido, 0 `.part`; una 2.ª llamada a `finalize_unit` también da `Ok(())` |
  | `finalize_final_de_otro_tamano_no_cuenta_como_renombrada` (en `verify.rs`) | parte `Verified` sin `.part` y con un destino de 10 bytes (`expected_size` 16) | `Err(..)` (no `Ok`); el destino sigue con sus 10 bytes |
  | `finalize_revierte_tambien_las_ya_renombradas` (en `verify.rs`) | 3 partes `Verified`: la 1.ª ya renombrada (sin `.part`, final de `expected_size`), la 2.ª con `.part`, la 3.ª sin `.part` ni final (su `rename` falla) | `Err(Io(_))`; ningún final en disco; la 1.ª y la 2.ª vuelven a estar en su `.part` con su contenido |
  | `reanudar_tras_caida_en_finalize_completa` | `downloads.json` con la unidad dividida en `Downloading`, las 2 partes `Verified`; en disco el final de la 1.ª y el `.part` de la 2.ª | tras `new`: `Paused`; `resume` → `Completed`, 0 peticiones, 2 finales y 0 `.part` (nunca `Failed(DestinationExists)`) |
  | `verificada_sin_fichero_vuelve_a_pending` | unidad `Paused` con 1 parte `Verified` sin `.part` ni final | `resume` → la parte pasa a `Pending`, se pide con `Range` ausente (offset 0) y la unidad acaba `Completed` |
  | `verificada_sin_fichero_espacio_justo_completa` | `downloads.json` con una unidad `Paused` dividida en 2 partes de 1 MiB: la 1.ª `Verified` sin `.part` ni final, la 2.ª `Pending` sin `.part`; `FakeDiskSpace(Some(2097152 + 1073741824))` | `resume` → pasa la comprobación de espacio (el paso (0) de T12.2 cuenta la 1.ª); `Completed`; 2 peticiones, ninguna con `Range` |
  | `verificada_sin_fichero_cuenta_para_el_espacio` | igual, con `FakeDiskSpace(Some(2097152 + 1073741823))` | `resume` → `Failed(InsufficientSpace { needed: 1075838976, available: 1075838975 })`; 0 peticiones; la 1.ª parte persistida como `Pending` |
  | `candidata_desaparecida_falla_io_y_reintento_descarga` | unidad dividida en 2 partes de 1 MiB: la 1.ª candidata (final con sha correcto en disco), la 2.ª `Pending`; un `EventSink` de prueba que borra el final de la 1.ª al recibir el primer `DownloadProgress` con `phase: Download`; `FakeDiskSpace(Some(2097152 + 1073741824))` | la unidad acaba `Failed(Io { message })` (no `DestinationExists`) y `message` no contiene la ruta del `TempDir`; el `.part` de la 2.ª sigue en disco; `retry` → 1 petición nueva, sin `Range`, solo para la 1.ª; `Completed` con los 2 finales y 0 `.part` |
  | `quitar_fallida_borra_partes_ya_renombradas` | unidad dividida `Failed(Network)`: parte 1 ya renombrada (sin `.part`, final de `expected_size`), parte 2 con `.part` | `remove` → sale de `list()`; ni el final de la 1.ª ni el `.part` de la 2.ª en disco |
  | `quitar_no_borra_ficheros_de_otra_unidad` | `downloads.json` (estado que T11.5 ya no permite crear con `enqueue`, construido a mano): unidad A dividida `Failed(Network)` con la parte 1 `Verified` ya renombrada; unidad B de la misma variante `Completed`, con la parte 1 `Preexisting` y la 2 `Verified`; en disco, los 2 finales | `remove(A)` → A sale de `list()`; los 2 finales siguen en disco con su contenido; B sigue `Completed` |
  | `quitar_no_borra_part_de_otra_unidad` | igual, pero B en `Downloading` (se restaura como `Paused`) con la parte 1 `Preexisting` y el `.part` de 64 KiB de la parte 2; A `Failed(Network)` con la parte 1 `Verified` ya renombrada | `remove(A)` → el final de la parte 1 y el `.part` de la parte 2 siguen en disco, intactos; `resume(B)` pide `Range: bytes=65536-` y B acaba `Completed` |
  | `cancelar_no_borra_ficheros_de_otra_unidad` | `downloads.json` construido a mano: A `Paused` dividida en 2 partes de 16 bytes, con la parte 1 `Verified` ya renombrada (final de 16 bytes en disco, sin `.part`) y un `.part` de 8 bytes en la parte 2; B `Completed` de un solo fichero de 16 bytes, `Verified`, cuyo destino es el de la parte 1 de A | `cancel(A)` → `Cancelled`; el final de la parte 1 sigue en disco con su contenido (lo protege B: `Verified` ya renombrada); el `.part` de la parte 2 de A se borra; B sigue `Completed` |
  | `cancelar_no_borra_part_de_unidad_viva` | `downloads.json` construido a mano: A `Paused` de 1 fichero con un `.part` de 64 KiB; B `Paused` con el mismo destino y `Pending` | `cancel(A)` → `Cancelled`; el `.part` sigue en disco, intacto (lo protege B, no terminal); `resume(B)` pide `Range: bytes=65536-` |
  | `cancelar_tras_otra_cancelada_borra_su_part` | flujo real: alta de A (1 fichero de 256 KiB), `cancel(A)` → `Cancelled` (sigue en `list()`); alta de B, la misma variante (`Ok`); `cancel(B)` desde `on_bytes` del `RecordingSink` tras 2 bloques | B `Cancelled`; no existe ni el `.part` ni el final; A sigue `Cancelled` en `list()` |
  | `cancelar_tras_otra_completada_sin_final_borra_su_part` | `downloads.json` con A `Completed` de 1 fichero de 256 KiB, `Verified`, con el final borrado (ni final ni `.part` en disco); alta de B, la misma variante (`Ok`, no es candidata); `cancel(B)` desde `on_bytes` tras 2 bloques | B `Cancelled`; no existe ni el `.part` ni el final; A sigue `Completed` |
  | `protected_keys_segun_estado` | `TempDir` y unidades a mano, con `except` = la unidad 1 (destino `i`): 2 `Queued` (`a`, `Pending`); 3 `Paused` (`b`, `Pending`); 4 `Failed(Network)` (`c` `Pending` y `j` `Verified` ya renombrada); 5 `Completed` (`d` `Verified` con final de `expected_size` y sin `.part`); 6 `Completed` (`e` `Verified` sin final); 7 `Cancelled` (`f` `Verified` con final en disco y `g` `Preexisting` con final); 8 `Completed` (`h` `Preexisting` con final) | `parts == {a, b, c, j}` y `finals == {d, h, j}` (como `dest_key`); ni `e`, ni `f`, ni `g`, ni `i` aparecen en ninguno |
  | `pendiente_sin_final_cuenta_para_el_espacio` | `downloads.json` con una unidad `Paused` de 1 fichero de 1 MiB `Pending`, sin final ni `.part` (una candidata cuyo final se borró en pausa: el estado candidata no se persiste); `FakeDiskSpace(Some(0))` | `resume` → `Failed(InsufficientSpace { needed: 1074790400, available: 0 })`; 0 peticiones |
  | `quitar_completada_y_reencolar_sin_espacio` | unidad `Completed` de 1 fichero de 1 MiB con sha correcto; `remove`; alta de la misma variante con `FakeDiskSpace(Some(0))` | la nueva unidad acaba `Completed` con `status == Preexisting`, 0 peticiones nuevas, final intacto (#73) |
  | `quitar_cancelada_reintenta_borrar_part` (T13.6) | `downloads.json` con A `Cancelled` de 1 fichero de 256 KiB, `Pending`, y en disco un `.part` de 64 KiB que dejó un borrado fallido de `cancel` (estado construido a mano, sin provocar el fallo) | `remove(A)` → `Ok`; A sale de `list()`; el `.part` ya no existe |
  | `quitar_cancelada_respeta_part_de_unidad_viva` (T13.6) | igual, más B `Paused` con el mismo destino y `Pending` (el `.part` es de B a efectos de `protected_keys`) | `remove(A)` → `Ok`; A sale de `list()`; el `.part` sigue en disco, intacto; `resume(B)` pide `Range: bytes=65536-` |
  | `quitar_cancelada_no_borra_final_de_descarga_posterior` (T13.6, #87) | `downloads.json` construido a mano con solo A `Cancelled` de 1 fichero `m.gguf`, `Verified`, sin `.part`; en disco un `m.gguf` de `expected_size` con contenido conocido que no es de ninguna unidad de la cola (el que dejó una B de la misma variante, `Completed` y ya quitada) | `remove(A)` → `Ok`; A sale de `list()`; `m.gguf` sigue en disco con el mismo contenido byte a byte |
  | `quitar_cancelada_conserva_unidad_si_falla_borrado` (T13.6) | `downloads.json` con A `Cancelled` de 1 fichero, `Pending`; en la ruta de su `.part` hay un **directorio no vacío** en lugar de un fichero, para que `remove_file` falle con un error distinto de `NotFound` en Windows y macOS (misma técnica que `error_de_lectura_no_es_cola_vacia` en `store.rs`: un directorio donde se espera un fichero; no hay ningún test previo de fallo de borrado en `cancel` en `orders.rs` ni en `cleanup.rs`) | `remove(A)` → `Err(DownloadError::Io(_))`; A sigue en `list()` en `Cancelled`; el directorio sigue en disco |
  | `quitar_fallida_respeta_preexistentes_y_ajenos` | `Failed(DestinationExists)` con la parte 1 `Preexisting` y la parte 2 candidata a `Preexisting` cuyo hash no coincidió (su `status` nunca llega a `Verified`), con el final de `expected_size` y sin `.part` | `remove` → los dos finales siguen en disco con su contenido |

### T14 — Revisión fijada y unidad completa desde la ficha (#55)
- **Ficheros/módulos:** `crates/core/src/hf/url.rs`, `model.rs`, `client.rs`, `service.rs`, `files.rs`; `crates/core/src/download/request.rs` (conversión); `src-tauri/src/commands/hf.rs` (test `json_model_detail_para_la_ui`); `ui/src/lib/hf/types.ts` (espejo)
- **Depende de:** T2, T11
- **Cubre:** CA6, CA12
- **Subtareas:**
  - [x] T14.1 `MODEL_INFO_EXPANDS` añade `"sha"`; `ModelInfo` gana `sha: Option<String>`. `DEFAULT_REVISION` pasa a `pub` y `is_valid_revision(&str) -> bool` (`"main"` o `^[0-9a-f]{40}$`).
  - [x] T14.2 `tree_url(base, repo, revision)` y `HfClient::tree(repo, revision)`. `HfService::model_detail` usa `revision = info.sha` si es válida o `DEFAULT_REVISION` si no, pide el árbol en esa revisión y devuelve `ModelDetail { …, revision }`.
  - [x] T14.3 `GgufPart` y `DownloadRequest` derivan también `Deserialize`. Se borra `GgufVariant::download_request` (código muerto: la UI construye la petición con la revisión de la ficha) y sus dos tests se sustituyen por los de esta tabla. `UnitRequest::try_from(DownloadRequest)` valida `repo_id` (`InvalidRequest` si `RepoId::parse` falla), revisión (`is_valid_revision`), `sha256` (32 bytes con `hex::decode_to_slice`) y `size > 0`.
  - [x] T14.4 `ui/src/lib/hf/types.ts`: `ModelInfo.sha: string | null` y `ModelDetail.revision: string`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `model_info_url_pide_sha` | `model_info_url` | contiene `expand[]=sha` |
  | `parse_model_info_con_sha` | fixture con `"sha": "9217f5db79a29953eb74d5343926648285ec7e67"` | `info.sha == Some(…)` |
  | `parse_model_info_sin_sha` | fixture actual sin `sha` | `info.sha == None` |
  | `tree_url_con_revision` | revisión `9217…7e67` | termina en `/tree/9217…7e67?recursive=true` |
  | `model_detail_usa_sha_del_commit` | info con `sha`; mock solo en `/tree/<sha>` | `detail.revision == sha`; 0 peticiones a `/tree/main` |
  | `model_detail_sin_sha_usa_main` | info sin `sha` | `detail.revision == "main"`, árbol pedido en `/tree/main` |
  | `model_detail_sha_invalido_usa_main` | `sha: "../x"` | `detail.revision == "main"` |
  | `revision_valida` | `"main"`, 40 hex minúsculas | `true` |
  | `revision_invalida` | `""`, `"Main"`, 39 hex, 40 con `G`, `"../main"`, `"main/x"` | `false` en todos |
  | `download_request_deserializa` | JSON `{"repo_id":"a/b","revision":"main","parts":[{"path":"m.gguf","size":10,"sha256":null}]}` | `DownloadRequest` igual |
  | `unit_request_valido` | petición de 2 partes con sha de 64 hex | `Ok`, 2 partes en orden |
  | `unit_request_repo_invalido` | `repo_id: "bad id"` | `Err(InvalidRequest(_))` |
  | `unit_request_revision_invalida` | `revision: "../x"` | `Err(InvalidRequest(_))` |
  | `unit_request_sha_laxo` | `sha256` `"+f"` × 32, 63 hex, 65 hex | `Err(InvalidRequest(_))` en los tres |
  | `unit_request_tamano_cero` | `size: 0` | `Err(InvalidRequest(_))` |
  | `json_model_detail_para_la_ui` (en `src-tauri`, actualizado) | `ModelDetail` con `info.sha` y `revision` | JSON exacto con `"sha"` en `info` y `"revision"` en la raíz |

### T15 — Puente Tauri: estado, comandos, eventos y errores
- **Ficheros/módulos:** `src-tauri/src/commands/downloads.rs` (nuevo), `src-tauri/src/commands.rs`, `src-tauri/src/commands/hf.rs` (`HfAppState.service` pasa a `Arc<HfService>` con un accesor), `src-tauri/src/lib.rs` (registro y `setup`)
- **Depende de:** T13, T14
- **Cubre:** CA9, CA13 (backend)
- **Subtareas:**
  - [x] T15.1 `TauriEventSink<R: Runtime>` sobre `AppHandle::emit` con `downloads://state` y `downloads://progress`. Si `emit` falla, la descarga sigue (como en `runtime_install`).
  - [x] T15.2 `DownloadsAppState { manager: Arc<DownloadManager> }` y `production_state(data_dir, hf: Arc<HfService>, app: AppHandle) -> Result<DownloadsAppState, Box<dyn Error>>` con `reqwest::Client` propio (user agent `app-llama/<versión>`, `connect_timeout` 30 s, `read_timeout` 60 s, sin timeout total), `SystemClock`, `TokioSleeper`, `Fs4DiskSpace`, `JsonQueueStore`, `max_active = 1`, `data_dir = app_local_data_dir()` (no `app_data_dir()`), `models_dir = data_dir/models` y la cola en `data_dir/downloads.json` (decisión del usuario, 2026-11-28; ver Contexto). En `setup`, después del estado de HF.
  - [x] T15.3 Comandos: `downloads_enqueue_hf(request: DownloadRequest) -> DownloadId`, `downloads_list() -> Vec<DownloadSnapshot>`, `downloads_pause(id)`, `downloads_resume(id)`, `downloads_cancel(id)`, `downloads_retry(id)` y `downloads_remove(id)`. Las órdenes devuelven `Result<(), ErrorPayload>`. `downloads_remove` es la única orden que no produce ningún evento (`remove` no pasa por `EventSink`, que se queda con `state` y `progress`): su `Ok(())` es la señal de que la unidad ya no está en la cola y la UI la quita al recibirlo (T16.3). No se añade un evento de borrado.
  - [x] T15.4 `From<DownloadError> for ErrorPayload` con `download_error_code` (un código por variante; para `Failed(kind)`, el nombre de `kind`) y mensajes en español sin datos sensibles.
- **Tests unitarios** (los comandos son una capa fina; la lógica se prueba en `core`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `error_payload_download_codes` | cada variante de `DownloadError` | código esperado (`"UnsafePath"`, `"IncompleteSplit"`, `"AlreadyQueued"`, `"DestinationExists"`, `"InvalidTransition"`, `"UnknownDownload"`, `"InvalidRequest"`, `"Cancelled"`, `"Io"`, `"Network"`, `"Gated"`…) y mensaje no vacío |
  | `error_payload_download_never_contains_token` | `Io` con un mensaje que contiene un token `hf_…` | el JSON del payload no contiene el token |
  | `json_download_snapshot_para_la_ui` | snapshot `Failed(InsufficientSpace { needed: 65498251264, available: 42949672960 })` con 2 ficheros | JSON exacto, p. ej. `"state":{"Failed":{"InsufficientSpace":{"needed":65498251264,"available":42949672960}}}` |
  | `json_download_snapshot_estado_simple` | snapshot `Paused` | `"state":"Paused"` |
  | `json_download_progress_para_la_ui` | progreso con `eta_secs: None` y `phase: Verify` | JSON exacto con `"eta_secs":null,"phase":"Verify"` |
  | `json_download_request_desde_la_ui` | JSON que envía `VariantRow` | deserializa a `DownloadRequest` |

### T16 — UI: tipos, API, formato y store de descargas
- **Ficheros/módulos:** `ui/src/lib/downloads/types.ts`, `api.ts`, `format.ts`, `store.svelte.ts` y sus `*.test.ts`
- **Depende de:** T15
- **Cubre:** CA13
- **Subtareas:**
  - [x] T16.1 `types.ts`: espejo de `DownloadSnapshot`, `DownloadProgress`, `UnitState`, `FailureKind` y `FileStatus` (comentario que remite a los tests `json_*_para_la_ui` de `commands/downloads.rs`). `api.ts`: un envoltorio por comando con `invoke`, como `ui/src/lib/hf/api.ts`.
  - [x] T16.2 `format.ts` en **GiB** (base 1024, una decimal, coma `es-ES`): `formatBytes` (`B`, `KiB`, `MiB`, `GiB`), `formatSpeed` (`…/s`) y `formatEta` (`null` → `—`). `failureMessage(kind)`.
  - [x] T16.3 `store.svelte.ts` (runas de Svelte 5): funciones puras `applyState(units, snapshot)` (inserta o reemplaza por `id`, descarta `version` menor; regla de `progress` abajo), `applyProgress(units, event)` (solo si la unidad está en `Downloading` o `Verifying`; guarda el progreso **por fase**, `progress: { download?: PhaseProgress; verify?: PhaseProgress; phase: ProgressPhase }`, donde `PhaseProgress` lleva `downloaded_bytes`, `total_bytes`, `bytes_per_sec` y `eta_secs`, y `phase` es la del último evento; un evento solo reemplaza la entrada de su `phase` y nunca se comparan valores de fases distintas, porque pueden bajar entre fases, Diseño §6), `mergeList(units, list, knownBefore: ReadonlySet<number>)`, `removeUnit(units, id)` y `actionsFor(state)` según la tabla del Diseño §8. Clase `DownloadsStore` con `start()` (se suscribe con `listen` de `@tauri-apps/api/event` y llama a `downloads_list`), `stop()` y `remove(id)`.
    - **`applyState` y el `progress` (#92, decisión del usuario: se limpia en el store):** al reemplazar una unidad, conserva su `progress` **solo** si el estado anterior y el nuevo son ambos de ejecución (`Downloading` o `Verifying`, incluido el mismo estado: `Downloading` → `Downloading`, `Downloading` → `Verifying`, `Verifying` → `Downloading`). Así la barra no se vacía al empezar a verificar (CA13). En cualquier otra transición (hacia `Queued`, `Paused`, `Failed`, `Cancelled` o `Completed`) lo borra (`progress` ausente), y la vista pinta esa unidad con los datos del snapshot (T17.2). Motivo: tras `Failed(ChecksumMismatch)` → `Queued` (reintentar), o al pausar durante el hash de una parte (la unidad sigue en `Downloading`, Diseño §5), la unidad mostraba «Verificando…» y bytes viejos. La misma regla aplica cuando `mergeList` reemplaza una unidad por la de la lista con `version` mayor.
    - **`start()` y `stop()` con token de generación (#91):** el store guarda un contador `generation`. `stop()` lo incrementa y quita los `unlisteners` que haya. `start()` llama a `stop()`, captura `const gen = this.generation` y, **tras cada `await`** (el `Promise.all` de los `listen` y el `invoke("downloads_list")`), comprueba si `this.generation !== gen`: si ha cambiado, llama a los `unlisten` que acaba de obtener (si aún no los ha guardado) y sale **sin** asignar `unlisteners` ni fusionar la lista. Solo si la generación sigue igual asigna `unlisteners` y aplica `mergeList`. Cubre montar y desmontar `Downloads.svelte` enseguida (T17.2) y dos `start()` concurrentes: el segundo `start()` invalida al primero, que limpia lo suyo, y nunca quedan suscripciones vivas tras `stop()`.
    - **`mergeList`:** la respuesta de `downloads_list` es la **lista completa** de la cola. Las unidades presentes se fusionan por `id` y `version` (no pisa una `version` mayor que llegó por evento). Una unidad del store **ausente** de la lista se quita solo si su `id` está en `knownBefore`, el conjunto de `id` que tenía el store justo antes de enviar la petición (`DownloadsStore` lo captura antes del `invoke`). Las que llegaron por evento después de enviar la petición se conservan aunque falten en la lista, porque pueden ser posteriores a la foto del backend. Es correcto porque un evento recibido antes de enviar la petición implica que la unidad ya existía cuando el backend hizo la foto: si falta, es que se quitó.
    - **`DownloadsStore.remove(id)`:** llama a `downloads_remove` y, solo si responde `Ok`, aplica `removeUnit` (quita por `id`; un `id` desconocido no cambia nada). Si rechaza, la unidad sigue y el error se propaga a quien llamó, sin tocar el store.
- **Tests unitarios** (Vitest; IPC con `mockIPC(…, { shouldMockEvents: true })`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `formatBytes_unidades` | 0, 1023, 1536, 4831838208, 64424509440 | `"0 B"`, `"1023 B"`, `"1,5 KiB"`, `"4,5 GiB"`, `"60,0 GiB"` |
  | `formatSpeed_mib_s` | 12897485 | `"12,3 MiB/s"` |
  | `formatEta_casos` | `null`, 0, 59, 200, 7322 | `"—"`, `"0 s"`, `"59 s"`, `"3 min 20 s"`, `"2 h 2 min"` |
  | `failureMessage_espacio` | `{ InsufficientSpace: { needed: 65498251264, available: 42949672960 } }` | contiene `"61,0 GiB"` y `"40,0 GiB"` |
  | `failureMessage_por_tipo` | `"Gated"`, `"Unauthorized"`, `"Network"`, `"ChecksumMismatch"` | 4 textos no vacíos y distintos; `Gated` menciona aceptar las condiciones en la web del modelo |
  | `applyState_inserta_y_reemplaza` | snapshot `version` 1 y después el mismo `id` con `version` 2 | 1 unidad con el estado de la 2 |
  | `applyState_descarta_version_vieja` | `version` 3 y después `version` 2 | conserva la 3 |
  | `applyState_conserva_el_progreso` | unidad 1 `Downloading` (`version` 1) con progreso `Download` 10 de 10; después snapshot `Verifying` (`version` 2) | `progress.download.downloaded_bytes == 10` (no se borra al pasar de `Downloading` a `Verifying`, CA13) |
  | `applyState_limpia_progreso_al_salir_de_ejecucion` | (a) unidad 1 `Downloading` (`version` 1); progreso `Verify` 5 de 10; snapshot `Failed("ChecksumMismatch")` (`version` 2); snapshot `Queued` (`version` 3). (b) unidad 2 `Downloading` con progreso `phase: "Verify"` 5 de 10; snapshot `Paused` con `version` mayor | (a) tras `Failed` y tras `Queued`, la unidad 1 no tiene `progress` (`undefined`). (b) la unidad 2 en `Paused` no tiene `progress` |
  | `mergeList_no_pisa_evento_nuevo` | estado con `id` 1 `version` 5; lista con `id` 1 `version` 4 e `id` 2; `knownBefore = {1}` | `id` 1 sigue en `version` 5; `id` 2 añadido; sin duplicados |
  | `mergeList_quita_ausentes` | estado con `id` 1 y `id` 2; lista solo con `id` 2; `knownBefore = {1, 2}` | queda solo `id` 2 |
  | `mergeList_lista_vacia_quita_conocidas` | estado con `id` 1; lista `[]`; `knownBefore = {1}` | estado vacío |
  | `mergeList_conserva_llegadas_por_evento` | estado con `id` 1 y `id` 3 (el 3 llegó por evento tras enviar la petición); lista solo con `id` 1; `knownBefore = {1}` | quedan `id` 1 e `id` 3 |
  | `removeUnit_quita_por_id` | estado con `id` 1 y `id` 2; `removeUnit(units, 1)`; después `removeUnit(units, 9)` | queda solo `id` 2; el `id` 9 desconocido no cambia nada |
  | `quitar_desaparece_de_la_lista` | `start()` con `downloads_list` mockeado que devuelve `id` 1 `Completed` e `id` 2 `Paused`; `store.remove(1)` con `downloads_remove` que responde `null`; después `downloads_list` devuelve solo `id` 2, `stop()` y `start()` de nuevo | tras `remove`: `invoke("downloads_remove", { id: 1 })` y el store solo tiene `id` 2; tras la nueva lista, sigue solo `id` 2 (el 1 no reaparece) |
  | `quitar_rechazado_conserva_unidad` | `downloads_remove` rechaza con un `ErrorPayload` | `store.remove(1)` rechaza con **el mismo** objeto (aserto por identidad, lección `aserto-debil`); `id` 1 sigue en el store |
  | `applyProgress_actualiza_unidad` | unidad 1 `Downloading` + progreso id 1 `phase: "Download"` | `progress.download` con bytes, velocidad y ETA del evento; `progress.phase == "Download"` |
  | `applyProgress_por_fase` | unidad 1 `Downloading`; progreso `Download` 1048576 de 2097152; después `Verify` 524288 de 2097152; después `Download` 1572864 de 2097152 | tras los dos primeros, `progress.download.downloaded_bytes == 1048576` y `progress.verify.downloaded_bytes == 524288` (se conservan los dos); tras el tercero, `progress.download.downloaded_bytes == 1572864`, `progress.verify.downloaded_bytes` sigue en 524288 y `progress.phase == "Download"` |
  | `applyProgress_id_desconocido` | progreso id 9 sin snapshot | estado sin cambios |
  | `applyProgress_tras_pausa_ignorado` | unidad 1 `Paused` + progreso id 1 | sin cambios |
  | `actions_por_estado` | `Queued`, `Downloading`, `Verifying`, `Paused`, `Failed`, `Completed`, `Cancelled` | `[pause,cancel]`, `[pause,cancel]`, `[cancel]`, `[resume,cancel]`, `[retry,remove]`, `[remove]`, `[remove]` |
  | `store_se_suscribe_a_eventos` | `start()`, después `emit("downloads://state", snapshot)` mockeado | la unidad aparece en el store; tras `stop()` un nuevo evento no la cambia |
  | `start_y_stop_inmediato_no_deja_suscripciones` | `mockIPC(…, { shouldMockEvents: true })` con `downloads_list` → `[]`; `const p = store.start(); store.stop(); await p;` después `emit("downloads://state", snapshot id 1)` | el store sigue vacío (ninguna suscripción viva) y la lista de `start` no se fusiona |
  | `start_concurrente_no_pierde_suscripciones` | mismo mock; `await Promise.all([store.start(), store.start()]); store.stop();` después `emit("downloads://state", snapshot id 1)` | el store no cambia (las suscripciones de los dos `start()` se han quitado) |
  | `api_enqueue_pasa_la_peticion` | `downloadsEnqueueHf(request)` | `invoke("downloads_enqueue_hf", { request })` con el objeto idéntico; el rechazo propaga **el mismo** `ErrorPayload` (aserto por identidad, lección `aserto-debil`) |

### T17 — UI: vista "Descargas", navegación y botón "Descargar" de la ficha
- **Ficheros/módulos:** `ui/src/lib/downloads/DownloadItem.svelte`, `ui/src/views/Downloads.svelte`, `ui/src/lib/navigation.ts`, `ui/src/App.svelte`, `ui/src/views/Explore.svelte`, `ui/src/lib/hf/ModelDetail.svelte`, `ui/src/lib/hf/VariantRow.svelte`; tests `navigation.test.ts`, `App.test.ts`, `ModelDetail.test.ts`, `Explore.test.ts` y nuevos `DownloadItem.test.ts`, `Downloads.test.ts`
- **Depende de:** T16
- **Cubre:** CA12, CA13
- **Subtareas:**
  - [ ] T17.1 Sección `{ id: "downloads", label: "Descargas" }` en `SECTIONS`, justo después de `explore`. `App.svelte` pasa `onNavigate(id)` a la vista activa.
  - [ ] T17.2 `Downloads.svelte`: `DownloadsStore.start()` al montar y `stop()` al desmontar; lista con `{#each units as unit (unit.id)}` (clave única: el store no admite dos unidades con el mismo `id`). `DownloadItem.svelte`: nombre, barra principal `<progress max={total} value={downloaded}>` alimentada **solo** por `progress.download` (T16.3) y sin `style` en línea, bytes, velocidad y ETA de la descarga. Cuando el último evento es `Verify` (`progress.phase == "Verify"`) o la unidad está en `Verifying`, se añade la etiqueta «Verificando» con su propio `<progress>` (sin `style` en línea) alimentado por `progress.verify`, y la velocidad y la ETA mostradas pasan a ser las del hash con el formato «Verificando… 1,5 GiB/s» (`formatSpeed`) y la ETA de `formatEta`; la barra principal conserva su último valor de `Download` (Diseño §6: las fases son independientes). Si la unidad está en `Verifying` sin ningún evento `Verify` aún, se muestra «Verificando…» sin velocidad. **Sin `progress`** (estados fuera de ejecución, donde el store lo borra, T16.3, o una unidad en ejecución sin eventos aún), la barra principal y los bytes salen del snapshot: `max = total_bytes` y `value = Σ size` de las partes `Verified` o `Preexisting` (una `Completed` sale llena); no se muestran velocidad, ETA ni «Verificando» (salvo en `Verifying`, regla anterior). `failureMessage` si está en `Failed`, y botones de `actionsFor`. Si muestra las partes, la clave del `each` es `relative_path` (única: T2 rechaza duplicados).
  - [ ] T17.3 `VariantRow.svelte`: nueva prop `revision`, se borra `const DEFAULT_REVISION` y el botón deja de estar deshabilitado. `ModelDetail.svelte` pasa `detail.revision`. `Explore.svelte` pasa `onDownload`: llama a `downloadsEnqueueHf`, navega a "Descargas" si va bien y, si falla, muestra `errorMessage` en la ficha con `role="alert"`.
- **Tests unitarios** (Vitest + Testing Library):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `secciones_en_orden` (actualizado) | `SECTIONS` | `["explore", "downloads", "library", "run", "chat", "settings"]` |
  | `secciones_con_etiqueta_en_espanol` (actualizado) | `SECTIONS` | `["Explorar", "Descargas", "Biblioteca", "Ejecutar", "Chat", "Ajustes"]` |
  | `muestra_seis_botones_de_navegacion` (renombra `muestra_cinco_botones_de_navegacion`) | `render(App)` | 6 botones con las etiquetas de `SECTIONS` |
  | `item_descargando` | snapshot `Downloading` + progreso 1288490189 de 4831838208 a 12897485 B/s, ETA 200 | textos `"1,2 GiB de 4,5 GiB"`, `"12,3 MiB/s"`, `"3 min 20 s"`; botones Pausar y Cancelar |
  | `item_verificando_no_retrocede` | snapshot `Downloading` de 4831838208 bytes; progreso `Download` 4831838208 de 4831838208; después `Verify` 0 de 4831838208 a 1610612736 B/s, ETA 3 | la barra principal sigue con `value == 4831838208` (no cae a 0); aparece «Verificando»; hay un segundo `<progress>` con `value == 0`; textos `"Verificando… 1,5 GiB/s"` y `"3 s"` |
  | `item_sin_progreso_usa_snapshot` | snapshot `Paused` de 4831838208 bytes con 2 partes: una `Verified` de 1073741824 y otra `Pending` de 3758096384; sin `progress` | barra principal con `max == 4831838208` y `value == 1073741824`; texto `"1,0 GiB de 4,5 GiB"`; no aparece «Verificando» ni ningún texto con `/s` |
  | `item_botones_por_estado` | `Paused`, `Failed("Network")`, `Completed` | Reanudar+Cancelar; Reintentar+Quitar; Quitar |
  | `item_pulsar_pausar` | clic en Pausar | `invoke("downloads_pause", { id: 1 })` |
  | `item_progress_sin_style` | render | el `<progress>` no tiene atributo `style` |
  | `vista_lista_inicial_y_eventos` | `downloads_list` con 1 unidad; después evento de estado de otra | 2 unidades renderizadas, sin aviso `each_key_duplicate` |
  | `variant_row_descargar_habilitado` | variante completa | botón "Descargar" habilitado |
  | `variant_row_envia_revision_de_la_ficha` | `revision` `"9217…7e67"`, clic | `onDownload` recibe `{ repo_id, revision: "9217…7e67", parts }` con las partes exactas |
  | `ficha_descargar_encola_y_navega` (`Explore.test.ts`) | abrir ficha, clic en Descargar | `invoke("downloads_enqueue_hf", { request })` y la sección activa pasa a "Descargas" |
  | `ficha_error_de_alta_se_muestra` | `downloads_enqueue_hf` rechaza con `{ code: "AlreadyQueued", message: "Ya está en la cola" }` | `role="alert"` con ese mensaje; sigue en "Explorar" |

  Comprobación adicional del reviewer (no es un test): `grep '"main"' ui/src` no devuelve ningún literal de revisión (CA12).

### T18 — Migración del runtime (004) a la descarga con reanudación
- **Ficheros/módulos:** `crates/core/src/download/single.rs`; `crates/core/src/runtime/fetch.rs`, `crates/core/src/runtime/mod.rs` (`RuntimeError::Network`, exportaciones); `src-tauri/src/commands/runtime.rs` (`production_state` y `error_code`)
- **Depende de:** T6, T7
- **Cubre:** CA14
- **Subtareas:**
  - [ ] T18.1 `download_one(fetcher, SingleRequest { url, dest, expected_size: u64, sha256: Option<String> }, sleeper, cancel: &CancellationToken, on_progress) -> Result<(), DownloadError>`: reutiliza `transfer_with_retry`, `verify_file` y el renombrado; sin cola, persistencia ni token (`hf_origin: None`, `auth = &|| None` según T9.4). Llama a `transfer_with_retry` con la firma de T12.4: `TransferObserver { on_bytes, on_restart }` por valor, donde `on_bytes` alimenta `on_progress` y `on_restart` rebobina su progreso a 0.
  - [ ] T18.2 `runtime::CancelToken` pasa a envolver un `tokio_util::sync::CancellationToken` (misma API pública: `cancel`, `is_cancelled`, `check`), más `pub(crate) fn as_cancellation(&self) -> &CancellationToken`. Así la cancelación de la instalación también corta una espera de reintento o una conexión parada.
  - [ ] T18.3 `ResumableHttpFetcher::new(client, sleeper)` implementa `ArtifactFetcher` con `download_one`: convierte `sha256: [u8; 32]` a hex y mapea los errores: `SizeMismatch` → `RuntimeError::SizeMismatch`, `ChecksumMismatch` → `IntegrityMismatch`, `Cancelled` → `Cancelled`, `NotFound`/`Unauthorized`/`Gated` → `HttpStatus(404/401/403)`, `Network` → `RuntimeError::Network` (nueva, código `"Network"` en `commands/runtime.rs`), `Io` → `Io`. Ante cualquier error borra el `.part` (el staging se borra igualmente; se conserva el contrato "el fichero final solo existe si la descarga es íntegra").
  - [ ] T18.4 Se borra `SimpleHttpFetcher` y sus tests se portan a `ResumableHttpFetcher` con los mismos nombres y aserciones. `production_state` de `commands/runtime.rs` usa `ResumableHttpFetcher`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `download_one_completa` | wiremock, 512 KiB | fichero final correcto, sin `.part` |
  | `download_one_reanuda_part_previo` | `.part` de 100 KiB | petición con `Range: bytes=102400-` |
  | `download_one_sha_incorrecto` | sha incorrecto | `Err(Failed(ChecksumMismatch))`, sin final |
  | `download_one_reintenta_corte` | `cutting_server` que corta una vez tras 256 KiB | fichero correcto en 2 peticiones |
  | `fetch_ok_verifica_hash_y_tamano` … `fetch_cancelado_a_mitad_de_descarga` (portados de `fetch.rs`: `fetch_sin_digest_solo_tamano`, `fetch_hash_distinto`, `fetch_tamano_distinto`, `fetch_http_404`, `fetch_cancelado`, `fetch_progreso_monotono`, `fetch_sigue_redirecciones`, `fetch_tamano_excedido`, `fetch_chunked_excedido`) | los mismos que hoy, contra `ResumableHttpFetcher` con `FakeSleeper` | las mismas aserciones (errores `RuntimeError` exactos y `assert_no_files`) |
  | `fetch_corte_persistente_es_network` | puerto de un `TcpListener` soltado | `Err(RuntimeError::Network)`, `FakeSleeper` con `[1, 2, 4, 8]` s |
  | `cancel_token_compartido` (existente) | — | sigue en verde |
  | `cancel_token_corta_la_espera` | `cancel()` sobre el `CancelToken` | `as_cancellation().is_cancelled()` |
  | `error_payload_codigos` (actualizado, `commands/runtime.rs`) | añade `RuntimeError::Network` | código `"Network"`, mensaje no vacío |
  | tests de `install.rs` | — | siguen en verde sin cambios |

### T19 — Comprobación en vivo (Windows)
- **Ficheros/módulos:** sección "Verificación en vivo" de este plan (resultado anotado al cerrarlo)
- **Depende de:** T17, T18
- **Cubre:** CA15
- **Subtareas:**
  - [ ] T19.1 En Windows: descargar `qwen2.5-0.5b-instruct-q4_k_m.gguf` de `Qwen/Qwen2.5-0.5B-Instruct-GGUF`, pausar a mitad, cerrar la app, abrirla (aparece en `Paused`), reanudar y comprobar `Completed`, que no queda `.part` y que `certutil -hashfile <fichero> SHA256` es igual al `lfs.oid` del árbol en la revisión usada.
  - [ ] T19.2 Con un modelo dividido real pequeño: una sola unidad, ningún `.gguf` final antes de terminar.
  - [ ] T19.3 Con un repositorio gated sin aceptar la licencia: mensaje de `Gated` en la UI, y el token no aparece en `downloads.json`.
  - [ ] T19.4 Instalar o actualizar llama.cpp desde Ajustes sigue funcionando con `ResumableHttpFetcher`.
- **Tests unitarios:** no aplica (comprobación manual). Los equivalentes automáticos están en T5–T18. macOS se cubre solo por la CI (decisión del usuario).

## Verificación en vivo

_Pendiente (T19)._

## Fuera de alcance

- Descargas en paralelo de varias partes o por segmentos (varias conexiones `Range` por fichero). El máximo es 1 unidad activa, sin UI para cambiarlo.
- El protocolo Xet nativo (deduplicación por chunks). Se usa la descarga HTTP de `/resolve/`.
- Limitar el ancho de banda y programar descargas.
- Reanudación automática de la cola al arrancar: se restaura en `Paused` (decisión del usuario).
- Guardar el estado del SHA256 para no rehashear al final.
- Elegir el directorio de modelos desde la UI y escanear la biblioteca (Fase 4).
- Descargas de fuentes que no sean HF, salvo `download_one` para el runtime.
- Notificaciones del sistema al terminar una descarga.
- Modelos `mmproj` como parte de la unidad (MVP §6; la ficha ya los excluye).
- Las issues abiertas de 005 que no afectan a la integración: **#41** (carrera entre guardar y borrar el token) y **#45** (llamadas síncronas al llavero en código async). Este plan no las arregla. Su diseño evita empeorarlas: la descarga lee el token en memoria y nunca llama al llavero.
- Pruebas manuales en macOS.

## Riesgos

| Riesgo | Mitigación |
|---|---|
| La URL firmada de la CDN caduca durante una descarga larga o una pausa | Nunca se persiste. Cada intento vuelve a pedir `/resolve/`. Un 403 desde un origen distinto del de HF se trata como transitorio (T4 `classify_403_cdn_reintenta`) |
| `reqwest` reenvía `Authorization` en una redirección | Comprobado en 005 con `reqwest 0.13`; se vuelve a fijar con un test (T9). Si fallara, una política de redirección propia quita la cabecera |
| El fichero cambia en HF entre la pausa y la reanudación | Revisión = commit `sha` (T14), y el árbol se pide en ese mismo commit. Además, el `Content-Range` total debe ser igual a `expected_size` y el SHA256 final lo detecta |
| La ficha queda abierta mucho tiempo y el commit deja de ser el último | Se descarga el commit que el usuario vio, que sigue siendo descargable por `/resolve/<sha>/`. Es el comportamiento buscado |
| La webview envía un `DownloadRequest` manipulado (rutas, hashes, revisión) | El backend valida todo (Contexto, #55) y construye él la URL sobre `base_url`. El destino siempre cuelga de `models_dir/<owner>/<name>/` |
| Hashear 50+ GB tarda minutos, parece colgado o bloquea el runtime de tokio | `spawn_blocking`, progreso propio (`phase: Verify`, emitido tanto en `Downloading` al hashear cada parte descargada como en `Verifying` con las candidatas, Diseño §5 y §6), etiqueta «Verificando» en la UI (T17.2) y cancelable |
| Pausar con la conexión parada tarda hasta el `read_timeout` (60 s) | `select!` contra `cancel.cancelled()` en cada lectura y en cada espera (T5 `cancelacion_durante_espera_de_respuesta`, T6 `cancel_durante_backoff`) |
| Reutilizar por error el cliente HTTP de `commands/hf.rs`, con timeout total de 30 s, cortaría toda descarga larga | T15 crea un cliente propio sin timeout total; queda escrito en T5.1 |
| El espacio libre cambia durante la descarga | El error de escritura se mapea a `Failed(Io { message })` con un mensaje fijo, sin el texto del sistema ni rutas (T12.6). El `.part` se conserva para reintentar |
| Rutas largas en Windows (>260 caracteres) o nombres reservados (`CON`, `NUL`) en repositorios anidados | `safe_join` + `Path`; se documenta en T19 si aparece. Si se reproduce, issue aparte (prefijo `\\?\`, filtrado de nombres reservados) |
| Una URL firmada de la CDN acaba en `downloads.json` o en la UI a través del `Display` de `reqwest::Error` | Regla del Diseño §3: nunca `reqwest::Error::to_string()` para `Io`/`Network`; `without_url()` o mensaje propio en un único punto (T5.4). Tests `error_reqwest_no_filtra_url_firmada` (T5) y `fallo_red_agotado_no_filtra_url_firmada` (T6) |
| Dos peticiones distintas en texto (`M.gguf` / `m.gguf`, `x.gguf.` / `x.gguf`) acaban en el mismo fichero en Windows o macOS y se descargan a la vez | `AlreadyQueued` compara la clave normalizada `dest_key` (mayúsculas carácter a carácter sin reglas de contexto, sin `.`/` ` finales), T11.5. No se usa `str::to_lowercase` por la regla de la sigma final (#79; tests `dest_key_sigma_final` y `enqueue_duplicado_sigma_rechazado`) |
| En macOS (APFS/HFS+) también pueden chocar en el mismo fichero las formas NFC/NFD de un nombre y otros pares que `dest_key` no iguala (`ſ`/`s`) | Limitación conocida del MVP: no se normaliza a NFC para no añadir dependencias. Se cubre sobre todo porque los nombres de ficheros de HF son ASCII. Si aparece un caso real, issue aparte |
| Antivirus de Windows que bloquea el `rename` del `.part` | Si aparece en T19, issue aparte con reintento del `rename` mediante `Sleeper` |
| Carrera de #41: guardar y borrar el token a la vez puede dejar el token en memoria distinto del del llavero | La descarga usa el de memoria, igual que el explorador; el comportamiento es coherente con 005. Se arregla en #41 |
| `fs4` no expone `available_space` en la versión elegida o falla en macOS | El developer lo comprueba en T8 (la CI de macOS lo ejecuta en `fs4_directorio_inexistente_usa_ancestro`). Alternativa: `sysinfo` con la feature de discos |
| Un servidor que ignora siempre el `Range` (o responde `416` sin motivo) provoca un bucle infinito de reinicios (issue #67) | Todo `Restart` cuenta como intento sin progreso con backoff, el progreso se mide contra la marca máxima del `.part` y un `416` con offset 0 falla (Diseño §3 y §4; tests `reinicios_repetidos_acaban_fallando`, `respuesta_416_tras_reinicio_falla` y `classify_416_offset_cero_no_reinicia`) |
| Sin origen de HF conocido (`hf_origin: None`, runtime de T18), un `401`/`403` se reintentaría 5 veces y acabaría en `Failed(Network)` | `from_hf_origin` vale `true` por defecto (T5.1); tests `sin_origen_hf_401_es_unauthorized`, `sin_origen_hf_403_es_gated` y `sin_origen_hf_401_sin_reintento`. Coste aceptado: un `403` de la CDN de GitHub en T18 no se reintenta. Se conserva así el comportamiento de 004: `SimpleHttpFetcher` ya trataba cualquier respuesta no exitosa como definitiva (`RuntimeError::HttpStatus(403)`, sin reintento), y la URL firmada de `objects.githubusercontent.com` se usa nada más llegar la redirección |
| La cabecera `Authorization` se calcula una vez por fichero y un token guardado o borrado a mitad de descarga no se aplica hasta el fichero siguiente | `transfer_with_retry` recibe un closure que se evalúa en cada intento (T9.4); tests `token_se_relee_en_cada_intento` y `token_borrado_entre_intentos` |
| Tras un `Restart` (servidor que ignora `Range`), la UI suma lo re-descargado a lo que ya tenía el `.part` y muestra más bytes que el total (356 KiB de 256 KiB) | `on_restart` en `transfer_with_retry` y `ProgressReporter::rewind` a la cantidad absoluta (T12.4, T12.5); test `reinicio_no_supera_total` |
| El proceso muere a mitad de `finalize_unit` (parte 1 renombrada, parte 2 en `.part`): al reanudar, `DestinationExists` y `Failed`; «Quitar» deja un `.gguf` suelto | Una parte `Verified` sin `.part` y con destino de `expected_size` cuenta como ya renombrada (`finalize_unit` idempotente) y «Quitar» en `Failed` borra esos finales (T13.4, T13.5). Coste aceptado: un fichero ajeno con el mismo nombre y tamaño exacto, copiado a mano en el destino mientras la unidad no ha terminado, se tomaría como propio; el riesgo es mínimo porque el destino cuelga de `models_dir/<owner>/<name>/` y `AlreadyQueued` impide que otra unidad viva o `Failed` lo use (T11.5) |
| Una unidad `Failed` y otra nueva de la misma variante comparten ficheros: la nueva toma como `Preexisting` el final que renombró la fallida y continúa su `.part`; al quitar la fallida, T13.5 borra ese final y la nueva queda `Completed` sin modelo, o T13.2 borra el `.part` que la nueva está escribiendo (issue #72) | Dos capas. (a) `AlreadyQueued` también rechaza los destinos de unidades `Failed` (T11.5; tests `enqueue_rechaza_destino_de_unidad_fallida` y `enqueue_rechaza_destino_fallido_con_otra_grafia`). (b) `cancel` y `remove` no borran un fichero que otra unidad sigue necesitando, según `protected_keys`: un `.part`, solo si lo comparte una unidad no terminal o `Failed`; un final, solo si otra unidad no `Cancelled` tiene esa parte `Preexisting` o `Verified` ya renombrada; y `remove` sobre una `Cancelled` no borra ningún final (T13.6, #87). Las `Cancelled` no protegen nada y las `Completed` no protegen ningún `.part`, así que cancelar una unidad encolada de nuevo tras una `Cancelled`, o tras una `Completed` cuyo final se borró, sí borra su `.part` (CA5) (Diseño §8, T13.2; tests `protected_keys_segun_estado`, `quitar_no_borra_ficheros_de_otra_unidad`, `quitar_no_borra_part_de_otra_unidad`, `cancelar_no_borra_ficheros_de_otra_unidad`, `cancelar_no_borra_part_de_unidad_viva`, `cancelar_tras_otra_cancelada_borra_su_part` y `cancelar_tras_otra_completada_sin_final_borra_su_part`). Coste aceptado: si el proceso muere a mitad de `finalize_unit` de la nueva unidad y luego se cancela, el final que ya renombró se conserva porque la `Completed` lo da por suyo (mismo destino y tamaño) |
| Una candidata a `Preexisting` cuyo hash no coincide se detecta en `Verifying`, después de descargar el resto de la unidad (puede ser mucho tiempo de descarga para acabar en `Failed(DestinationExists)`) | Coste de experiencia de uso aceptado: hashearla antes ocultaría minutos de trabajo tras un estado `Queued` sin progreso (Diseño §7, paso 4). Lo descargado no se pierde: las partes quedan `Verified` con su `.part`, y si el usuario aparta el final ajeno y pulsa «Reintentar», solo se descarga esa parte |
| El final de una candidata desaparece (lo borra el usuario) mientras se descarga el resto de la unidad | Al verificarla en `Verifying`, abrir el final falla y la unidad pasa a `Failed(Io { message })` con mensaje fijo, sin ruta (Diseño §9, T12.2 y T12.6). Al reintentar, la parte ya no es candidata: es una `Pending` normal, cuenta en la comprobación de espacio y se descarga (test `candidata_desaparecida_falla_io_y_reintento_descarga`) |
| La comprobación de espacio cuenta las candidatas a `Preexisting` (llegan como `Pending` con `.part` de 0): volver a encolar un modelo de 60 GiB que ya está en disco, con 20 GiB libres, acaba en `Failed(InsufficientSpace)` (issue #73) | El runner clasifica las candidatas antes de comprobar el espacio, las excluye de `required_bytes` y no llama a `check_space` si no hay nada que descargar. El hash se calcula después, en `Verifying` (Diseño §7 y §9, T12.2; tests `reencolar_completado_sin_espacio_completa_sin_peticiones`, `candidata_no_cuenta_para_el_espacio`, `candidata_sin_espacio_para_el_resto_falla`, `pendiente_sin_final_cuenta_para_el_espacio` y `quitar_completada_y_reencolar_sin_espacio`). Las partes `Verified` sin ficheros vuelven a `Pending` antes de todo eso (paso 0), así que sí cuentan (tests `verificada_sin_fichero_espacio_justo_completa` y `verificada_sin_fichero_cuenta_para_el_espacio`). `disk.rs` no cambia |
| Falla el borrado al cancelar (p. ej., un antivirus retiene el `.part` en Windows): la unidad queda `Cancelled` con un `.part` huérfano y `cancel` ya no vale desde `Cancelled` | `cancel` devuelve el error pero la unidad pasa igualmente a `Cancelled`, y «Quitar» sobre una `Cancelled` vuelve a intentar borrar **solo sus `.part`**, respetando `protected_keys.parts`; si vuelve a fallar, la unidad sigue en la lista para reintentarlo. Nunca borra finales: una parte que era `Verified` como `.part` sigue `Verified` en la `Cancelled`, y la regla de «ya renombrada» tomaría como propio el `.gguf` de una descarga posterior de la misma variante ya quitada (#87) (Diseño §8, T13.6; tests `quitar_cancelada_reintenta_borrar_part`, `quitar_cancelada_respeta_part_de_unidad_viva`, `quitar_cancelada_no_borra_final_de_descarga_posterior` y `quitar_cancelada_conserva_unidad_si_falla_borrado`). **Coste aceptado:** si el borrado de `cancel` falla justo en un final ya renombrado, ese final se queda en disco y «Quitar» no lo limpia |
| «Quitar» no emite ningún evento y la UI podría seguir mostrando la unidad, incluso tras volver a pedir `downloads_list` | El `Ok` de `downloads_remove` quita la unidad del store, y `mergeList` trata la lista como completa: quita las ausentes que el store ya conocía al enviar la petición y conserva las que llegaron por evento después (T15.3, T16.3; tests `mergeList_quita_ausentes`, `mergeList_conserva_llegadas_por_evento` y `quitar_desaparece_de_la_lista`). Coste aceptado: si un evento `downloads://state` tardío de esa unidad llegara después del `Ok`, la volvería a mostrar hasta la siguiente lista; no ocurre en la práctica porque solo se quitan unidades terminales, cuyo último evento ya se ha recibido antes de que el usuario pulse «Quitar» |
| `wiremock` no puede cortar una respuesta a mitad | Servidor TCP mínimo propio en `test_support` (T6.3) |
| Tests lentos por tamaños grandes | Los casos de más de 50 GB solo usan tamaños declarados (aritmética y cabeceras); los cuerpos reales son ≤ 10 MiB |
| Cambiar `ModelInfo`/`ModelDetail` rompe el espejo de la UI | T14 actualiza en el mismo commit `json_model_detail_para_la_ui` y `ui/src/lib/hf/types.ts` |

## Preguntas abiertas

Ninguna.
