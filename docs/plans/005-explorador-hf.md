# 005 — Explorador de Hugging Face (búsqueda, ficha de modelo y token)

- **Tipo:** feature
- **Estado:** borrador
- **Rama:** feat/005-explorador-hf
- **Fase del roadmap:** 2. Hugging Face ([docs/MVP.md](../MVP.md), funcionalidad B sin descargas)

## Objetivo

Que el usuario encuentre desde la app modelos GGUF de Hugging Face, ordenados por descargas o por likes, y vea en su ficha cada cuantización disponible con su tamaño real (los modelos divididos cuentan como un solo archivo) y una estimación orientativa de la memoria que necesita. También puede guardar su token de HF de forma segura para acceder a modelos privados o restringidos (gated).

## Contexto

### Punto de partida

- Se dan por hechos el **plan 003** (esqueleto: workspace Rust con el crate `core` en `crates/core`, `src-tauri/` con Tauri 2, `ui/` con Svelte 5 + TypeScript y Vitest) y el **plan 004** (runtime de llama.cpp). Hoy el repositorio no tiene código Rust ni UI, así que las rutas concretas siguen la arquitectura de `docs/MVP.md` §3.
- **Supuesto de rutas:** la lógica vive en el módulo `crates/core/src/hf/` del crate `core`. Si el plan 003 define `crates/core/hf` como crate propio, solo cambian las rutas, no el diseño (ver Preguntas abiertas).
- Decisiones ya tomadas: API REST directa de HF (sin el crate `hf-hub`), `reqwest` + `tokio`, token en el llavero del sistema con el crate `keyring` (R6). `core` no depende de Tauri (R7).
- La descarga real es el **plan 006**. Este plan solo deja el tipo `DownloadRequest` y el botón «Descargar» como punto de integración, deshabilitado.
- La memoria a largo plazo (`docs/memory/long/`) no tiene decisiones ni errores registrados todavía.

### API de Hugging Face verificada (2026-11-26)

La referencia oficial está en el [OpenAPI del Hub](https://huggingface.co/.well-known/openapi.md) ([página índice](https://huggingface.co/docs/hub/api)). Además se han hecho llamadas reales a la API. Las fixtures de los tests se recortan de esas respuestas.

| Endpoint | Uso en este plan | Forma observada |
|---|---|---|
| `GET /api/models?search=<q>&filter=gguf&sort=<downloads\|likes>&direction=-1&limit=20&expand[]=downloads&expand[]=likes&expand[]=gated&expand[]=lastModified&expand[]=pipeline_tag` | Búsqueda | Array de objetos `{ _id, id, downloads, likes, gated, lastModified, pipeline_tag? }`. Con `expand[]` solo vuelven los campos pedidos más `_id` e `id`. Puede faltar `pipeline_tag`. `gated` es `false`, `"auto"` o `"manual"`. |
| Paginación | «Cargar más» | Cabecera `Link: <URL completa con cursor=…>; rel="next"`. Sin `rel="next"` no hay más páginas. El OpenAPI describe así el cursor en otros listados ("Pagination cursor from the `Link` header"), y una página puede venir vacía. |
| `GET /api/models/{owner}/{name}?expand[]=gguf&expand[]=gated&expand[]=downloads&expand[]=likes&expand[]=lastModified&expand[]=cardData` | Ficha | `gguf` es opcional: `{ total, architecture, context_length, chat_template, bos_token, eos_token, totalFileSize, quantize_imatrix_file? }`. `total` es el número de parámetros (p. ej. `27320697856`). `cardData` puede incluir `license` y `base_model`. |
| `GET /api/models/{owner}/{name}/tree/main?recursive=true` | Lista de archivos | Array de `{ type: "file"\|"directory", oid, size, path, lfs?: { oid, size, pointerSize }, xetHash? }`. **Sin `recursive` no aparecen los archivos de las subcarpetas**: los splits pueden ir en carpetas (`BF16/Qwen3.8-27B-BF16-00001-of-00002.gguf`). `lfs.oid` es el SHA256. Paginado con `Link` (1000 entradas por defecto). En repos gated el árbol se lista sin token. |
| `GET /api/whoami-v2` con `Authorization: Bearer <token>` | Validar el token | El OpenAPI dice "Get information about the user and auth method used", sin esquema. Por `huggingface_hub` se sabe que devuelve `{ type, name, fullname?, orgs?, auth: { type, accessToken?: { displayName, role } } }` y `401` si el token no vale. Solo se usa `name` (obligatorio) y `auth.accessToken.role` (opcional). |

Hechos observados que condicionan el diseño:
- Un repo GGUF incluye archivos `.gguf` que **no** son modelos de texto: `mmproj-*.gguf` (multimodal, fuera del MVP) e `imatrix*.gguf`. Se excluyen.
- Nombres de cuantización reales: `Q4_0`, `Q8_0`, `UD-Q4_K_XL`, `UD-IQ2_XXS`, `Q6_K`, `BF16`, `MXFP4`, `llama-2-7b.Q4_K_M.gguf` (con punto), en minúsculas (`q4_k_m`)… A veces la cuantización está en la carpeta y no en el archivo.
- Formato de los splits: `<prefijo>-00001-of-00002.gguf`, con 5 dígitos.

### Diseño

1. **`crates/core/src/hf/`**, sin Tauri:
   - `model.rs`: tipos de dominio y DTO serde (`ModelSummary`, `ModelInfo`, `TreeEntry`, `Gated`, `GgufInfo`, `WhoAmI`).
   - `quant.rs`: `extract_quant(path) -> Option<String>`.
   - `files.rs`: `group_gguf_files(entries) -> Vec<GgufVariant>` (filtra, agrupa splits y calcula tamaños).
   - `memory.rs`: `estimate_memory(file_bytes, params_total, ctx) -> MemoryEstimate`.
   - `url.rs`: `RepoId` validado, construcción de URLs, `parse_next_link`.
   - `client.rs`: `HfClient` (reqwest) con la URL base inyectable (`https://huggingface.co` en producción, servidor mock en los tests) y el token opcional.
   - `token.rs`: `HfToken` (formato y `Debug` ocultado), trait `SecretStore`, `KeyringStore` y `MemoryStore` (fake de los tests).
   - `service.rs`: `HfService` combina el cliente y el `SecretStore`. Es la única API que usan los comandos.
   - `error.rs`: `HfError` (`thiserror`).
2. **`src-tauri/src/commands/hf.rs`**: comandos finos que delegan en `HfService` y convierten `HfError` en un `CommandError { kind, message }` serializable. **El token nunca vuelve a la UI.**
3. **`ui/src/lib/hf/`** + **`ui/src/routes|screens/Explorar`** (según la estructura del 003): envoltorio tipado de `invoke`, pantalla Explorar, ficha del modelo y diálogo del token.

### Fórmula de la estimación de memoria (orientativa)

```
memoria = bytes_archivo + KV(ctx) + OVERHEAD
KV(ctx) = ctx × (params_total × 16_384 / 1_000_000_000)   [bytes; aritmética entera u128, división truncada]
OVERHEAD = 512 MiB (536_870_912 bytes)
ctx por defecto = 4096
```

- El coeficiente `16_384 B/token por cada mil millones de parámetros` reproduce la caché KV f16 de Llama-3-8B (32 capas × 8 cabezas KV × 128 × 2 (K y V) × 2 bytes = 131 072 B/token). Para modelos con otra atención solo es una aproximación, y por eso la UI la etiqueta como **orientativa**.
- Si el repo no publica `gguf.total`, se omite `KV` y el resultado lleva `kv_included = false`. La UI muestra entonces «sin caché KV».
- La misma cifra vale para RAM (solo CPU) o VRAM (todas las capas en la GPU). La UI muestra una sola cifra con el texto «RAM o VRAM aprox. con contexto de 4096 tokens».

## Criterios de aceptación

- **CA1 — Búsqueda filtrada por GGUF.** Dado el texto «qwen» y el orden «Descargas», cuando el usuario busca, entonces se hace una única petición `GET /api/models` con `search=qwen`, `filter=gguf`, `sort=downloads`, `direction=-1`, `limit=20` y los `expand[]` de la tabla. Cada resultado muestra el id, las descargas, los likes, la fecha de última modificación y una insignia «Restringido» si `gated` ≠ `false`. Lo verifican `search_sends_expected_query` y `explorar_renders_results`.
- **CA2 — Orden por likes.** Dado el orden «Likes», cuando el usuario busca o cambia el orden, entonces la petición lleva `sort=likes&direction=-1` y la lista se sustituye (no se acumula). Lo verifican `search_url_sort_likes` y `explorar_sort_change_replaces_results`.
- **CA3 — Paginación.** Dada una respuesta con `Link: <…cursor=abc>; rel="next"`, cuando el usuario pulsa «Cargar más», entonces se pide exactamente esa URL y los nuevos resultados se añaden al final. Sin `rel="next"` el botón desaparece. Si la URL de `next` apunta a otro host o esquema distinto de la URL base, se ignora (no hay más páginas y nunca se envía el token a otro host). Lo verifican `parse_next_link_*`, `search_follows_next_cursor` y `explorar_load_more_appends`.
- **CA4 — Ficha con archivos GGUF.** Dado un repo con el árbol de la fixture `tree_unsloth.json`, cuando el usuario abre la ficha, entonces se pide `tree/main?recursive=true` (siguiendo la paginación `Link`) y se listan solo los `.gguf` de tipo `file`, sin `mmproj*` ni `*imatrix*`. Cada variante muestra su cuantización (o «desconocida»), su tamaño en GiB con un decimal y su número de partes. Las variantes van ordenadas por tamaño total ascendente y, si empatan, por ruta. Lo verifican `group_excludes_mmproj_and_imatrix`, `model_detail_assembles_variants` y `ficha_renders_variants`.
- **CA5 — Modelos divididos.** Dados `BF16/X-BF16-00001-of-00002.gguf` (49 986 159 616 B) y `BF16/X-BF16-00002-of-00002.gguf` (4 671 576 000 B), cuando se agrupan, entonces se obtiene **una** variante `BF16` con 2 partes ordenadas, `total_bytes = 54 657 735 616` y `complete = true`. Si falta alguna parte de `1..=M`, la variante sale con `complete = false` y la UI la marca «incompleta» sin botón de descarga. Lo verifican `group_split_two_parts` y `group_split_missing_part_incomplete`.
- **CA6 — Estimación de memoria.** Dados `bytes_archivo = 4 920 000 000`, `params_total = 8 000 000 000` y `ctx = 4096`, cuando se estima, entonces `bytes = 5 993 741 824` y `kv_included = true`, y la UI muestra «≈ 5,6 GiB» con la etiqueta «orientativa». Sin `params_total`: `bytes = 5 456 870 912` y `kv_included = false`. Lo verifican `estimate_with_params` y `estimate_without_params`.
- **CA7 — Guardar el token.** Dado un token con formato válido (`hf_` seguido de 30 o más caracteres `[A-Za-z0-9]`, tras recortar los espacios), cuando el usuario lo guarda, entonces se llama a `/api/whoami-v2` con `Authorization: Bearer <token>`. Si responde 200, el token se guarda en el llavero (servicio `app-llama`, cuenta `huggingface-token`) y la UI muestra «Conectado como <name>». Si responde 401, **no** se guarda y la UI muestra «Token no válido». Con un formato inválido no se hace ninguna petición. Lo verifican `save_token_valid_stores`, `save_token_unauthorized_not_stored`, `save_token_bad_format_no_request` y `token_dialog_*`.
- **CA8 — Borrar y consultar el token.** Dado un token guardado, cuando el usuario lo borra, entonces desaparece del llavero, `hf_token_status` devuelve `{ present: false }` y las peticiones siguientes no llevan `Authorization`. Borrar sin token guardado no es un error. Al abrir la app, `hf_token_status` devuelve `{ present: true, user }` validando con whoami (o `{ present: true, user: null, invalid: true }` si responde 401). Lo verifican `delete_token_*` y `status_*`.
- **CA9 — Uso del token.** Dado un token guardado, cuando se busca o se abre una ficha, entonces las peticiones a la URL base llevan `Authorization: Bearer <token>`. Sin token no llevan la cabecera. El token no aparece en ningún `Debug`, log, error ni respuesta de comando. Lo verifican `client_sends_bearer_when_token`, `client_no_auth_header_without_token`, `token_debug_is_redacted` y `command_error_never_contains_token`.
- **CA10 — Errores comprensibles.** Dada una respuesta 429 con `Retry-After: 30`, cuando se busca, entonces la UI muestra «Hugging Face limita las peticiones; reintenta en 30 s». Con 404 en la ficha: «Modelo no encontrado». Con 401/403 en la ficha: «Acceso denegado: revisa tu token». Con un JSON inesperado: «Respuesta inesperada de Hugging Face». Con un fallo de red: «Sin conexión con Hugging Face». Ningún error tumba la pantalla. Lo verifican `client_maps_status_*` y `explorar_shows_error`.
- **CA11 — Punto de integración de la descarga.** Dada una variante completa, cuando se muestra la ficha, entonces aparece un botón «Descargar» **deshabilitado**, con el título «Disponible próximamente», y existe `GgufVariant::download_request(repo_id) -> DownloadRequest { repo_id, revision: "main", parts: [{ path, size, sha256: Option<String> }] }`, con las partes en orden y `sha256` tomado de `lfs.oid`. Lo verifican `download_request_from_split_variant` y `ficha_download_button_disabled`.
- **CA12 — Validación del repo id.** Dado un id que no cumple `^[A-Za-z0-9][A-Za-z0-9._-]{0,95}/[A-Za-z0-9][A-Za-z0-9._-]{0,95}$` o que contiene `..`, cuando se pide la ficha, entonces se devuelve `HfError::InvalidRepoId` sin hacer ninguna petición. Lo verifican `repo_id_*`.
- **CA13 — Calidad.** `node scripts/quality-gate.mjs` en verde en Windows y macOS. Ningún test usa la red real (HTTP con un servidor mock local), el reloj real ni el llavero real del sistema.

## Tareas

> Orden recomendado: T1 → T2 → T3 → T4 → T5 → T6 → T7 → T8 → T9 → T10 → T11 → T12 → T13 → T14. T2, T4 y T5 son independientes entre sí.

### T1 — Tipos de dominio y parseo de respuestas de HF con fixtures
- **Ficheros/módulos:** `crates/core/src/hf/mod.rs`, `crates/core/src/hf/model.rs`, `crates/core/src/hf/error.rs`, `crates/core/fixtures/hf/{search_page.json,model_info_gguf.json,model_info_no_gguf.json,model_info_gated_manual.json,tree_unsloth.json,tree_flat.json,whoami.json}`, `crates/core/Cargo.toml` (serde, serde_json, thiserror)
- **Depende de:** — (plan 003 hecho)
- **Cubre:** CA1, CA4, CA7 (parseo)
- **Subtareas:**
  - [ ] T1.1 Fixtures recortadas de respuestas reales: sin `chat_template` y con 2 o 3 resultados por búsqueda. `tree_unsloth.json` lleva un directorio, `.gitattributes`, `README.md`, 3 `.gguf` simples, 2 partes BF16 en `BF16/`, `mmproj-F16.gguf` e `imatrix_unsloth.gguf`.
  - [ ] T1.2 `Gated` (`Open` | `Auto` | `Manual`) con un deserializador que acepta `false`, `"auto"` y `"manual"`. Cualquier otro valor es un error de parseo.
  - [ ] T1.3 `ModelSummary { id, downloads, likes, gated, last_modified: Option<String>, pipeline_tag: Option<String> }`, `ModelInfo { id, gated, downloads, likes, gguf: Option<GgufInfo>, license: Option<String> }`, `GgufInfo { total: Option<u64>, architecture: Option<String>, context_length: Option<u64> }`, `TreeEntry { kind: File|Directory, path, size, lfs: Option<LfsInfo { oid, size }> }` y `WhoAmI { name, role: Option<String> }`. Se ignoran los campos desconocidos.
  - [ ] T1.4 `HfError` con las variantes `Network`, `Unauthorized`, `Forbidden`, `NotFound`, `RateLimited { retry_after_secs: Option<u64> }`, `Status(u16)`, `Parse(String)`, `InvalidRepoId`, `InvalidToken`, `TooManyPages`, `ForeignUrl` y `Secret(String)`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `parse_search_page` | `search_page.json` | `Vec<ModelSummary>` con la longitud de la fixture; el primero con `id`, `downloads` y `likes` exactos |
  | `parse_search_missing_pipeline_tag` | elemento sin `pipeline_tag` | `pipeline_tag == None`, sin error |
  | `parse_gated_false` | `"gated": false` | `Gated::Open` |
  | `parse_gated_auto` | `"gated": "auto"` | `Gated::Auto` |
  | `parse_gated_manual` | `model_info_gated_manual.json` | `Gated::Manual` |
  | `parse_gated_unknown_value_fails` | `"gated": "other"` | error de parseo |
  | `parse_model_info_with_gguf` | `model_info_gguf.json` | `gguf.total == Some(27320697856)`, `architecture == Some("qwen35")` |
  | `parse_model_info_without_gguf` | `model_info_no_gguf.json` | `gguf == None` |
  | `parse_tree_entries` | `tree_unsloth.json` | número de entradas exacto; el directorio `BF16` con `kind == Directory`; el `.gguf` LFS con `lfs.oid` de 64 hex |
  | `parse_tree_non_lfs_file` | entrada `config.json` sin `lfs` | `lfs == None`, `size == 3760` |
  | `parse_whoami` | `whoami.json` | `name` correcto, `role == Some("read")` |
  | `parse_whoami_without_access_token` | whoami sin `auth.accessToken` | `role == None`, sin error |
  | `parse_ignores_unknown_fields` | JSON con un campo extra `foo` | parsea sin error |

### T2 — Extracción de la cuantización del nombre de archivo
- **Ficheros/módulos:** `crates/core/src/hf/quant.rs`
- **Depende de:** T1 (solo por el módulo `hf`)
- **Cubre:** CA4
- **Subtareas:**
  - [ ] T2.1 Se quita la extensión `.gguf` y el sufijo de split `-NNNNN-of-NNNNN`. El nombre se trocea por `-` y `.` y se recorren los trozos **de derecha a izquierda**. El primero que encaja (sin distinguir mayúsculas) con `^(?:I?Q[1-8](?:_[0-9A-Z]{1,3}){0,3}|TQ[12]_0|BF16|F16|F32|MXFP4(?:_MOE)?)$` es la cuantización, normalizada a mayúsculas.
  - [ ] T2.2 Si el nombre del archivo no tiene ninguna, se prueban los componentes de la carpeta de derecha a izquierda (con `/` como separador lógico de las rutas de HF, que no son rutas del sistema).
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `quant_simple_q4_0` | `Qwen3.8-27B-Q4_0.gguf` | `Some("Q4_0")` |
  | `quant_ud_prefix` | `Qwen3.8-27B-UD-Q4_K_XL.gguf` | `Some("Q4_K_XL")` |
  | `quant_iq_xxs` | `Qwen3.8-27B-UD-IQ2_XXS.gguf` | `Some("IQ2_XXS")` |
  | `quant_iq4_nl` | `model-IQ4_NL.gguf` | `Some("IQ4_NL")` |
  | `quant_dot_separator` | `llama-2-7b.Q4_K_M.gguf` | `Some("Q4_K_M")` |
  | `quant_lowercase_normalized` | `model-q8_0.gguf` | `Some("Q8_0")` |
  | `quant_bf16_split` | `BF16/Qwen3.8-27B-BF16-00001-of-00002.gguf` | `Some("BF16")` |
  | `quant_f16` | `model-f16.gguf` | `Some("F16")` |
  | `quant_mxfp4` | `gpt-oss-20b-MXFP4.gguf` | `Some("MXFP4")` |
  | `quant_tq1_0` | `model-TQ1_0.gguf` | `Some("TQ1_0")` |
  | `quant_from_directory` | `Q4_K_M/model-00001-of-00003.gguf` | `Some("Q4_K_M")` |
  | `quant_model_name_digits_not_matched` | `Qwen2.5-7B-Instruct.gguf` | `None` |
  | `quant_rightmost_wins` | `Q8_0-distill-Q4_K_M.gguf` | `Some("Q4_K_M")` |
  | `quant_none` | `model.gguf` | `None` |
  | `quant_empty_path` | `""` | `None` |

### T3 — Filtrado de GGUF y agrupación de modelos divididos
- **Ficheros/módulos:** `crates/core/src/hf/files.rs`
- **Depende de:** T1, T2
- **Cubre:** CA4, CA5, CA11
- **Subtareas:**
  - [ ] T3.1 Filtro: `kind == File`, extensión `.gguf` sin distinguir mayúsculas y nombre base que no empiece por `mmproj` ni contenga `imatrix` (sin distinguir mayúsculas).
  - [ ] T3.2 Split: `^(?P<prefix>.+)-(?P<idx>\d{5})-of-(?P<total>\d{5})\.gguf$` (sin distinguir mayúsculas en la extensión). Clave de grupo: `(prefix con carpeta, total)`. Un `idx == 0`, `idx > total` o `total == 0` se trata como archivo suelto.
  - [ ] T3.3 `GgufVariant { name, quant: Option<String>, parts: Vec<GgufPart { path, size, sha256: Option<String> }>, total_bytes, complete }`. `size` es `lfs.size` si existe y, si no, `size`. Las partes van ordenadas por índice. `complete` es `true` si están todas de `1..=total` sin duplicados. Un archivo suelto es una variante de 1 parte con `complete = true`. `name` es la ruta sin el sufijo de split ni la extensión.
  - [ ] T3.4 Orden de las variantes: `total_bytes` ascendente y, si empatan, `name`.
  - [ ] T3.5 `GgufVariant::download_request(&RepoId) -> DownloadRequest` (con `revision = "main"`). El tipo `DownloadRequest` se define aquí para que lo consuma el plan 006.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `group_single_file` | 1 entrada `a-Q4_0.gguf` de 100 B | 1 variante, 1 parte, `total_bytes == 100`, `complete`, `quant == Some("Q4_0")` |
  | `group_split_two_parts` | las 2 partes BF16 de CA5, en orden inverso | 1 variante `BF16`, partes ordenadas 1 y 2, `total_bytes == 54_657_735_616`, `complete` |
  | `group_split_missing_part_incomplete` | partes 1 y 3 de `-of-00003` | 1 variante, `complete == false`, `total_bytes` = suma de las presentes |
  | `group_split_duplicate_part_incomplete` | parte 1 dos veces de `-of-00002` | `complete == false` |
  | `group_split_index_zero_is_single` | `x-00000-of-00002.gguf` | variante suelta de 1 parte |
  | `group_split_index_above_total_is_single` | `x-00003-of-00002.gguf` | variante suelta de 1 parte |
  | `group_excludes_mmproj_and_imatrix` | `tree_unsloth.json` | ninguna variante contiene `mmproj` ni `imatrix` |
  | `group_excludes_directories_and_non_gguf` | directorio + `README.md` + `config.json` | lista vacía |
  | `group_extension_case_insensitive` | `model-Q8_0.GGUF` | 1 variante |
  | `group_prefers_lfs_size` | entrada con `size = 136` y `lfs.size = 1000` | `total_bytes == 1000` |
  | `group_same_prefix_different_dirs` | `a/m-00001-of-00001.gguf` y `b/m-00001-of-00001.gguf` | 2 variantes |
  | `group_sorted_by_size_then_name` | 3 variantes con tamaños 300, 100, 100 (nombres `b`, `c`, `a`) | orden `a`, `c`, `b` |
  | `group_empty_tree` | `[]` | `[]` |
  | `download_request_from_split_variant` | variante BF16 de 2 partes | `DownloadRequest` con `revision == "main"`, 2 partes en orden, `sha256` = `lfs.oid` |
  | `download_request_non_lfs_has_no_sha` | variante de 1 parte sin `lfs` | `sha256 == None` |

### T4 — Estimación orientativa de memoria
- **Ficheros/módulos:** `crates/core/src/hf/memory.rs`
- **Depende de:** T1 (solo por el módulo)
- **Cubre:** CA6
- **Subtareas:**
  - [ ] T4.1 Constantes `DEFAULT_CTX = 4096`, `OVERHEAD_BYTES = 536_870_912` y `KV_BYTES_PER_TOKEN_PER_BILLION = 16_384`.
  - [ ] T4.2 `estimate_memory(file_bytes: u64, params_total: Option<u64>, ctx: u32) -> MemoryEstimate { bytes: u64, kv_included: bool }`, con aritmética `u128` y saturación a `u64::MAX`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `estimate_with_params` | `4_920_000_000`, `Some(8_000_000_000)`, `4096` | `bytes == 5_993_741_824`, `kv_included` |
  | `estimate_without_params` | `4_920_000_000`, `None`, `4096` | `bytes == 5_456_870_912`, `!kv_included` |
  | `estimate_ctx_zero` | `1000`, `Some(8_000_000_000)`, `0` | `bytes == 1000 + 536_870_912`, `kv_included` |
  | `estimate_kv_truncates` | `0`, `Some(1)`, `1` | `bytes == 536_870_912` (KV trunca a 0) |
  | `estimate_scales_with_ctx` | `0`, `Some(8_000_000_000)`, `8192` | `bytes == 536_870_912 + 1_073_741_824` |
  | `estimate_saturates` | `u64::MAX`, `Some(u64::MAX)`, `u32::MAX` | `bytes == u64::MAX`, sin pánico |

### T5 — Repo id, construcción de URLs y paginación `Link`
- **Ficheros/módulos:** `crates/core/src/hf/url.rs`, `crates/core/Cargo.toml` (`url`)
- **Depende de:** T1
- **Cubre:** CA1, CA2, CA3, CA12
- **Subtareas:**
  - [ ] T5.1 `RepoId::parse(&str) -> Result<RepoId, HfError>` según la regex de CA12, rechazando `..`.
  - [ ] T5.2 `SearchParams { query: String, sort: SortBy::{Downloads, Likes}, limit: u32 }` → `search_url(base, &params)`, con la query codificada en porcentaje y los `expand[]` de la tabla del Contexto. Una query vacía o solo con espacios omite `search`.
  - [ ] T5.3 `model_info_url(base, &RepoId)` y `tree_url(base, &RepoId)` (con `recursive=true`). El `/` del repo id no se codifica.
  - [ ] T5.4 `parse_next_link(header: &str, base: &Url) -> Option<Url>`: extrae la URL con `rel="next"` de entre varias separadas por comas y solo la devuelve si su esquema, host y puerto coinciden con `base`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `repo_id_valid` | `unsloth/Qwen3.8-27B-GGUF` | `Ok` |
  | `repo_id_missing_owner` | `Qwen3-8B` | `Err(InvalidRepoId)` |
  | `repo_id_three_segments` | `a/b/c` | `Err(InvalidRepoId)` |
  | `repo_id_dotdot` | `a/..` | `Err(InvalidRepoId)` |
  | `repo_id_query_injection` | `a/b?x=1` | `Err(InvalidRepoId)` |
  | `repo_id_empty` | `""` | `Err(InvalidRepoId)` |
  | `search_url_sort_downloads` | `query="qwen"`, `Downloads`, `20` | contiene `search=qwen`, `filter=gguf`, `sort=downloads`, `direction=-1`, `limit=20` y los 5 `expand[]` |
  | `search_url_sort_likes` | `Likes` | contiene `sort=likes` |
  | `search_url_encodes_query` | `query="llama 3&x"` | `search=llama+3%26x` o `llama%203%26x`; no aparece un `&x` suelto |
  | `search_url_blank_query_omits_search` | `query="  "` | sin el parámetro `search` |
  | `tree_url_recursive` | `RepoId("a/b")` | termina en `/api/models/a/b/tree/main?recursive=true` |
  | `model_info_url_expands` | `RepoId("a/b")` | contiene `expand[]=gguf` y `expand[]=gated` |
  | `parse_next_link_simple` | `<https://huggingface.co/api/models?cursor=abc>; rel="next"` | `Some` con `cursor=abc` |
  | `parse_next_link_multiple_rels` | `<…p>; rel="prev", <…n>; rel="next"` | la URL `n` |
  | `parse_next_link_absent` | `<…>; rel="prev"` | `None` |
  | `parse_next_link_other_host_ignored` | `<https://evil.example/api>; rel="next"` | `None` |
  | `parse_next_link_other_scheme_ignored` | `<http://huggingface.co/…>; rel="next"` con base https | `None` |
  | `parse_next_link_malformed` | `garbage` | `None` |

### T6 — Cliente HTTP: búsqueda paginada y mapeo de errores
- **Ficheros/módulos:** `crates/core/src/hf/client.rs`, `crates/core/Cargo.toml` (`reqwest` con `rustls-tls` + `json`, `tokio`; dev: `wiremock` o el servidor mock que fije el plan 004)
- **Depende de:** T1, T5
- **Cubre:** CA1, CA3, CA9, CA10
- **Subtareas:**
  - [ ] T6.1 `HfClient::new(base_url: Url, token: Option<HfToken>)` con un `User-Agent` `app-llama/<versión del crate>` y un timeout configurable (por defecto 30 s; en los tests no se espera a que venza).
  - [ ] T6.2 `search(&SearchParams) -> SearchPage { items, next: Option<Url> }` y `search_next(&Url) -> SearchPage`. `search_next` devuelve la nueva variante `HfError::ForeignUrl` si la URL no comparte esquema, host y puerto con la URL base (defensa en profundidad, además de T5.4).
  - [ ] T6.3 Mapeo de estados: 401 → `Unauthorized`, 403 → `Forbidden`, 404 → `NotFound`, 429 → `RateLimited` (lee `Retry-After` en segundos; si falta o no es numérico, `None`), otro no-2xx → `Status(code)`, JSON inválido → `Parse`, error de conexión → `Network`. Ningún mensaje incluye la cabecera `Authorization`.
  - [ ] T6.4 `Authorization: Bearer` solo si hay token y solo hacia la URL base.
- **Tests unitarios** (con un servidor mock local; sin red real):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `search_sends_expected_query` | mock que exige los parámetros de CA1 y responde `search_page.json` | `Ok` con los elementos de la fixture |
  | `search_follows_next_cursor` | 1.ª respuesta con `Link` a `/api/models?cursor=abc` en el mock; luego `search_next` | 2.ª petición con `cursor=abc`; `next == None` en la 2.ª página |
  | `search_empty_page` | respuesta `[]` sin `Link` | `items` vacío, `next == None` |
  | `search_next_foreign_url_rejected` | `search_next("https://evil.example/…")` | `Err(ForeignUrl)`, 0 peticiones al mock |
  | `client_sends_bearer_when_token` | token `hf_` + 34 caracteres | el mock recibe `Authorization: Bearer hf_…` |
  | `client_no_auth_header_without_token` | sin token | el mock no recibe `Authorization` |
  | `client_maps_status_401` | mock 401 | `Err(Unauthorized)` |
  | `client_maps_status_403` | mock 403 | `Err(Forbidden)` |
  | `client_maps_status_404` | mock 404 | `Err(NotFound)` |
  | `client_maps_status_429_with_retry_after` | 429 + `Retry-After: 30` | `Err(RateLimited { retry_after_secs: Some(30) })` |
  | `client_maps_status_429_without_retry_after` | 429 sin cabecera | `Err(RateLimited { retry_after_secs: None })` |
  | `client_maps_status_500` | 500 | `Err(Status(500))` |
  | `client_maps_invalid_json` | 200 con `{"no":"array"}` | `Err(Parse(_))` |
  | `client_maps_connection_refused` | URL base a un puerto local cerrado | `Err(Network)` |

### T7 — Ficha del modelo: info + árbol recursivo paginado
- **Ficheros/módulos:** `crates/core/src/hf/client.rs`, `crates/core/src/hf/service.rs` (parte de la ficha)
- **Depende de:** T3, T4, T6
- **Cubre:** CA4, CA5, CA6, CA12
- **Subtareas:**
  - [ ] T7.1 `HfClient::model_info(&RepoId)` y `HfClient::tree(&RepoId) -> Vec<TreeEntry>`, que sigue `Link rel="next"` hasta un máximo de `MAX_TREE_PAGES = 20`. Si se supera: `Err(TooManyPages)`.
  - [ ] T7.2 `HfService::model_detail(repo: &str) -> ModelDetail { info: ModelInfo, variants: Vec<VariantView { variant, estimate: MemoryEstimate }> }`, con `ctx = DEFAULT_CTX` y `params_total = info.gguf.total`. Primero valida el `RepoId` (sin red si no vale).
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `tree_requests_recursive` | mock que exige `recursive=true` | `Ok` |
  | `tree_follows_pagination` | 2 páginas encadenadas con `Link` | entradas de las 2 páginas en orden |
  | `tree_too_many_pages` | mock cuyo `Link` apunta siempre a otra página (21 veces) | `Err(TooManyPages)` tras 20 peticiones |
  | `model_detail_assembles_variants` | `model_info_gguf.json` + `tree_unsloth.json` | variantes sin `mmproj`/`imatrix`, BF16 agrupada en 2 partes, cada una con `kv_included == true` |
  | `model_detail_without_gguf_metadata` | `model_info_no_gguf.json` + `tree_flat.json` | variantes con `kv_included == false` |
  | `model_detail_invalid_repo_no_request` | `"bad id"` | `Err(InvalidRepoId)`, 0 peticiones |
  | `model_detail_not_found` | mock 404 en info | `Err(NotFound)` |

### T8 — Token: tipo seguro y almacén de secretos
- **Ficheros/módulos:** `crates/core/src/hf/token.rs`, `crates/core/Cargo.toml` (`keyring`, con los features nativos de Windows y macOS)
- **Depende de:** T1
- **Cubre:** CA7, CA8, CA9
- **Subtareas:**
  - [ ] T8.1 `HfToken::parse(&str) -> Result<HfToken, HfError::InvalidToken>`: recorta y exige `^hf_[A-Za-z0-9]{30,}$`. `Debug` y `Display` muestran `hf_***`. `expose()` es solo `pub(crate)`.
  - [ ] T8.2 `trait SecretStore { fn get(&self) -> Result<Option<HfToken>, HfError>; fn set(&self, &HfToken) -> Result<(), HfError>; fn delete(&self) -> Result<(), HfError>; }`. `delete` sin entrada es `Ok(())`.
  - [ ] T8.3 `KeyringStore` (servicio `app-llama`, cuenta `huggingface-token`). Los errores del llavero se convierten en `HfError::Secret(msg)` sin el valor del token. **No lleva test unitario** porque toca el llavero real; se cubre con la verificación manual (CA7/CA8).
  - [ ] T8.4 `MemoryStore` (fake con `Mutex<Option<HfToken>>`) para los tests, y un `FailingStore` que siempre devuelve `Secret`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `token_parse_valid` | `hf_` + 34 alfanuméricos | `Ok` |
  | `token_parse_trims_whitespace` | `"  hf_…  \n"` | `Ok`, sin espacios |
  | `token_parse_wrong_prefix` | `ghp_abc…` | `Err(InvalidToken)` |
  | `token_parse_too_short` | `hf_abc` | `Err(InvalidToken)` |
  | `token_parse_inner_space` | `hf_abc def…` | `Err(InvalidToken)` |
  | `token_parse_empty` | `""` | `Err(InvalidToken)` |
  | `token_debug_is_redacted` | `format!("{:?}", token)` | `"hf_***"`; no contiene el token |
  | `token_display_is_redacted` | `format!("{}", token)` | `"hf_***"` |
  | `memory_store_roundtrip` | `set` y después `get` | `Some(token)` |
  | `memory_store_delete_absent_ok` | `delete` sin entrada | `Ok(())` |
  | `memory_store_delete_then_get` | `set`, `delete`, `get` | `None` |

### T9 — Servicio del token: validar con whoami, guardar, borrar y estado
- **Ficheros/módulos:** `crates/core/src/hf/client.rs` (`whoami`), `crates/core/src/hf/service.rs`
- **Depende de:** T6, T8
- **Cubre:** CA7, CA8, CA9
- **Subtareas:**
  - [ ] T9.1 `HfClient::whoami(&HfToken) -> WhoAmI` contra `/api/whoami-v2`.
  - [ ] T9.2 `HfService::new(base_url, Box<dyn SecretStore>)`. El cliente se reconstruye con el token actual cada vez que este cambia.
  - [ ] T9.3 `save_token(raw) -> WhoAmI`: valida el formato (sin red si falla), luego whoami y solo con 200 guarda. `delete_token()`. `token_status() -> TokenStatus { present, user: Option<String>, invalid: bool }`: con 401 devuelve `invalid = true` sin borrar; con error de red, `present = true, user = None, invalid = false`.
- **Tests unitarios** (mock HTTP + `MemoryStore`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `save_token_valid_stores` | whoami 200 `whoami.json` | `Ok(WhoAmI{name})`; el store contiene el token |
  | `save_token_unauthorized_not_stored` | whoami 401 | `Err(Unauthorized)`; store vacío |
  | `save_token_bad_format_no_request` | `"abc"` | `Err(InvalidToken)`; 0 peticiones |
  | `save_token_store_failure` | whoami 200 + `FailingStore` | `Err(Secret(_))` |
  | `save_token_then_search_uses_bearer` | guardar y después `search` | la búsqueda lleva `Authorization` |
  | `delete_token_removes_and_drops_header` | guardar, borrar, `search` | store vacío; la búsqueda va sin `Authorization` |
  | `delete_token_absent_ok` | borrar sin token | `Ok(())` |
  | `status_no_token` | store vacío | `{ present: false, user: None, invalid: false }`; 0 peticiones |
  | `status_valid_token` | store con token + whoami 200 | `{ present: true, user: Some(name), invalid: false }` |
  | `status_revoked_token` | store con token + whoami 401 | `{ present: true, user: None, invalid: true }`; el token sigue guardado |
  | `status_network_error` | store con token + puerto cerrado | `{ present: true, user: None, invalid: false }` |

### T10 — Comandos Tauri del explorador
- **Ficheros/módulos:** `src-tauri/src/commands/hf.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` (registro y `State<HfService>` con `KeyringStore`)
- **Depende de:** T7, T9
- **Cubre:** CA1–CA11 (puente)
- **Subtareas:**
  - [ ] T10.1 Comandos: `hf_search(query, sort) -> SearchPageDto`, `hf_search_next(next_url) -> SearchPageDto`, `hf_model_detail(repo_id) -> ModelDetailDto`, `hf_token_status() -> TokenStatus`, `hf_token_save(token) -> WhoAmIDto` y `hf_token_delete()`. Los DTO van en `camelCase` y `next` es un `String` opaco.
  - [ ] T10.2 `to_command_error(HfError) -> CommandError { kind: "network"|"unauthorized"|"forbidden"|"notFound"|"rateLimited"|"invalidRepoId"|"invalidToken"|"parse"|"status"|"secret"|"tooManyPages"|"foreignUrl", message, retryAfterSecs? }`. Es una función pura y testeable. Los mensajes en español son los de CA10.
  - [ ] T10.3 Los comandos no contienen lógica: solo delegan y convierten.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `command_error_rate_limited` | `RateLimited{Some(30)}` | `kind == "rateLimited"`, `retryAfterSecs == 30`, mensaje con «30 s» |
  | `command_error_not_found` | `NotFound` | `kind == "notFound"`, «Modelo no encontrado» |
  | `command_error_unauthorized` | `Unauthorized` | «Acceso denegado: revisa tu token» |
  | `command_error_parse` | `Parse("x")` | «Respuesta inesperada de Hugging Face» |
  | `command_error_network` | `Network` | «Sin conexión con Hugging Face» |
  | `command_error_never_contains_token` | `Secret("…")` construido a partir de un fallo con token | el JSON serializado no contiene `hf_` seguido del token |
  | `model_detail_dto_camel_case` | `ModelDetail` de ejemplo | JSON con `totalBytes`, `kvIncluded` y `parts[].sha256` |
  | `token_status_dto_has_no_token_field` | `TokenStatus` serializado | solo las claves `present`, `user` e `invalid` |

### T11 — UI: cliente tipado y utilidades de formato
- **Ficheros/módulos:** `ui/src/lib/hf/types.ts`, `ui/src/lib/hf/api.ts`, `ui/src/lib/hf/format.ts` y sus `*.test.ts`
- **Depende de:** T10
- **Cubre:** CA1, CA4, CA6, CA10
- **Subtareas:**
  - [ ] T11.1 Tipos TS que replican los DTO de T10.
  - [ ] T11.2 `api.ts`: funciones que envuelven `invoke` (inyectable para los tests) y convierten el rechazo en un `HfUiError { kind, message, retryAfterSecs? }`.
  - [ ] T11.3 `format.ts`: `formatGiB(bytes)` con 1 decimal y coma decimal (`Intl.NumberFormat('es-ES')`), `formatCount(n)` (`12,3 mil`, `8,2 M`) y `formatDate(iso)` en `es-ES` y UTC (sin depender de la zona horaria de la máquina).
- **Tests unitarios (Vitest):**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `format_gib_one_decimal` | `5_993_741_824` | `"5,6 GiB"` |
  | `format_gib_zero` | `0` | `"0,0 GiB"` |
  | `format_gib_large` | `54_657_735_616` | `"50,9 GiB"` |
  | `format_count_millions` | `8_181_570` | `"8,2 M"` |
  | `format_count_thousands` | `12_345` | `"12,3 mil"` |
  | `format_count_small` | `999` | `"999"` |
  | `format_date_utc` | `"2026-11-12T02:15:52.000Z"` | `"12/11/2026"` |
  | `api_search_invokes_command` | `search("qwen","downloads")` con un `invoke` falso | llamado con `("hf_search", { query: "qwen", sort: "downloads" })` |
  | `api_maps_rejection_to_ui_error` | `invoke` rechaza `{kind:"notFound",message:"…"}` | lanza un `HfUiError` con `kind == "notFound"` |
  | `api_maps_unknown_rejection` | `invoke` rechaza la cadena `"boom"` | `HfUiError` con `kind == "unknown"` |

### T12 — UI: pantalla Explorar (búsqueda, orden y paginación)
- **Ficheros/módulos:** `ui/src/lib/hf/Explorar.svelte`, `ui/src/lib/hf/ModelCard.svelte`, `ui/src/lib/hf/explorar.svelte.ts` (estado con runas), el registro de la pantalla en la navegación del plan 003 y sus tests
- **Depende de:** T11
- **Cubre:** CA1, CA2, CA3, CA10
- **Subtareas:**
  - [ ] T12.1 Estado `ExplorarState`: `query`, `sort`, `items`, `next`, `loading` y `error`. `search()` sustituye los resultados; `loadMore()` los añade y evita peticiones dobles mientras `loading`.
  - [ ] T12.2 La búsqueda se lanza con Enter o con el botón (sin debounce por tiempo, para no depender del reloj). Al cambiar el orden se relanza la búsqueda.
  - [ ] T12.3 Tarjeta: id, descargas, likes, fecha e insignia «Restringido» si `gated != "open"`. Al hacer clic se abre la ficha (T13).
  - [ ] T12.4 Estados vacíos («Sin resultados») y de error (mensaje de CA10, con un botón «Reintentar»).
- **Tests unitarios (Vitest + Testing Library, con la API falsa):**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `explorar_renders_results` | la API devuelve 2 modelos, uno `manual` | 2 tarjetas; 1 insignia «Restringido» |
  | `explorar_sort_change_replaces_results` | buscar y cambiar a «Likes» | 2.ª llamada con `sort: "likes"`; la lista solo tiene los nuevos |
  | `explorar_load_more_appends` | 1.ª página con `next`, 2.ª sin él | 4 tarjetas; el botón «Cargar más» desaparece |
  | `explorar_load_more_no_double_request` | doble clic rápido en «Cargar más» | 1 sola llamada a `searchNext` |
  | `explorar_empty_state` | la API devuelve `[]` | texto «Sin resultados» |
  | `explorar_shows_error` | la API lanza `rateLimited` con 30 | texto «reintenta en 30 s» y botón «Reintentar» |
  | `explorar_enter_triggers_search` | escribir «qwen» y pulsar Enter | 1 llamada con `query: "qwen"` |

### T13 — UI: ficha del modelo con variantes, estimación y botón Descargar
- **Ficheros/módulos:** `ui/src/lib/hf/ModelDetail.svelte`, `ui/src/lib/hf/VariantRow.svelte` y sus tests
- **Depende de:** T12
- **Cubre:** CA4, CA5, CA6, CA11
- **Subtareas:**
  - [ ] T13.1 Cabecera: id, licencia, arquitectura, contexto máximo e insignia «Restringido» con el texto «Requiere token de HF y aceptar las condiciones en la web del modelo».
  - [ ] T13.2 Tabla de variantes: cuantización (o «desconocida»), tamaño, «N partes» si hay más de una, «incompleta» si `!complete` y una estimación «≈ X GiB (orientativa)» o «≈ X GiB (sin caché KV)», con la leyenda de la fórmula y el contexto de 4096 tokens.
  - [ ] T13.3 Botón «Descargar» por variante: deshabilitado, con `title="Disponible próximamente"`, y oculto en las incompletas. Recibe la prop `onDownload?: (req: DownloadRequest) => void` para que el plan 006 la conecte.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `ficha_renders_variants` | detalle con 3 variantes | 3 filas en el orden recibido, con cuantización y tamaño formateado |
  | `ficha_unknown_quant_label` | variante con `quant: null` | texto «desconocida» |
  | `ficha_split_shows_parts` | variante de 2 partes | texto «2 partes» |
  | `ficha_incomplete_variant` | `complete: false` | texto «incompleta» y sin botón «Descargar» |
  | `ficha_estimate_orientative` | `kvIncluded: true`, `bytes 5_993_741_824` | «≈ 5,6 GiB (orientativa)» |
  | `ficha_estimate_without_kv` | `kvIncluded: false` | «(sin caché KV)» |
  | `ficha_download_button_disabled` | variante completa | botón deshabilitado con `title` «Disponible próximamente» |
  | `ficha_gated_notice` | `gated: "manual"` | aviso de token y condiciones |
  | `ficha_error_not_found` | la API lanza `notFound` | «Modelo no encontrado» |

### T14 — UI: diálogo del token de Hugging Face
- **Ficheros/módulos:** `ui/src/lib/hf/TokenDialog.svelte`, `ui/src/lib/hf/TokenStatusBadge.svelte` (en la cabecera de Explorar) y sus tests
- **Depende de:** T11, T12
- **Cubre:** CA7, CA8
- **Subtareas:**
  - [ ] T14.1 Al montar Explorar se llama a `tokenStatus()`. La insignia muestra «Conectado como <user>», «Token no válido» o «Sin token».
  - [ ] T14.2 Diálogo con un campo `type="password"` y los botones Guardar y Borrar. Con formato inválido muestra el error sin invocar el comando (la misma regex que en T8, replicada para dar feedback inmediato; la autoridad sigue siendo el backend). Tras guardar se vacía el campo. El token nunca se guarda en el estado global ni en `localStorage`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `token_dialog_save_valid` | `hf_` + 34 caracteres; la API devuelve `{name:"ana"}` | `tokenSave` llamado una vez; insignia «Conectado como ana»; campo vacío |
  | `token_dialog_invalid_format_no_call` | `abc` | mensaje de formato; `tokenSave` no llamado |
  | `token_dialog_unauthorized` | la API lanza `unauthorized` | «Token no válido»; insignia sin cambios |
  | `token_dialog_delete` | token presente; Borrar | `tokenDelete` llamado; insignia «Sin token» |
  | `token_badge_revoked` | `tokenStatus` → `{present:true,user:null,invalid:true}` | «Token no válido» |
  | `token_not_persisted_in_local_storage` | guardar un token válido | `localStorage` sin claves nuevas |

### Verificación manual (usuario, al final del plan)

No forma parte del gate. Toca la red real y el llavero del sistema, así que no puede ser un test unitario (R3).
1. Buscar «qwen gguf» por Descargas y luego por Likes, y cargar 2 páginas más.
2. Abrir `unsloth/Qwen3.8-27B-GGUF` (o un repo similar con splits): la variante BF16 sale como una sola fila de 2 partes y no aparecen `mmproj` ni `imatrix`.
3. Guardar un token real: aparece «Conectado como …» y se comprueba la entrada en el Administrador de credenciales (Windows) o en Acceso a Llaveros (macOS). Borrarlo: la entrada desaparece.

## Fuera de alcance

- Descarga, progreso, pausa/reanudación, cancelación, verificación SHA256 y espacio en disco: **plan 006**. Aquí solo se definen `DownloadRequest` y el botón deshabilitado.
- Comprobar si el usuario tiene concedido el acceso a un repo gated concreto (se detectará con un 403 al descargar, en el plan 006).
- Archivos `mmproj` (multimodal, fuera del MVP) e `imatrix`.
- El formato antiguo de split `*.gguf.partXofY`.
- Selector de contexto para recalcular la estimación (los perfiles con `ctx-size` llegan en el plan de ejecución) e indicador «cabe/no cabe» comparando con la RAM/VRAM detectada por el plan 004.
- Filtros adicionales (autor, tarea, tamaño), orden por `trendingScore` o fecha, caché de respuestas e inicio de sesión OAuth en HF.
- Lectura de la tarjeta del modelo (README) y de los metadatos GGUF del archivo (plan de la Biblioteca).

## Riesgos

| Riesgo | Mitigación |
|---|---|
| HF cambia campos o el formato de `gated`/`expand` | Los DTO toleran campos desconocidos y opcionales; un valor imprevisto da `Parse` con un mensaje claro y no un pánico. Las fixtures llevan la fecha de captura en un comentario del test. |
| El esquema de `/api/whoami-v2` no está documentado en el OpenAPI | Solo se exige `name`; el resto es opcional. La verificación manual confirma la forma real. |
| `keyring` 4.x reorganizó sus stores en crates aparte (`windows-native-keyring-store`, `apple-native-keyring-store`) | El trait `SecretStore` aísla la dependencia. El developer fija la versión y los features tras consultar docs.rs. Los tests usan `MemoryStore`. |
| Fuga del token (logs, errores, otro host al seguir `Link`) | `HfToken` con `Debug`/`Display` ocultados, la cabecera solo hacia la URL base, `parse_next_link` y `ForeignUrl` como doble barrera, el token nunca vuelve a la UI y hay tests específicos. |
| Límite de peticiones de HF (429) | Sin reintentos automáticos. Se muestra `Retry-After` y la paginación es bajo demanda («Cargar más»). |
| Repos con miles de archivos o un `Link` cíclico | Tope de 20 páginas por árbol (`TooManyPages`). |
| La estimación de memoria se aleja de la real (MoE, GQA agresiva, caché KV cuantizada) | Se etiqueta como «orientativa», la fórmula aparece en la UI y el coeficiente es una constante única fácil de recalibrar. |
| Falsos positivos o negativos al extraer la cuantización | Gramática cerrada, recorrido de derecha a izquierda, búsqueda también en la carpeta y «desconocida» como reserva; los casos reales están en la tabla de T2. |
| Conflicto de rutas o de mock HTTP con los planes 003/004 escritos en paralelo | Las rutas se declaran como supuesto y el mock HTTP se alinea con el que fije el 004 (ver Preguntas abiertas). |

## Preguntas abiertas

1. **Ubicación del módulo:** ¿el plan 003 crea `hf` como módulo de `crates/core` (`crates/core/src/hf/`, supuesto de este plan) o como crate propio `crates/core/hf`? Solo cambian las rutas.
2. **Servidor mock HTTP:** ¿qué crate fija el plan 004 para los tests HTTP (`wiremock`, `httpmock`, `mockito`)? Este plan propone `wiremock` si el 004 no fija ninguno.
3. **Ubicación del diálogo del token:** ¿existe en el 003 una pantalla Ajustes donde deba vivir el token? Este plan lo pone en la cabecera de Explorar; el componente es reutilizable.
4. **Calibración de la estimación:** ¿le vale al usuario la constante de 16 384 B/token por cada mil millones de parámetros y el overhead de 512 MiB con contexto de 4096, o prefiere otros valores por defecto?
5. **Tamaño de página:** se propone `limit=20`. ¿Prefiere el usuario otro valor?
