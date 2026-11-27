# 005 — Explorador de Hugging Face (búsqueda, ficha de modelo y token)

- **Tipo:** feature
- **Estado:** aprobado
- **Rama:** feat/005-explorador-hf
- **Fase del roadmap:** 2. Hugging Face ([docs/MVP.md](../MVP.md), funcionalidad B sin descargas)

## Objetivo

Que el usuario encuentre desde la app modelos GGUF de Hugging Face, ordenados por descargas o por likes, y vea en su ficha cada cuantización disponible con su tamaño real (los modelos divididos cuentan como un solo archivo) y una estimación orientativa de la memoria que necesita, comparada con el hardware de su equipo: cada cuantización indica si «Cabe en GPU», «Cabe en RAM» o «No cabe», y se destaca la mejor que cabe. En la búsqueda, cada resultado lleva una insignia orientativa calculada con lo que ya publica HF, sin peticiones extra. También puede guardar su token de HF de forma segura para acceder a modelos privados o restringidos (gated).

## Contexto

### Punto de partida

- Los **planes 003** (esqueleto) y **004** (runtime de llama.cpp) están hechos y mergeados en `main`. Estado real del repo que condiciona este plan:
  - Un único crate `app-llama-core` en `crates/core`, organizado por módulos. `crates/core/src/hf/mod.rs` ya existe (solo un comentario) y está declarado en `crates/core/src/lib.rs`. Toda la lógica de este plan vive en `crates/core/src/hf/`.
  - `crates/core/Cargo.toml` ya depende de `serde`, `serde_json`, `thiserror`, `reqwest` (0.13, features `json` + `stream`; TLS por defecto) y `tokio`, y tiene `wiremock` 0.6 y `tempfile` como dev-dependencies, todas heredadas de `[workspace.dependencies]` del `Cargo.toml` raíz. No hay crate `url`: se usa `reqwest::Url` (reexporta `url::Url`). Faltan `keyring`, que se añade al workspace en T8, y `sysinfo` (aprobada por el usuario el 2026-11-27: sin features por defecto, solo memoria), que se añade en T15 (`regex` no se añade: ver T2).
  - `src-tauri/src/commands.rs` declara los submódulos (`pub mod runtime;`) y `src-tauri/src/commands/runtime.rs` define el error serializable `ErrorPayload { code: &'static str, message: String }`, con `code` en PascalCase igual al nombre de la variante (`"RateLimited"`, `"Cancelled"`…). `src-tauri/src/lib.rs` registra los comandos en `register()` y crea el estado en `setup` (`app.manage(...)`). Este plan **reutiliza `ErrorPayload`** (lo mueve a `commands/error.rs`, T10) en lugar de crear otro tipo.
  - Convención de serialización del 004: los tipos de `core` derivan `Serialize` y se devuelven tal cual (campos en `snake_case`, enums con la forma por defecto de serde) y un test de `src-tauri` fija el JSON exacto para que los tipos TS (`ui/src/lib/runtime/types.ts`) no diverjan. Este plan sigue la misma convención (no `camelCase`).
  - UI: `ui/src/views/Explore.svelte` (marcador «Próximamente», ya registrado en `ui/src/lib/navigation.ts` como `explore`/«Explorar») y `ui/src/views/Settings.svelte` (sección «llama.cpp» con `RuntimeSettings`, test en `Settings.test.ts`; navegación `settings`/«Ajustes»). Patrón de acceso a comandos: `ui/src/lib/api.ts` y `ui/src/lib/runtime/api.ts` importan `invoke` de `@tauri-apps/api/core` y devuelven la promesa sin transformar el rechazo; los tests usan `mockIPC`/`clearMocks` de `@tauri-apps/api/mocks` (no un `invoke` inyectado). El mensaje de error se extrae con `errorMessage(unknown)` de `ui/src/lib/runtime/view-model.ts`.
  - **Detección de hardware del plan 004** (`crates/core/src/runtime/hardware.rs`), que este plan reutiliza sin duplicarla: `detect(&dyn SystemProbe) -> HardwareInfo { os, arch, avx2, avx512, gpus: Vec<GpuInfo { name, vram_mib: Option<u64> }>, driver_cuda, vulkan_available, metal }`, expuesta como `RuntimeManager::hardware()` (en `spawn_blocking`, porque ejecuta `nvidia-smi`) y como el comando `runtime_hardware`. En los tests, el hardware se simula con `FakeProbe` (`crates/core/src/runtime/test_support.rs`) o se construye `HardwareInfo` como literal (como en `select.rs`). Lo que hay y lo que falta:
    - **VRAM:** solo de GPUs NVIDIA en Windows (`nvidia-smi --query-gpu=name,memory.total`); `vram_mib` es `None` si `nvidia-smi` no da una cifra. En macOS `gpus` está vacío.
    - **Metal:** `metal == true` en macOS aarch64 (Apple Silicon, memoria unificada). Sin cifra de VRAM.
    - **Vulkan:** solo `vulkan_available` (existe `vulkan-1.dll`), sin GPU ni VRAM asociadas.
    - **RAM: no se detecta.** Este plan la añade (T15) como `HardwareInfo.ram_bytes: Option<u64>` (RAM física total) a través de un método nuevo de `SystemProbe`, para que siga siendo simulable.
- Decisiones ya tomadas: API REST directa de HF (sin el crate `hf-hub`), `reqwest` + `tokio`, token en el llavero del sistema con el crate `keyring` (R6). `core` no depende de Tauri (R7). Mock HTTP de los tests: `wiremock`. Tamaño de página `limit=20`. Unidades en GiB. La gestión del token vive en **Ajustes**, no en Explorar.
- Decisiones del usuario del 2026-11-27: (1) los `.gguf` cuyo nombre base empieza por `mtp` (sin distinguir mayúsculas) se excluyen igual que `mmproj` e `imatrix`; (2) se acepta la dependencia `sysinfo` (sin features por defecto, solo memoria) para leer la RAM total en T15.
- La descarga real es el **plan 006**. Este plan solo deja el tipo `DownloadRequest` y el botón «Descargar» como punto de integración, deshabilitado.
- Lecciones de la memoria (`docs/memory/long/` y `docs/memory/medium/004-runtime-llamacpp.md`) aplicadas aquí:
  - **aserto-debil:** los tests de rechazo de la UI comprueban la identidad o el contenido exacto del error (`rejects.toBe(error)`, `toEqual`), nunca `rejects.toBeDefined()`.
  - **refactor-equivalente:** los envoltorios de `invoke` propagan el rechazo sin transformarlo; la traducción a texto se hace en el view-model.
  - **reserva-sin-test:** toda rama de UI (estados de carga, «Cargar más» sin doble petición, reseteos) lleva test de componente con `mockIPC`; no se da por «difícil de probar».
  - **dato-inventado / fixture-distinto-del-plan:** las fixtures se recortan de respuestas reales y coinciden con las de las tablas de tests de este plan.
  - **csp-estricta:** el JSON que recibe la UI se fija en un test de Rust.

### API de Hugging Face verificada (2026-11-26; búsqueda con `gguf` verificada el 2026-11-27)

La referencia oficial está en el [OpenAPI del Hub](https://huggingface.co/.well-known/openapi.md) ([página índice](https://huggingface.co/docs/hub/api)). Además se han hecho llamadas reales a la API. Las fixtures de los tests se recortan de esas respuestas.

| Endpoint | Uso en este plan | Forma observada |
|---|---|---|
| `GET /api/models?search=<q>&filter=gguf&sort=<downloads\|likes>&direction=-1&limit=20&expand[]=downloads&expand[]=likes&expand[]=gated&expand[]=lastModified&expand[]=pipeline_tag&expand[]=gguf` | Búsqueda | Array de objetos `{ _id, id, downloads, likes, gated, lastModified, pipeline_tag?, gguf? }`. Con `expand[]` solo vuelven los campos pedidos más `_id` e `id`. Puede faltar `pipeline_tag`. `gated` es `false`, `"auto"` o `"manual"`. **`expand[]=gguf` funciona en el listado (verificado el 2026-11-27)** y devuelve el mismo objeto que la ficha: `{ total, architecture, context_length, chat_template, bos_token, eos_token, totalFileSize, quantize_imatrix_file? }`. Ejemplo real (`search=llama`, orden por descargas): `bartowski/Meta-Llama-3.1-8B-Instruct-GGUF` con `total: 8030261312`, `context_length: 131072` y `totalFileSize: 32128885888`. `chat_template` ocupa unos 4 KB por resultado. |
| Paginación | «Cargar más» | Cabecera `Link: <URL completa con cursor=…>; rel="next"`. Sin `rel="next"` no hay más páginas. El OpenAPI describe así el cursor en otros listados ("Pagination cursor from the `Link` header"), y una página puede venir vacía. |
| `GET /api/models/{owner}/{name}?expand[]=gguf&expand[]=gated&expand[]=downloads&expand[]=likes&expand[]=lastModified&expand[]=cardData` | Ficha | `gguf` es opcional: `{ total, architecture, context_length, chat_template, bos_token, eos_token, totalFileSize, quantize_imatrix_file? }`. `total` es el número de parámetros (p. ej. `27320697856`). `cardData` puede incluir `license` y `base_model`. |
| `GET /api/models/{owner}/{name}/tree/main?recursive=true` | Lista de archivos | Array de `{ type: "file"\|"directory", oid, size, path, lfs?: { oid, size, pointerSize }, xetHash? }`. **Sin `recursive` no aparecen los archivos de las subcarpetas**: los splits pueden ir en carpetas (`BF16/Qwen3.8-27B-BF16-00001-of-00002.gguf`). `lfs.oid` es el SHA256. Paginado con `Link` (1000 entradas por defecto). En repos gated el árbol se lista sin token. |
| `GET /api/whoami-v2` con `Authorization: Bearer <token>` | Validar el token | El OpenAPI dice "Get information about the user and auth method used", sin esquema. Por `huggingface_hub` se sabe que devuelve `{ type, name, fullname?, orgs?, auth: { type, accessToken?: { displayName, role } } }` y `401` si el token no vale. Solo se usa `name` (obligatorio) y `auth.accessToken.role` (opcional). |

Hechos observados que condicionan el diseño:
- Un repo GGUF incluye archivos `.gguf` que **no** son modelos de texto: `mmproj-*.gguf` (multimodal, fuera del MVP), `imatrix*.gguf` y los borradores MTP para decodificación especulativa (`MTP/mtp-Qwen3.8-27B-Q4_0.gguf`, 1 369 590 656 B, en el árbol real de `unsloth/Qwen3.8-27B-GGUF` del 2026-11-27). Se excluyen. Un MTP no sirve solo y, si no se excluyera, podría salir «Recomendada»: con una RTX 4070 ninguna cuantización real del 27B cabe en la GPU (la menor, `UD-IQ1_S` de 6 192 222 208 B, necesita 10 396 012 544 B), pero el MTP sí (5 573 381 248 B).
- Nombres de cuantización reales: `Q4_0`, `Q8_0`, `UD-Q4_K_XL`, `UD-IQ2_XXS`, `Q6_K`, `BF16`, `MXFP4`, `llama-2-7b.Q4_K_M.gguf` (con punto), en minúsculas (`q4_k_m`)… A veces la cuantización está en la carpeta y no en el archivo.
- Formato de los splits: `<prefijo>-00001-of-00002.gguf`, con 5 dígitos.
- **`gguf.totalFileSize` no sirve para la insignia de la búsqueda.** No es la suma de los `.gguf` del repo ni una cuantización fija: es el tamaño de **un único archivo** que HF toma como representativo, y cuál toca varía de un repo a otro. Comprobado el 2026-11-27: en `TheBloke/Llama-2-7B-GGUF` vale `2825940672`, el tamaño exacto de `llama-2-7b.Q2_K.gguf` (de 12 cuantizaciones entre 2,8 y 7,2 GB); en `bartowski/Meta-Llama-3.1-8B-Instruct-GGUF` vale `32128885888`, que con `total = 8030261312` son 4 bytes por parámetro (el archivo f32); en `bartowski/Llama-3.3-70B-Instruct-GGUF` vale `16751201248` (≈ 1,9 bits por parámetro). Por eso la insignia se calcula a partir de `gguf.total` con la heurística de la sección «Encaje en el hardware».

### Diseño

1. **`crates/core/src/hf/`**, sin Tauri:
   - `model.rs`: tipos de dominio y DTO serde (`ModelSummary`, `ModelInfo`, `TreeEntry`, `Gated`, `GgufInfo`, `WhoAmI`).
   - `quant.rs`: `extract_quant(path) -> Option<String>`.
   - `files.rs`: `group_gguf_files(entries) -> Vec<GgufVariant>` (filtra, agrupa splits y calcula tamaños).
   - `memory.rs`: `estimation_ctx(context_length) -> u32` y `estimate_memory(file_bytes, params_total, ctx) -> MemoryEstimate`.
   - `fit.rs`: `Fit::{Gpu, Ram, NoFit}`, `MemoryBudget` (memoria utilizable calculada a partir de `HardwareInfo`), `fit(required, &budget) -> Option<Fit>` y `recommended(...)` (la mejor variante que cabe).
   - `hint.rs`: `typical_quant_bytes(params)` y `fit_hint(gguf, &budget) -> Option<FitHint>` (insignia orientativa de la búsqueda).
   - `budget_cache.rs`: `BudgetCache`, que detecta el hardware una sola vez por sesión.
   - `url.rs`: `RepoId` validado, construcción de URLs, `parse_next_link`.
   - `client.rs`: `HfClient` (reqwest) con la URL base inyectable (`https://huggingface.co` en producción, servidor mock en los tests) y el token opcional.
   - `token.rs`: `HfToken` (formato y `Debug` ocultado), trait `SecretStore`, `KeyringStore` y `MemoryStore` (fake de los tests).
   - `service.rs`: `HfService` combina el cliente y el `SecretStore`. Es la única API que usan los comandos.
   - `error.rs`: `HfError` (`thiserror`).
2. **`src-tauri/src/commands/hf.rs`** (declarado en `src-tauri/src/commands.rs`): comandos finos que delegan en `HfService` y convierten `HfError` en el `ErrorPayload { code, message }` existente, movido a `src-tauri/src/commands/error.rs` para compartirlo con `runtime.rs`. **El token nunca vuelve a la UI.** El hardware lo detecta el `RuntimeManager` del plan 004 (vía `RuntimeAppState`) y el backend calcula el encaje: la UI recibe `fit`, `recommended`, `hint` y `budget` ya resueltos en los DTO.
3. **UI:**
   - `ui/src/lib/hf/`: `types.ts` (espejo del JSON de Rust), `api.ts` (envoltorios de `invoke`, mismo patrón que `ui/src/lib/runtime/api.ts`), `view-model.ts` (formato), `explore-state.svelte.ts` (estado con runas) y los componentes `ModelCard.svelte`, `ModelDetail.svelte`, `VariantRow.svelte` y `TokenSettings.svelte`.
   - `ui/src/views/Explore.svelte`: sustituye el marcador «Próximamente» por la pantalla Explorar (lista + ficha).
   - `ui/src/views/Settings.svelte`: añade la sección «Hugging Face» con `TokenSettings`.
   - `ui/src/lib/errors.ts`: `ErrorPayload` y `errorMessage`, movidos desde `ui/src/lib/runtime/` para compartirlos.
4. **`crates/core/src/runtime/hardware.rs`** (T15): `SystemProbe::total_memory_bytes()` y `HardwareInfo.ram_bytes`. Es la única detección nueva; el resto del hardware del plan 004 se reutiliza tal cual.

### Fórmula de la estimación de memoria (orientativa)

```
memoria = bytes_archivo + KV(ctx) + OVERHEAD
KV(ctx) = ctx × (params_total × 16_384 / 1_000_000_000)   [bytes; aritmética entera u128; la división del
                                                            paréntesis se trunca primero (B/token) y
                                                            después se multiplica por ctx]
OVERHEAD = 512 MiB (536_870_912 bytes)
ctx = min(gguf.context_length, 8192)   si el repo publica context_length > 0
ctx = 4096                             si no lo publica o vale 0
```

- **Decisión sobre el contexto (2026-11-27):** el usuario aceptó como contexto por defecto general `min(contexto del modelo, 8192)`, que es el que usarán los perfiles de ejecución. La estimación usa la misma regla para que la cifra de la ficha se corresponda con lo que se ejecutará sin tocar nada. Si HF no publica `gguf.context_length`, se cae a 4096 (valor conservador conocido para cualquier modelo actual). El `ctx` usado se devuelve en la ficha (`ModelDetail.ctx`) y la UI lo muestra.
- El coeficiente `16_384 B/token por cada mil millones de parámetros` reproduce la caché KV f16 de Llama-3-8B (32 capas × 8 cabezas KV × 128 × 2 (K y V) × 2 bytes = 131 072 B/token). Para modelos con otra atención solo es una aproximación, y por eso la UI la etiqueta como **orientativa**.
- Ejemplos de referencia (8 000 000 000 parámetros → 131 072 B/token): con ctx 4096, KV = 536 870 912 B (512 MiB); con ctx 8192, KV = 1 073 741 824 B (1 GiB).
- Si el repo no publica `gguf.total`, se omite `KV` y el resultado lleva `kv_included = false`. La UI muestra entonces «sin caché KV».
- La misma cifra vale para RAM (solo CPU) o VRAM (todas las capas en la GPU). La UI muestra una sola cifra con el texto «RAM o VRAM aprox. con contexto de <ctx> tokens».

### Encaje en el hardware (cabe / no cabe), orientativo

**Memoria utilizable (`MemoryBudget`)**, calculada una vez por sesión a partir de `HardwareInfo`:

```
SAFETY_MARGIN_PERCENT = 20                      [constante única; usable_percent = 100 − 20 = 80]
utilizable(total) = total × 80 / 100            [u128, división truncada, saturación a u64::MAX]
ram_bytes  = utilizable(HardwareInfo.ram_bytes)                 si la RAM se conoce; si no, None
gpu_bytes  = utilizable(max(vram_mib) × 1_048_576)              GPUs con vram_mib = Some (NVIDIA)
gpu_bytes  = ram_bytes y unified = true                         si metal (Apple Silicon)
gpu_bytes  = None                                               sin cifra de VRAM (Vulkan sin NVIDIA,
                                                                 GPU con vram_mib = None, macOS x86_64)
```

- **RAM total menos un margen, no la RAM disponible.** La disponible cambia con cada pestaña del navegador: la misma ficha diría «cabe» o «no cabe» según el momento, y el resultado no se podría reproducir. Con la total, la etiqueta es estable durante la sesión y describe el equipo, no el instante. El margen del 20 % reserva el sistema operativo y las apps habituales (unos 3,2 GiB en un equipo de 16 GiB) y, en la VRAM, el escritorio y los búferes de cómputo de llama.cpp que la `OVERHEAD` de 512 MiB no cubre del todo. Es una sola constante, aplicada igual a la RAM y a la VRAM, para que sea fácil de explicar («80 % del total») y de recalibrar.
- **Varias GPUs:** se usa la de **más VRAM**, no la suma. Repartir capas entre GPUs distintas depende de la configuración de ejecución, que llega en otro plan; así nunca se sobrestima.
- **Memoria unificada (Metal):** RAM y VRAM son la misma memoria, así que `gpu_bytes = ram_bytes` y en un Mac nunca sale «Cabe en RAM»: o cabe en la GPU o no cabe.
- **Sin VRAM conocida**, solo se evalúa la RAM («Cabe en RAM» o «No cabe»). **Sin RAM conocida** (la sonda no la lee), solo se puede afirmar «Cabe en GPU»; en otro caso no hay etiqueta. Sin ninguna de las dos, no hay etiquetas.
- El encaje describe el **hardware**, no el runtime instalado: una GPU NVIDIA cuenta aunque el usuario haya instalado el build de CPU.

**Regla de encaje** (`fit(required, &budget) -> Option<Fit>`), con `required = MemoryEstimate.bytes` y comparaciones `≤`:

1. `gpu_bytes = Some(g)` y `required ≤ g` → `Some(Gpu)`.
2. Si no, `ram_bytes = Some(r)` y `required ≤ r` → `Some(Ram)`.
3. Si no, `ram_bytes = Some(_)` → `Some(NoFit)`.
4. Si no (RAM desconocida) → `None` (sin etiqueta).

Las variantes **incompletas** no se evalúan (`fit = None`): su `total_bytes` solo suma las partes presentes.

**Mejor que cabe** (`recommended`), determinista: entre las variantes **completas** con `fit = Some(Gpu)`, la de mayor `total_bytes`; si no hay ninguna, lo mismo entre las de `Some(Ram)`; si tampoco, ninguna. Si empatan en `total_bytes`, gana la de `name` menor. La de mayor tamaño que cabe es la de mayor calidad (más bits por peso) que el equipo puede mover, y una que cabe en la GPU siempre gana a una mayor que solo cabe en la RAM, porque la diferencia de velocidad es de un orden de magnitud. Como mucho una variante sale con `recommended = true`.

**Insignia orientativa de la búsqueda** (`fit_hint`), sin peticiones extra, con el `gguf` que ya trae el listado:

```
TYPICAL_QUANT_MILLIBYTES_PER_PARAM = 606                 [Q4_K_M ≈ 4,85 bits por parámetro]
typical_quant_bytes(total) = total × 606 / 1000          [u128, división truncada]
estimate = estimate_memory(typical_quant_bytes(total), Some(total), estimation_ctx(gguf.context_length))
hint     = { estimate, fit: fit(estimate.bytes, &budget) }   si gguf.total = Some(n) con n > 0
hint     = None (sin insignia)                                si falta gguf o gguf.total, o vale 0
```

- Q4_K_M es la cuantización que la mayoría de repos publica y la que se suele recomendar por defecto, así que la insignia responde a «¿podría usar este modelo con una cuantización normal?». La constante está calibrada con un archivo real: `TheBloke/Llama-2-7B-GGUF` tiene `total = 6 738 415 616` y `llama-2-7b.Q4_K_M.gguf` pesa 4 081 004 224 B; la fórmula da 4 083 479 863 B (error del 0,06 %).
- Es **orientativa** de forma explícita: la UI antepone «≈» y el `title` dice que está estimada para una cuantización Q4_K_M típica. La ficha, con los tamaños reales, es la referencia.

## Criterios de aceptación

- **CA1 — Búsqueda filtrada por GGUF.** Dado el texto «qwen» y el orden «Descargas», cuando el usuario busca, entonces se hace una única petición `GET /api/models` con `search=qwen`, `filter=gguf`, `sort=downloads`, `direction=-1`, `limit=20` y los 6 `expand[]` de la tabla (incluido `expand[]=gguf`). Cada resultado muestra el id, las descargas, los likes, la fecha de última modificación, una insignia «Restringido» si `gated` ≠ `false` y la insignia orientativa de encaje de CA17. Lo verifican `search_sends_expected_query` y `explorar_renders_results`.
- **CA2 — Orden por likes.** Dado el orden «Likes», cuando el usuario busca o cambia el orden, entonces la petición lleva `sort=likes&direction=-1` y la lista se sustituye (no se acumula). Lo verifican `search_url_sort_likes` y `explorar_sort_change_replaces_results`.
- **CA3 — Paginación.** Dada una respuesta con `Link: <…cursor=abc>; rel="next"`, cuando el usuario pulsa «Cargar más», entonces se pide exactamente esa URL y los nuevos resultados se añaden al final. Sin `rel="next"` el botón desaparece. Si la URL de `next` apunta a otro host o esquema distinto de la URL base, se ignora (no hay más páginas y nunca se envía el token a otro host). Lo verifican `parse_next_link_*`, `search_follows_next_cursor` y `explorar_load_more_appends`.
- **CA4 — Ficha con archivos GGUF.** Dado un repo con el árbol de la fixture `tree_unsloth.json`, cuando el usuario abre la ficha, entonces se pide `tree/main?recursive=true` (siguiendo la paginación `Link`) y se listan solo los `.gguf` de tipo `file`, sin `mmproj*`, `*imatrix*` ni los archivos cuyo nombre base empieza por `mtp` (sin distinguir mayúsculas; p. ej. `MTP/mtp-Qwen3.8-27B-Q4_0.gguf`). Cada variante muestra su cuantización (o «desconocida»), su tamaño en GiB con un decimal y su número de partes. Las variantes van ordenadas por tamaño total ascendente y, si empatan, por ruta. Lo verifican `group_excludes_mmproj_and_imatrix`, `group_excludes_mtp`, `group_keeps_mtp_not_at_start`, `model_detail_assembles_variants` y `ficha_renders_variants`.
- **CA5 — Modelos divididos.** Dados `BF16/X-BF16-00001-of-00002.gguf` (49 986 159 616 B) y `BF16/X-BF16-00002-of-00002.gguf` (4 671 576 000 B), cuando se agrupan, entonces se obtiene **una** variante `BF16` con 2 partes ordenadas, `total_bytes = 54 657 735 616` y `complete = true`. Si falta alguna parte de `1..=M`, la variante sale con `complete = false` y la UI la marca «incompleta» sin botón de descarga. Lo verifican `group_split_two_parts` y `group_split_missing_part_incomplete`.
- **CA6 — Estimación de memoria.** Dados `bytes_archivo = 4 920 000 000`, `params_total = 8 000 000 000` y `ctx = 4096`, cuando se estima, entonces `bytes = 5 993 741 824` y `kv_included = true`; con `ctx = 8192`, `bytes = 6 530 612 736`. Sin `params_total`: `bytes = 5 456 870 912` y `kv_included = false`. El contexto de la estimación es `min(gguf.context_length, 8192)` (`40960 → 8192`, `2048 → 2048`) y 4096 si falta o vale 0. En la ficha de un modelo con `context_length ≥ 8192` y esos datos, la UI muestra «≈ 6,1 GiB (orientativa)» y la leyenda «con contexto de 8192 tokens». Lo verifican `estimate_with_params`, `estimate_scales_with_ctx_8192_example`, `estimate_without_params`, `estimation_ctx_*`, `model_detail_assembles_variants` y `ficha_estimate_orientative`.
- **CA7 — Guardar el token.** Dado un token con formato válido (`hf_` seguido de 30 o más caracteres `[A-Za-z0-9]`, tras recortar los espacios), cuando el usuario lo guarda, entonces se llama a `/api/whoami-v2` con `Authorization: Bearer <token>`. Si responde 200, el token se guarda en el llavero (servicio `app-llama`, cuenta `huggingface-token`) y la sección «Hugging Face» de Ajustes muestra «Conectado como <name>». Si responde 401, **no** se guarda y la UI muestra «Token no válido». Con un formato inválido no se hace ninguna petición. Lo verifican `save_token_valid_stores`, `save_token_unauthorized_not_stored`, `save_token_bad_format_no_request` y `token_settings_*`.
- **CA8 — Borrar y consultar el token.** Dado un token guardado, cuando el usuario lo borra, entonces desaparece del llavero, `hf_token_status` devuelve `{ present: false }` y las peticiones siguientes no llevan `Authorization`. Borrar sin token guardado no es un error. Al abrir **Ajustes**, la sección «Hugging Face» llama a `hf_token_status`, que devuelve `{ present: true, user, invalid: false }` validando con whoami (o `{ present: true, user: null, invalid: true }` si responde 401). La gestión del token (guardar, borrar, estado) solo está en Ajustes; Explorar no la muestra. Lo verifican `delete_token_*`, `status_*`, `token_settings_*` y `ajustes_incluye_seccion_hugging_face`.
- **CA9 — Uso del token.** Dado un token guardado, cuando se busca o se abre una ficha, entonces las peticiones a la URL base llevan `Authorization: Bearer <token>`. Sin token no llevan la cabecera. El token no aparece en ningún `Debug`, log, error ni respuesta de comando. Lo verifican `client_sends_bearer_when_token`, `client_no_auth_header_without_token`, `token_debug_is_redacted` y `error_payload_hf_never_contains_token`.
- **CA10 — Errores comprensibles.** Dada una respuesta 429 con `Retry-After: 30`, cuando se busca, entonces la UI muestra «Hugging Face limita las peticiones; reintenta en 30 s». Con 404 en la ficha: «Modelo no encontrado». Con 401/403 en la ficha: «Acceso denegado: revisa tu token». Con un JSON inesperado: «Respuesta inesperada de Hugging Face». Con un fallo de red: «Sin conexión con Hugging Face». Los textos los genera el backend en `ErrorPayload.message` y la UI los muestra tal cual. Ningún error tumba la pantalla. Lo verifican `client_maps_status_*`, `error_payload_hf_*` y `explorar_shows_error`.
- **CA11 — Punto de integración de la descarga.** Dada una variante completa, cuando se muestra la ficha, entonces aparece un botón «Descargar» **deshabilitado**, con el título «Disponible próximamente», y existe `GgufVariant::download_request(&RepoId) -> DownloadRequest { repo_id: String, revision: "main", parts: [{ path, size, sha256: Option<String> }] }`, con las partes en orden y `sha256` tomado de `lfs.oid`. Lo verifican `download_request_from_split_variant` y `ficha_download_button_disabled`.
- **CA12 — Validación del repo id.** Dado un id que no cumple `^[A-Za-z0-9][A-Za-z0-9._-]{0,95}/[A-Za-z0-9][A-Za-z0-9._-]{0,95}$` o que contiene `..`, cuando se pide la ficha, entonces se devuelve `HfError::InvalidRepoId` sin hacer ninguna petición. Lo verifican `repo_id_*`.
- **CA13 — Calidad.** `node scripts/quality-gate.mjs` en verde en Windows y macOS. Ningún test usa la red real (HTTP con `wiremock`), el reloj real, el llavero real del sistema ni el hardware real (el hardware llega como `HardwareInfo`/`MemoryBudget` literal o con `FakeProbe`). Los tests existentes de `runtime` (Rust y UI) siguen en verde tras mover `ErrorPayload` y `errorMessage` y tras añadir `ram_bytes` a `HardwareInfo`.
- **CA14 — RAM total en la detección de hardware.** Dada una `FakeProbe` que informa `total_memory_bytes = Some(34 359 738 368)` (32 GiB), cuando se detecta el hardware, entonces `HardwareInfo.ram_bytes == Some(34 359 738 368)`; si la sonda devuelve `None`, `ram_bytes == None` sin error. El resto de campos de `HardwareInfo` no cambia y el JSON de `runtime_hardware` añade `"ram_bytes":34359738368` al final del objeto. Lo verifican `detect_reports_ram_from_probe`, `detect_ram_unknown`, `detect_windows_nvidia_vulkan` (actualizado) y `json_hardware_status_y_update_para_la_ui` (actualizado).
- **CA15 — Memoria utilizable.** Dado `HardwareInfo` con una RTX 4070 (`vram_mib = 12282`) y 32 GiB de RAM, cuando se calcula el `MemoryBudget`, entonces `gpu_bytes = Some(10 302 888 345)` (12 878 610 432 × 80 / 100, truncado), `ram_bytes = Some(27 487 790 694)`, `unified = false` y `usable_percent = 80`. Con dos GPUs de 12 282 y 15 360 MiB se usa la mayor: `gpu_bytes = Some(12 884 901 888)`. En un Mac Apple Silicon (`metal = true`) con 16 GiB: `gpu_bytes = ram_bytes = Some(13 743 895 347)` y `unified = true`. Sin GPUs con cifra de VRAM (Vulkan sin NVIDIA, `vram_mib = None`, macOS x86_64): `gpu_bytes = None`. El hardware se detecta una sola vez por sesión; si la detección falla, `MemoryBudget` desconocido (todo `None`) sin cachearlo, y la UI no muestra etiquetas ni insignias. Lo verifican `budget_*` y `budget_cache_*`.
- **CA16 — Cabe / no cabe en la ficha y mejor que cabe.** Dados `model_info_gguf.json` (`total = 27 320 697 856`, `context_length ≥ 8192`, así que `ctx = 8192` y KV + OVERHEAD = 3 666 919 424 + 536 870 912 = 4 203 790 336 B) y `tree_unsloth.json`, con un `MemoryBudget` de una RTX 4090 (`vram_mib = 24564` → `gpu_bytes = 20 605 776 691`) y 32 GiB de RAM (`ram_bytes = 27 487 790 694`), cuando el usuario abre la ficha, entonces `UD-IQ2_XXS` (estimación 11 409 833 984 B) y `Q4_0` (20 260 269 024 B) salen con «Cabe en GPU», `UD-Q4_K_XL` (21 863 197 600 B) con «Cabe en RAM» y `BF16` (58 861 525 952 B) con «No cabe»; `Q4_0` es la única marcada «Recomendada» (la mayor que cabe en la GPU, aunque `UD-Q4_K_XL` sea mayor). Estas etiquetas abrevian el `name` completo (`Qwen3.8-27B-UD-IQ2_XXS`, `Qwen3.8-27B-Q4_0`, `Qwen3.8-27B-UD-Q4_K_XL`, `BF16/Qwen3.8-27B-BF16`); sus `quant` son `IQ2_XXS`, `Q4_0`, `Q4_K_XL` y `BF16` (véase «Nomenclatura de variantes» en T3). Las variantes incompletas no llevan etiqueta ni se recomiendan. El archivo `MTP/mtp-Qwen3.8-27B-Q4_0.gguf` de la fixture no aparece (CA4), así que nunca se recomienda. Si ninguna cabe, ninguna se marca y la ficha dice «Ninguna cuantización cabe en la memoria de este equipo». Bajo la tabla, una leyenda indica con qué memoria se compara: «Comparado con 19,2 GiB de VRAM y 25,6 GiB de RAM utilizables (80 % del total)», «Memoria unificada: comparado con 12,8 GiB utilizables (80 % del total)», «VRAM desconocida: solo se compara con la RAM (25,6 GiB utilizables, 80 % del total)», «Comparado con 9,6 GiB de VRAM utilizables (80 % del total); RAM desconocida» o «No se pudo detectar la memoria del equipo». Lo verifican `fit_*`, `recommend_*`, `model_detail_fit_and_recommended`, `budget_legend_*`, `ficha_fit_labels`, `ficha_recommended_highlight` y `ficha_no_recommended`.
- **CA17 — Insignia orientativa en la búsqueda, sin peticiones extra.** Dado un resultado con `gguf.total = 8 030 261 312` y `gguf.context_length = 131 072` (`bartowski/Meta-Llama-3.1-8B-Instruct-GGUF`) y el `MemoryBudget` de la RTX 4070 con 32 GiB de CA15, cuando se busca, entonces el resultado lleva `hint.estimate = { bytes: 6 481 006 131, kv_included: true }` (Q4_K_M típica 4 866 338 355 + KV(8192) 1 077 796 864 + 536 870 912) y `hint.fit = "Gpu"`, y la tarjeta muestra la insignia «≈ Cabe en GPU» con el `title` «Orientativa: estimada para una cuantización Q4_K_M típica (≈ 6,0 GiB)». Con `gguf.total = 70 553 706 560` (Llama-3.3-70B): `bytes = 52 761 967 679`, «≈ No cabe» con 32 GiB y «≈ Cabe en RAM» con 64 GiB (`ram_bytes = 54 975 581 388`). Sin `gguf`, sin `gguf.total` o con `total = 0`: `hint = null` y sin insignia. Una búsqueda de 3 resultados hace **exactamente 1** petición HTTP. Lo verifican `typical_quant_bytes_*`, `fit_hint_*`, `search_hints_without_extra_requests`, `json_search_page_para_la_ui` y `explorar_renders_fit_badges`.

## Tareas

> Orden recomendado: T1 → T2 → T3 → T4 → T15 → T16 → T17 → T5 → T6 → T7 → T8 → T9 → T10 → T11 → T12 → T13 → T14. T2, T4 y T5 son independientes entre sí. T15 (RAM en `runtime`) no depende de nada de este plan y puede ir la primera. T15–T17 se documentan tras T4, en su orden de implementación, aunque sus IDs se añadieron después. T14 solo depende de T11, así que puede ir antes que T12/T13 si conviene.

### T1 — Tipos de dominio y parseo de respuestas de HF con fixtures
- **Ficheros/módulos:** `crates/core/src/hf/mod.rs` (ya existe; se sustituye el comentario por la declaración de submódulos), `crates/core/src/hf/model.rs`, `crates/core/src/hf/error.rs`, `crates/core/fixtures/hf/{search_page.json,model_info_gguf.json,model_info_no_gguf.json,model_info_gated_manual.json,tree_unsloth.json,tree_flat.json,whoami.json}`. `crates/core/Cargo.toml` no cambia (`serde`, `serde_json` y `thiserror` ya están).
- **Depende de:** — (planes 003 y 004 hechos)
- **Cubre:** CA1, CA4, CA7, CA17 (parseo)
- **Subtareas:**
  - [x] T1.1 Fixtures recortadas de respuestas reales, sin `chat_template`. `search_page.json` es la captura del 2026-11-27 de `search=llama&filter=gguf&sort=downloads&direction=-1&limit=3` con los 6 `expand[]` de la tabla: `bartowski/Meta-Llama-3.1-8B-Instruct-GGUF` (`downloads: 371263`, `likes: 437`, `gguf.total: 8030261312`, `gguf.context_length: 131072`), `bartowski/Llama-3.2-1B-Instruct-GGUF` (`gguf.total: 1235814432`) y `bartowski/Llama-3.3-70B-Instruct-GGUF` (`gguf.total: 70553706560`), con su `totalFileSize` y sin `chat_template`. `tree_unsloth.json` (de `unsloth/Qwen3.8-27B-GGUF`) lleva los directorios `BF16` y `MTP`, `.gitattributes`, `README.md`, 3 `.gguf` simples (`Qwen3.8-27B-UD-IQ2_XXS.gguf` de 7 206 043 648 B, `Qwen3.8-27B-Q4_0.gguf` de 16 056 478 688 B y `Qwen3.8-27B-UD-Q4_K_XL.gguf` de 17 659 407 264 B), las 2 partes BF16 de CA5 en `BF16/`, `MTP/mtp-Qwen3.8-27B-Q4_0.gguf` (1 369 590 656 B, con `lfs`), `mmproj-F16.gguf` e `imatrix_unsloth.gguf`. `model_info_gguf.json` (del mismo repo) conserva `gguf.total = 27320697856` y su `gguf.context_length` real (≥ 8192).
  - [x] T1.2 `Gated` (`Open` | `Auto` | `Manual`) con un deserializador que acepta `false`, `"auto"` y `"manual"`. Cualquier otro valor es un error de parseo.
  - [x] T1.3 `ModelSummary { id, downloads, likes, gated, last_modified: Option<String>, pipeline_tag: Option<String>, gguf: Option<GgufInfo> }` (el `gguf` del listado tiene la misma forma que el de la ficha), `ModelInfo { id, gated, downloads, likes, gguf: Option<GgufInfo>, license: Option<String> }`, `GgufInfo { total: Option<u64>, architecture: Option<String>, context_length: Option<u64> }`, `TreeEntry { kind: File|Directory, path, size, lfs: Option<LfsInfo { oid, size }> }` y `WhoAmI { name, role: Option<String> }`. Se ignoran los campos desconocidos. Cuando la forma de HF no coincide con la del tipo de dominio (p. ej. `cardData.license`, `auth.accessToken.role`), se parsea a un DTO crudo privado y se convierte. Los tipos que llegan a la UI (`ModelSummary`, `ModelInfo`, `GgufInfo`, `Gated`, `WhoAmI`) derivan además `Serialize` con la forma por defecto de serde (campos `snake_case`, `Gated` como `"Open"`/`"Auto"`/`"Manual"`), igual que los tipos de `runtime`; el JSON exacto lo fija T10.
  - [x] T1.4 `HfError` con las variantes `Network`, `Unauthorized`, `Forbidden`, `NotFound`, `RateLimited { retry_after_secs: Option<u64> }`, `Status(u16)`, `Parse(String)`, `InvalidRepoId`, `InvalidToken`, `TooManyPages`, `ForeignUrl` y `Secret(String)`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `parse_search_page` | `search_page.json` | 3 `ModelSummary`; el primero con `id == "bartowski/Meta-Llama-3.1-8B-Instruct-GGUF"`, `downloads == 371263`, `likes == 437`, `gguf.total == Some(8030261312)` y `gguf.context_length == Some(131072)` |
  | `parse_search_missing_pipeline_tag` | elemento sin `pipeline_tag` | `pipeline_tag == None`, sin error |
  | `parse_search_item_without_gguf` | elemento de `search_page.json` sin la clave `gguf` (JSON en línea) | `gguf == None`, sin error |
  | `parse_search_gguf_without_total` | elemento con `"gguf": { "architecture": "llama" }` | `gguf.total == None`, sin error |
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
- **Ficheros/módulos:** `crates/core/src/hf/quant.rs`. **Sin el crate `regex`:** las expresiones regulares de este plan (T2, T3, T5, T8) son la especificación, pero se implementan con funciones de reconocimiento escritas a mano sobre `&str`/bytes ASCII. Compilar una regex exigiría `expect()` fuera de los tests, que R7 prohíbe, y las gramáticas son cortas y cerradas.
- **Depende de:** T1 (solo por el módulo `hf`)
- **Cubre:** CA4
- **Subtareas:**
  - [x] T2.1 Se quita la extensión `.gguf` y el sufijo de split `-NNNNN-of-NNNNN`. El nombre se trocea por `-` y `.` y se recorren los trozos **de derecha a izquierda**. El primero que encaja (sin distinguir mayúsculas) con `^(?:I?Q[1-8](?:_[0-9A-Z]{1,3}){0,3}|TQ[12]_0|BF16|F16|F32|MXFP4(?:_MOE)?)$` es la cuantización, normalizada a mayúsculas.
  - [x] T2.2 Si el nombre del archivo no tiene ninguna, se prueban los componentes de la carpeta de derecha a izquierda (con `/` como separador lógico de las rutas de HF, que no son rutas del sistema).
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
  - [x] T3.1 Filtro: `kind == File`, extensión `.gguf` sin distinguir mayúsculas y nombre base (el último componente de la ruta, tras el último `/`) que no empiece por `mmproj` ni por `mtp` ni contenga `imatrix` (las tres comprobaciones sin distinguir mayúsculas). `mtp` excluye los borradores para decodificación especulativa (decisión del usuario del 2026-11-27).
  - [x] T3.2 Split: `^(?P<prefix>.+)-(?P<idx>\d{5})-of-(?P<total>\d{5})\.gguf$` (sin distinguir mayúsculas en la extensión). Clave de grupo: `(prefix con carpeta, total)`. Un `idx == 0`, `idx > total` o `total == 0` se trata como archivo suelto.
  - [x] T3.3 `GgufVariant { name, quant: Option<String>, parts: Vec<GgufPart { path, size, sha256: Option<String> }>, total_bytes, complete }`. `size` es `lfs.size` si existe y, si no, `size`. Las partes van ordenadas por índice. `complete` es `true` si están todas de `1..=total` sin duplicados. Un archivo suelto es una variante de 1 parte con `complete = true`. `name` es la ruta sin el sufijo de split ni la extensión.
  - [x] T3.4 Orden de las variantes: `total_bytes` ascendente y, si empatan, `name`.
  - [x] T3.5 `GgufVariant::download_request(&RepoId) -> DownloadRequest` (con `revision = "main"`). El tipo `DownloadRequest` se define aquí para que lo consuma el plan 006. **Firma provisional:** como `RepoId` no existe hasta T5, se implementó como `download_request(&self, repo_id: &str)` con `DownloadRequest.repo_id: String`; la firma definitiva con `&RepoId` llega en T5.5.
- **Nomenclatura de variantes (vale para T3, T7, T13 y CA4/CA5/CA16):** `GgufVariant.name` es la ruta sin sufijo de split ni extensión y `quant` es la cuantización extraída por T2. En `tree_unsloth.json`: `name` = `Qwen3.8-27B-UD-IQ2_XXS`, `Qwen3.8-27B-Q4_0`, `Qwen3.8-27B-UD-Q4_K_XL` y `BF16/Qwen3.8-27B-BF16`; `quant` = `Some("IQ2_XXS")`, `Some("Q4_0")`, `Some("Q4_K_XL")` y `Some("BF16")`. Cuando un CA o un test cita una variante por una etiqueta corta (`UD-IQ2_XXS`, `Q4_0`, `UD-Q4_K_XL`, `BF16`), se refiere a la variante con ese `name` completo (o, lo que es equivalente en esa fixture, con ese `quant`, sin el prefijo `UD-`); la UI muestra `quant`, no `name`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `group_single_file` | 1 entrada `a-Q4_0.gguf` de 100 B | 1 variante, 1 parte, `total_bytes == 100`, `complete`, `quant == Some("Q4_0")` |
  | `group_split_two_parts` | las 2 partes BF16 de CA5, en orden inverso | 1 variante con `quant == Some("BF16")` y `name` = la carpeta y el prefijo sin sufijo de split (`BF16/Qwen3.8-27B-BF16` con las partes reales de `tree_unsloth.json`), partes ordenadas 1 y 2, `total_bytes == 54_657_735_616`, `complete` |
  | `group_split_missing_part_incomplete` | partes 1 y 3 de `-of-00003` | 1 variante, `complete == false`, `total_bytes` = suma de las presentes |
  | `group_split_duplicate_part_incomplete` | parte 1 dos veces de `-of-00002` | `complete == false` |
  | `group_split_index_zero_is_single` | `x-00000-of-00002.gguf` | variante suelta de 1 parte |
  | `group_split_index_above_total_is_single` | `x-00003-of-00002.gguf` | variante suelta de 1 parte |
  | `group_excludes_mmproj_and_imatrix` | `tree_unsloth.json` | ninguna variante contiene `mmproj` ni `imatrix` |
  | `group_excludes_mtp` | `tree_unsloth.json`; y, aparte, las entradas en línea `MTP/mtp-Qwen3.8-27B-Q4_0.gguf`, `MTP-x-Q8_0.gguf` y `drafts/Mtp_y.gguf` | con la fixture: exactamente 4 variantes (`name` = `Qwen3.8-27B-UD-IQ2_XXS`, `Qwen3.8-27B-Q4_0`, `Qwen3.8-27B-UD-Q4_K_XL`, `BF16/Qwen3.8-27B-BF16`; `quant` = `IQ2_XXS`, `Q4_0`, `Q4_K_XL`, `BF16`) y ninguna con ruta bajo `MTP/`; con las entradas en línea: lista vacía |
  | `group_keeps_mtp_not_at_start` | `Qwen3-MTP-Q4_0.gguf` (el nombre base no empieza por `mtp`) | 1 variante, `quant == Some("Q4_0")` |
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
  - [x] T4.1 Constantes `FALLBACK_CTX = 4096`, `MAX_ESTIMATION_CTX = 8192`, `OVERHEAD_BYTES = 536_870_912` y `KV_BYTES_PER_TOKEN_PER_BILLION = 16_384`.
  - [x] T4.2 `estimation_ctx(context_length: Option<u64>) -> u32`: `min(context_length, MAX_ESTIMATION_CTX)` si es `Some(n)` con `n > 0`; `FALLBACK_CTX` si es `None` o `Some(0)`.
  - [x] T4.3 `estimate_memory(file_bytes: u64, params_total: Option<u64>, ctx: u32) -> MemoryEstimate { bytes: u64, kv_included: bool }`, con aritmética `u128` y saturación a `u64::MAX`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `estimation_ctx_caps_at_8192` | `Some(40_960)` | `8192` |
  | `estimation_ctx_exact_8192` | `Some(8192)` | `8192` |
  | `estimation_ctx_small_model` | `Some(2048)` | `2048` |
  | `estimation_ctx_missing_falls_back` | `None` | `4096` |
  | `estimation_ctx_zero_falls_back` | `Some(0)` | `4096` |
  | `estimation_ctx_huge_value` | `Some(u64::MAX)` | `8192`, sin pánico |
  | `estimate_with_params` | `4_920_000_000`, `Some(8_000_000_000)`, `4096` | `bytes == 5_993_741_824`, `kv_included` |
  | `estimate_scales_with_ctx_8192_example` | `4_920_000_000`, `Some(8_000_000_000)`, `8192` | `bytes == 6_530_612_736`, `kv_included` |
  | `estimate_without_params` | `4_920_000_000`, `None`, `4096` | `bytes == 5_456_870_912`, `!kv_included` |
  | `estimate_ctx_zero` | `1000`, `Some(8_000_000_000)`, `0` | `bytes == 1000 + 536_870_912`, `kv_included` |
  | `estimate_kv_truncates` | `0`, `Some(1)`, `1` | `bytes == 536_870_912` (KV trunca a 0) |
  | `estimate_scales_with_ctx` | `0`, `Some(8_000_000_000)`, `8192` | `bytes == 536_870_912 + 1_073_741_824` |
  | `estimate_saturates` | `u64::MAX`, `Some(u64::MAX)`, `u32::MAX` | `bytes == u64::MAX`, sin pánico |
  | `estimate_kv_truncates_per_token` | `0`, `Some(8_030_261_312)`, `8192` | `bytes == 1_077_796_864 + 536_870_912` (131 567 B/token × 8192; no 1 077 803 428, que saldría truncando al final) |

### T15 — RAM total en la detección de hardware (`runtime`)
- **Ficheros/módulos:** `crates/core/src/runtime/hardware.rs` (`SystemProbe::total_memory_bytes`, `HardwareInfo.ram_bytes`, implementación en `NativeProbe`), `crates/core/src/runtime/test_support.rs` (`FakeProbe::with_ram`), `crates/core/src/runtime/select.rs` (solo los literales `HardwareInfo` de sus tests: `ram_bytes: None`), `Cargo.toml` raíz (`sysinfo` en `[workspace.dependencies]` con `default-features = false` y solo el feature de sistema/memoria), `crates/core/Cargo.toml` (`sysinfo.workspace = true`), `Cargo.lock`, `src-tauri/src/commands/runtime.rs` (solo el test `json_hardware_status_y_update_para_la_ui`), `ui/src/lib/runtime/types.ts` (`ram_bytes: number | null`) y los literales `HardwareInfo` de `ui/src/lib/runtime/RuntimeSettings.test.ts`, `ui/src/lib/runtime/view-model.test.ts` y `ui/src/views/Settings.test.ts` (añaden `ram_bytes`).
- **Depende de:** — (plan 004 hecho)
- **Cubre:** CA14, CA13
- **Subtareas:**
  - [ ] T15.1 `SystemProbe::total_memory_bytes(&self) -> Option<u64>` (RAM física total en bytes). `detect` la copia en `HardwareInfo.ram_bytes: Option<u64>`, el último campo del struct (así el JSON solo crece por el final). No usa `run`: en `detect_macos_arm64` sigue habiendo 0 ejecuciones.
  - [ ] T15.2 `NativeProbe` la lee con el crate `sysinfo` (solo memoria, sin refrescar procesos ni discos; en Windows equivale a `GlobalMemoryStatusEx`, en macOS a `hw.memsize`). Cualquier fallo da `None`, sin pánico. Se descarta lanzar un programa: en Windows no hay uno rápido y fiable (`wmic` ya no viene de serie y PowerShell tarda en arrancar). La dependencia `sysinfo` está **aprobada** por el usuario (2026-11-27), sin features por defecto y solo con la de memoria. El developer fija la versión tras consultar docs.rs (0.40.x el 2026-11-27) y no usa `unsafe`. **Sin test unitario propio** (toca el hardware real); `native_probe_no_falla` ya ejecuta `detect(&NativeProbe)` y sigue sin comprobar valores.
  - [ ] T15.3 `FakeProbe::with_ram(bytes)`; por defecto `None`.
  - [ ] T15.4 Espejo TS: `HardwareInfo.ram_bytes: number | null` en `ui/src/lib/runtime/types.ts`. `describeHardware` no cambia (mostrar la RAM en Ajustes queda fuera de alcance).
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `detect_reports_ram_from_probe` | `windows_probe().with_ram(34_359_738_368)` | `ram_bytes == Some(34_359_738_368)` |
  | `detect_ram_unknown` | `windows_probe()` sin `with_ram` | `ram_bytes == None`, sin pánico |
  | `detect_macos_arm64` (ampliado) | la sonda actual + `with_ram(17_179_869_184)` | `ram_bytes == Some(17_179_869_184)`, `metal`, `run_count() == 0` |
  | `detect_windows_nvidia_vulkan` (actualizado) | la sonda actual + `with_ram(34_359_738_368)` | el literal esperado incluye `ram_bytes: Some(34_359_738_368)` |
  | `json_hardware_status_y_update_para_la_ui` (actualizado) | el `HardwareInfo` actual con `ram_bytes: Some(34_359_738_368)` | el JSON actual termina en `…,"vulkan_available":true,"metal":false,"ram_bytes":34359738368}` |
  | tests existentes de `select.rs`, `manager.rs` y de la UI de `runtime` y `Settings` | los mismos, con `ram_bytes` añadido a los literales | siguen en verde; `npm --prefix ui run check` sin errores |

### T16 — Encaje en memoria y mejor variante (`fit.rs`)
- **Ficheros/módulos:** `crates/core/src/hf/fit.rs`. Usa `crate::runtime::HardwareInfo` (mismo crate; `core` sigue sin depender de Tauri).
- **Depende de:** T3 (`GgufVariant`), T4 (`MemoryEstimate`), T15 (`HardwareInfo.ram_bytes`)
- **Cubre:** CA15, CA16
- **Subtareas:**
  - [ ] T16.1 `Fit` (`Gpu` | `Ram` | `NoFit`), con `Serialize` por defecto (`"Gpu"`, `"Ram"`, `"NoFit"`).
  - [ ] T16.2 `SAFETY_MARGIN_PERCENT: u64 = 20` y `MemoryBudget { gpu_bytes: Option<u64>, ram_bytes: Option<u64>, unified: bool, usable_percent: u8 }` (`Clone`, `Copy`, `Serialize`; `usable_percent = 100 − SAFETY_MARGIN_PERCENT`, para que la UI no repita la constante). `MemoryBudget::from_hardware(&HardwareInfo)` y `MemoryBudget::unknown()` según las reglas de «Encaje en el hardware». Aritmética `u128` con saturación.
  - [ ] T16.3 `fit(required: u64, budget: &MemoryBudget) -> Option<Fit>` con los 4 pasos de la regla.
  - [ ] T16.4 `recommended(candidates: &[(&GgufVariant, Option<Fit>)]) -> Option<usize>`: índice de la mejor variante según la regla «Mejor que cabe» (solo completas; `Gpu` antes que `Ram`; mayor `total_bytes`; empate por `name` menor).
- **Tests unitarios** (`HardwareInfo` construido como literal, sin sonda ni hardware real):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `budget_single_nvidia_gpu` | `gpus: [RTX 4070, Some(12282)]`, `ram_bytes: Some(34_359_738_368)` | `gpu_bytes == Some(10_302_888_345)`, `ram_bytes == Some(27_487_790_694)`, `!unified`, `usable_percent == 80` |
  | `budget_uses_largest_gpu` | `gpus: [Some(12282), Some(15360)]` | `gpu_bytes == Some(12_884_901_888)` |
  | `budget_gpu_without_vram_figure_ignored` | `gpus: [Tesla X, None]`, RAM 32 GiB | `gpu_bytes == None`, `ram_bytes == Some(27_487_790_694)` |
  | `budget_metal_unified` | `metal: true`, `gpus: []`, `ram_bytes: Some(17_179_869_184)` | `gpu_bytes == ram_bytes == Some(13_743_895_347)`, `unified` |
  | `budget_metal_without_ram` | `metal: true`, `ram_bytes: None` | `gpu_bytes == None`, `ram_bytes == None`, `unified` |
  | `budget_vulkan_only_ram` | `vulkan_available: true`, `gpus: []`, RAM 16 GiB | `gpu_bytes == None`, `ram_bytes == Some(13_743_895_347)` |
  | `budget_ram_unknown` | RTX 4070, `ram_bytes: None` | `gpu_bytes == Some(10_302_888_345)`, `ram_bytes == None` |
  | `budget_saturates` | `vram_mib: Some(u64::MAX)`, `ram_bytes: Some(u64::MAX)` | `gpu_bytes == Some(u64::MAX)`, `ram_bytes == Some(14_757_395_258_967_641_292)` (u64::MAX × 80 / 100), sin pánico |
  | `budget_unknown` | `MemoryBudget::unknown()` | `gpu_bytes == None`, `ram_bytes == None`, `!unified`, `usable_percent == 80` |
  | `fit_gpu_when_fits_vram` | `6_530_612_736`, presupuesto RTX 4070 + 32 GiB | `Some(Gpu)` |
  | `fit_boundary_equal_is_gpu` | `10_302_888_345`, mismo presupuesto | `Some(Gpu)` |
  | `fit_one_byte_over_vram_is_ram` | `10_302_888_346`, mismo presupuesto | `Some(Ram)` |
  | `fit_ram_boundary` | `27_487_790_694` y `27_487_790_695`, mismo presupuesto | `Some(Ram)` y `Some(NoFit)` |
  | `fit_only_ram_known` | `6_530_612_736`, `gpu_bytes: None`, `ram_bytes: Some(13_743_895_347)` | `Some(Ram)` |
  | `fit_unified_never_ram` | `6_530_612_736` y `15_000_000_000`, presupuesto Mac 16 GiB | `Some(Gpu)` y `Some(NoFit)` |
  | `fit_ram_unknown_over_gpu_is_none` | `15_000_000_000`, `gpu_bytes: Some(10_302_888_345)`, `ram_bytes: None` | `None` |
  | `fit_ram_unknown_within_gpu` | `6_530_612_736`, mismo presupuesto | `Some(Gpu)` |
  | `fit_unknown_budget` | cualquier valor, `MemoryBudget::unknown()` | `None` |
  | `recommend_largest_gpu_fit` | `a` 4e9 `Gpu`, `b` 8e9 `Gpu`, `c` 16e9 `Ram` (completas) | `Some(1)` |
  | `recommend_gpu_beats_larger_ram` | `a` 4e9 `Gpu`, `b` 16e9 `Ram` | `Some(0)` |
  | `recommend_falls_back_to_ram` | `a` 4e9 `Ram`, `b` 8e9 `Ram`, `c` 16e9 `NoFit` | `Some(1)` |
  | `recommend_none_when_nothing_fits` | todas `NoFit` | `None` |
  | `recommend_skips_incomplete` | `a` 4e9 `Gpu` completa, `b` 8e9 `Gpu` con `complete = false` | `Some(0)` |
  | `recommend_tie_smallest_name` | `b` 8e9 `Gpu`, `a` 8e9 `Gpu` (en ese orden) | `Some(1)` (la de `name == "a"`) |
  | `recommend_ignores_unknown_fit` | todas con `None` | `None` |
  | `recommend_empty` | `[]` | `None` |

### T17 — Insignia orientativa de la búsqueda y caché del hardware
- **Ficheros/módulos:** `crates/core/src/hf/hint.rs` y `crates/core/src/hf/budget_cache.rs`. `tokio` ya tiene el feature `sync` (`tokio::sync::OnceCell`).
- **Depende de:** T1 (`GgufInfo`), T4, T16
- **Cubre:** CA15 (caché), CA17
- **Subtareas:**
  - [ ] T17.1 `TYPICAL_QUANT_MILLIBYTES_PER_PARAM: u64 = 606` y `typical_quant_bytes(params_total: u64) -> u64` (`u128`, división truncada).
  - [ ] T17.2 `FitHint { estimate: MemoryEstimate, fit: Option<Fit> }` (`Serialize`) y `fit_hint(gguf: Option<&GgufInfo>, budget: &MemoryBudget) -> Option<FitHint>` según la sección «Insignia orientativa de la búsqueda». Reutiliza `estimation_ctx`, `estimate_memory` y `fit`, sin fórmulas propias.
  - [ ] T17.3 `BudgetCache` (envuelve `tokio::sync::OnceCell<MemoryBudget>`) con `async fn get_or_detect<F, Fut, E>(&self, detect: F) -> MemoryBudget` donde `F: FnOnce() -> Fut` y `Fut: Future<Output = Result<HardwareInfo, E>>`. Con `Ok` calcula `MemoryBudget::from_hardware` y lo guarda; con `Err` devuelve `MemoryBudget::unknown()` **sin** guardarlo, para reintentar en la siguiente petición. Las llamadas concurrentes detectan una sola vez.
- **Tests unitarios** (sin hardware real; la detección es un cierre que cuenta sus llamadas con un `AtomicUsize`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `typical_quant_bytes_llama2_7b` | `6_738_415_616` | `4_083_479_863` (Q4_K_M real: 4 081 004 224) |
  | `typical_quant_bytes_8b` | `8_030_261_312` | `4_866_338_355` |
  | `typical_quant_bytes_zero` | `0` | `0` |
  | `typical_quant_bytes_max_no_panic` | `u64::MAX` | `11_178_726_908_667_988_278`, sin pánico |
  | `fit_hint_llama31_8b_gpu` | `gguf { total: Some(8_030_261_312), context_length: Some(131_072) }`, RTX 4070 + 32 GiB | `estimate == { bytes: 6_481_006_131, kv_included: true }`, `fit == Some(Gpu)` |
  | `fit_hint_llama33_70b_no_fit_32gib` | `total: Some(70_553_706_560)`, `context_length: Some(131_072)`, RTX 4070 + 32 GiB | `bytes == 52_761_967_679`, `fit == Some(NoFit)` |
  | `fit_hint_llama33_70b_ram_64gib` | el mismo `gguf`; `gpu_bytes: Some(10_302_888_345)`, `ram_bytes: Some(54_975_581_388)` | `fit == Some(Ram)` |
  | `fit_hint_missing_context_uses_fallback` | `total: Some(8_000_000_000)`, `context_length: None` | `bytes == 5_921_741_824` (4 848 000 000 + 536 870 912 + 536 870 912) |
  | `fit_hint_unknown_budget` | gguf del 8B, `MemoryBudget::unknown()` | `Some(FitHint)` con `bytes == 6_481_006_131` y `fit == None` |
  | `fit_hint_without_gguf` | `None` | `None` |
  | `fit_hint_without_total` | `gguf { total: None, .. }` | `None` |
  | `fit_hint_total_zero` | `gguf { total: Some(0), .. }` | `None` |
  | `budget_cache_detects_once` | dos `get_or_detect` con un cierre que devuelve `Ok(RTX 4070 + 32 GiB)` | el cierre se llama 1 vez; los dos resultados con `gpu_bytes == Some(10_302_888_345)` |
  | `budget_cache_error_not_cached` | 1.º cierre `Err(())`; 2.º `Ok(RTX 4070 + 32 GiB)` | 1.º `MemoryBudget::unknown()`; 2.º con `gpu_bytes == Some(10_302_888_345)`; 2 llamadas en total |
  | `budget_cache_concurrent_detects_once` | dos `get_or_detect` lanzados a la vez con `tokio::join!` y un cierre que cede (`tokio::task::yield_now`) antes de responder | 1 llamada al cierre; los dos resultados iguales |

### T5 — Repo id, construcción de URLs y paginación `Link`
- **Ficheros/módulos:** `crates/core/src/hf/url.rs` y, solo para T5.5, `crates/core/src/hf/files.rs`. Se usa `reqwest::Url` (sin añadir el crate `url`). `SortBy` deriva `Deserialize` con `rename_all = "lowercase"` (`"downloads"`/`"likes"`), como `BackendPreference` en `runtime`.
- **Depende de:** T1 (T5.5 también de T3, ya hecha)
- **Cubre:** CA1, CA2, CA3, CA12
- **Subtareas:**
  - [ ] T5.1 `RepoId::parse(&str) -> Result<RepoId, HfError>` según la regex de CA12, rechazando `..`.
  - [ ] T5.2 `SearchParams { query: String, sort: SortBy::{Downloads, Likes}, limit: u32 }` → `search_url(base, &params)`, con la query codificada en porcentaje y los `expand[]` de la tabla del Contexto. Una query vacía o solo con espacios omite `search`.
  - [ ] T5.3 `model_info_url(base, &RepoId)` y `tree_url(base, &RepoId)` (con `recursive=true`). El `/` del repo id no se codifica.
  - [ ] T5.4 `parse_next_link(header: &str, base: &Url) -> Option<Url>`: extrae la URL con `rel="next"` de entre varias separadas por comas y solo la devuelve si su esquema, host y puerto coinciden con `base`.
  - [ ] T5.5 Firma definitiva de `download_request` (seguimiento de la revisión de T3; toca también `crates/core/src/hf/files.rs`): se añade `RepoId::as_str(&self) -> &str` y `GgufVariant::download_request(&self, repo_id: &str)` pasa a `download_request(&self, repo: &RepoId)`. **Decisión:** `DownloadRequest.repo_id` sigue siendo `String` (serializable sin depender de cómo se serialice `RepoId`), rellenado con `repo.as_str().to_owned()`. Se ajustan los tests de T3 `download_request_from_split_variant` (construye el repo con `RepoId::parse("unsloth/Qwen3.8-27B-GGUF").unwrap()` y sigue comprobando `request.repo_id == "unsloth/Qwen3.8-27B-GGUF"`) y `download_request_non_lfs_has_no_sha` (con `RepoId::parse("a/b").unwrap()`); sus resultados esperados no cambian. No queda ninguna llamada con `&str`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `repo_id_valid` | `unsloth/Qwen3.8-27B-GGUF` | `Ok` |
  | `repo_id_missing_owner` | `Qwen3-8B` | `Err(InvalidRepoId)` |
  | `repo_id_three_segments` | `a/b/c` | `Err(InvalidRepoId)` |
  | `repo_id_dotdot` | `a/..` | `Err(InvalidRepoId)` |
  | `repo_id_query_injection` | `a/b?x=1` | `Err(InvalidRepoId)` |
  | `repo_id_empty` | `""` | `Err(InvalidRepoId)` |
  | `search_url_sort_downloads` | `query="qwen"`, `Downloads`, `20` | contiene `search=qwen`, `filter=gguf`, `sort=downloads`, `direction=-1`, `limit=20` y los 6 `expand[]` (incluido `expand[]=gguf`) |
  | `search_url_sort_likes` | `Likes` | contiene `sort=likes` |
  | `search_url_encodes_query` | `query="llama 3&x"` | `search=llama+3%26x` o `llama%203%26x`; no aparece un `&x` suelto |
  | `search_url_blank_query_omits_search` | `query="  "` | sin el parámetro `search` |
  | `tree_url_recursive` | `RepoId("a/b")` | termina en `/api/models/a/b/tree/main?recursive=true` |
  | `model_info_url_expands` | `RepoId("a/b")` | contiene `expand[]=gguf` y `expand[]=gated` |
  | `parse_next_link_simple` | `<https://huggingface.co/api/models?cursor=abc>; rel="next"` | `Some` con `cursor=abc` |
  | `parse_next_link_multiple_rels` | `<…p>; rel="prev", <…n>; rel="next"` | la URL `n` |
  | `parse_next_link_absent` | `<…>; rel="prev"` | `None` |
  | `parse_next_link_other_host_ignored` | `<https://evil.example/api>; rel="next"` | `None` |
  | `repo_id_as_str_roundtrip` | `RepoId::parse("unsloth/Qwen3.8-27B-GGUF").unwrap().as_str()` | `"unsloth/Qwen3.8-27B-GGUF"` |
  | `parse_next_link_other_scheme_ignored` | `<http://huggingface.co/…>; rel="next"` con base https | `None` |
  | `parse_next_link_malformed` | `garbage` | `None` |

### T6 — Cliente HTTP: búsqueda paginada y mapeo de errores
- **Ficheros/módulos:** `crates/core/src/hf/client.rs`. `crates/core/Cargo.toml` no cambia: `reqwest` (features `json` + `stream`, TLS por defecto de la 0.13), `tokio` y, como dev-dependencies, `wiremock` y `tokio` con `macros` + `rt-multi-thread` ya están. Los tests siguen el patrón de `crates/core/src/runtime/github.rs` (`wiremock::MockServer` y la URL base inyectada).
- **Depende de:** T1, T5
- **Cubre:** CA1, CA3, CA9, CA10
- **Subtareas:**
  - [ ] T6.1 `HfClient::new(http: reqwest::Client, base_url: Url, token: Option<HfToken>)`, `Clone` (el `reqwest::Client` se clona barato). El `reqwest::Client` lo construye quien llama (en producción, T10, con `User-Agent` `app-llama/<versión del crate>`, `connect_timeout` de 15 s y `timeout` de 30 s), igual que `GitHubReleaseSource::new(client, base)` en `runtime`. En los tests no se espera a que venza ningún timeout.
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

### T7 — Ficha del modelo (info + árbol recursivo paginado, con encaje) y búsqueda con insignias
- **Ficheros/módulos:** `crates/core/src/hf/client.rs`, `crates/core/src/hf/service.rs` (ficha y búsqueda)
- **Depende de:** T3, T4, T6, T16, T17
- **Cubre:** CA4, CA5, CA6, CA12, CA16, CA17
- **Subtareas:**
  - [ ] T7.1 `HfClient::model_info(&RepoId)` y `HfClient::tree(&RepoId) -> Vec<TreeEntry>`, que sigue `Link rel="next"` hasta un máximo de `MAX_TREE_PAGES = 20`. Si se supera: `Err(TooManyPages)`.
  - [ ] T7.2 `HfService::model_detail(repo: &str, budget: &MemoryBudget) -> ModelDetail { info: ModelInfo, ctx: u32, budget: MemoryBudget, variants: Vec<VariantView { variant, estimate: MemoryEstimate, fit: Option<Fit>, recommended: bool }> }`, con `ctx = estimation_ctx(info.gguf.context_length)` y `params_total = info.gguf.total`. `fit = fit(estimate.bytes, budget)` en las variantes completas y `None` en las incompletas; `recommended` es `true` solo en el índice que devuelve `recommended(...)`. Primero valida el `RepoId` (sin red si no vale). El hardware no se detecta aquí: llega como parámetro. `ModelDetail`, `VariantView`, `GgufVariant`, `GgufPart` y `MemoryEstimate` derivan `Serialize` (forma por defecto).
  - [ ] T7.3 `HfService::search(&SearchParams, budget: &MemoryBudget) -> SearchResults { items: Vec<SearchItem { model: ModelSummary, hint: Option<FitHint> }>, next: Option<Url> }` y `HfService::search_next(&Url, budget: &MemoryBudget) -> SearchResults`: delegan en `HfClient::search`/`search_next` y calculan `hint = fit_hint(model.gguf.as_ref(), budget)` por resultado, **sin ninguna petición adicional**. `SearchItem` y `FitHint` derivan `Serialize`.
- **Presupuestos de los tests** (literales de `MemoryBudget`): «RTX 4090 + 32 GiB» = `{ gpu_bytes: Some(20_605_776_691), ram_bytes: Some(27_487_790_694), unified: false, usable_percent: 80 }`; «RTX 4070 + 32 GiB» = `{ gpu_bytes: Some(10_302_888_345), ram_bytes: Some(27_487_790_694), .. }`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `tree_requests_recursive` | mock que exige `recursive=true` | `Ok` |
  | `tree_follows_pagination` | 2 páginas encadenadas con `Link` | entradas de las 2 páginas en orden |
  | `tree_too_many_pages` | mock cuyo `Link` apunta siempre a otra página (21 veces) | `Err(TooManyPages)` tras 20 peticiones |
  | `model_detail_assembles_variants` | `model_info_gguf.json` (con `gguf.context_length` ≥ 8192) + `tree_unsloth.json`, `MemoryBudget::unknown()` | 4 variantes sin `mmproj`/`imatrix`/`mtp` (el `MTP/mtp-Qwen3.8-27B-Q4_0.gguf` de la fixture no aparece) en el orden de `name` `Qwen3.8-27B-UD-IQ2_XXS`, `Qwen3.8-27B-Q4_0`, `Qwen3.8-27B-UD-Q4_K_XL`, `BF16/Qwen3.8-27B-BF16` (`quant` = `IQ2_XXS`, `Q4_0`, `Q4_K_XL`, `BF16`); BF16 agrupada en 2 partes; cada una con `kv_included == true`; `ctx == 8192`; todas con `fit == None` y ninguna `recommended` |
  | `model_detail_fit_and_recommended` | lo mismo con el presupuesto «RTX 4090 + 32 GiB» | `estimate.bytes` = `11_409_833_984`, `20_260_269_024`, `21_863_197_600`, `58_861_525_952`; `fit` = `Gpu`, `Gpu`, `Ram`, `NoFit`; solo la variante con `quant == Some("Q4_0")` con `recommended == true`; `detail.budget` igual al presupuesto recibido |
  | `model_detail_rtx4070_recommends_largest_ram` | lo mismo con «RTX 4070 + 32 GiB» | `fit` = `Ram`, `Ram`, `Ram`, `NoFit` (`11_409_833_984 > 10_302_888_345`); `recommended` en la variante con `name == "Qwen3.8-27B-UD-Q4_K_XL"` (`quant == Some("Q4_K_XL")`) |
  | `model_detail_incomplete_variant_no_fit` | árbol con solo la parte 1 de las 2 BF16 + `Q4_0`, «RTX 4090 + 32 GiB» | la BF16 con `complete == false`, `fit == None` y `recommended == false`; `Q4_0` recomendada |
  | `model_detail_without_gguf_metadata` | `model_info_no_gguf.json` + `tree_flat.json`, `MemoryBudget::unknown()` | variantes con `kv_included == false`; `ctx == 4096`; `fit == None` |
  | `model_detail_invalid_repo_no_request` | `"bad id"` | `Err(InvalidRepoId)`, 0 peticiones |
  | `model_detail_not_found` | mock 404 en info | `Err(NotFound)` |
  | `search_hints_without_extra_requests` | mock que responde `search_page.json` a `/api/models`; `HfService::search` con «RTX 4070 + 32 GiB» | **1** petición recibida por el mock en total; 3 `SearchItem`; el 8B con `hint = { estimate: { bytes: 6_481_006_131, kv_included: true }, fit: Some(Gpu) }`, el 1B con `bytes == 1_451_637_881` y `fit == Some(Gpu)`, el 70B con `bytes == 52_761_967_679` y `fit == Some(NoFit)` |
  | `search_item_without_gguf_has_no_hint` | página con un elemento sin `gguf` | ese `SearchItem` con `hint == None`; 1 petición |
  | `search_next_computes_hints` | `search_next` hacia una 2.ª página del mock con el elemento del 8B | `hint.fit == Some(Gpu)`; 1 petición por página |

### T8 — Token: tipo seguro y almacén de secretos
- **Ficheros/módulos:** `crates/core/src/hf/token.rs`, `Cargo.toml` raíz (`keyring` en `[workspace.dependencies]`, con los features o crates de store nativos de Windows y macOS), `crates/core/Cargo.toml` (`keyring.workspace = true`), `Cargo.lock`
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
- **Depende de:** T6, T7 (`HfService::search`), T8. En los tests de esta tarea, `search` recibe `MemoryBudget::unknown()`: aquí solo importa la cabecera `Authorization`.
- **Cubre:** CA7, CA8, CA9
- **Subtareas:**
  - [ ] T9.1 `HfClient::whoami(&HfToken) -> WhoAmI` contra `/api/whoami-v2`.
  - [ ] T9.2 `HfService::new(http: reqwest::Client, base_url: Url, store: Box<dyn SecretStore + Send + Sync>)`. Guarda el `HfClient` actual en un `std::sync::RwLock` y lo reconstruye con el token actual cada vez que este cambia. Cada operación clona el cliente y suelta el cerrojo **antes** de cualquier `await` (un cerrojo envenenado se recupera con `PoisonError::into_inner`, como en `runtime`). Al crearse lee el token del store; si el store falla, arranca sin token.
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
  | `service_loads_stored_token_on_new` | `MemoryStore` con token antes de `new`; luego `search` | la búsqueda lleva `Authorization` |
  | `service_new_store_failure_starts_without_token` | `FailingStore`; luego `search` | `new` no falla; la búsqueda va sin `Authorization` |

### T10 — Comandos Tauri del explorador
- **Ficheros/módulos:** `src-tauri/src/commands/error.rs` (nuevo: `ErrorPayload` movido desde `runtime.rs`), `src-tauri/src/commands/runtime.rs` (cambia el `use` de `ErrorPayload` y se añade `RuntimeAppState::hardware()`; `error_to_payload` y sus tests se quedan), `src-tauri/src/commands/hf.rs` (nuevo), `src-tauri/src/commands.rs` (`pub mod error; pub mod hf;`), `src-tauri/src/lib.rs` (registro en `register()` y `app.manage(commands::hf::production_state()?)` en `setup`)
- **Depende de:** T7, T9, T15, T17
- **Cubre:** CA1–CA11, CA15–CA17 (puente)
- **Subtareas:**
  - [ ] T10.1 Mover `ErrorPayload { code: &'static str, message: String }` a `commands/error.rs` sin cambiar su forma ni su JSON (refactor equivalente: los tests de `runtime.rs` siguen en verde sin tocarlos salvo el `use`).
  - [ ] T10.2 Comandos `async` (patrón de `runtime.rs`, `Result<T, ErrorPayload>`): `hf_search(query: String, sort: SortBy) -> SearchPageDto`, `hf_search_next(next_url: String) -> SearchPageDto`, `hf_model_detail(repo_id: String) -> ModelDetail`, `hf_token_status() -> TokenStatus`, `hf_token_save(token: String) -> WhoAmI` y `hf_token_delete() -> ()`. Solo `SearchPageDto { items: Vec<SearchItem>, next: Option<String> }` es un tipo propio de `src-tauri` (porque `reqwest::Url` no serializa); el resto son los tipos de `core` serializados con la forma por defecto. Tauri convierte los argumentos: la UI los envía en camelCase (`{ nextUrl }`, `{ repoId }`). Los argumentos que ve la UI no cambian por el hardware: `hf_search`, `hf_search_next` y `hf_model_detail` reciben además `State<'_, RuntimeAppState>`, que Tauri inyecta.
  - [ ] T10.5 Hardware hacia la ficha y la búsqueda: `HfAppState { service: HfService, budget: BudgetCache }` (lo que gestiona `app.manage`) y `RuntimeAppState::hardware(&self) -> Result<HardwareInfo, RuntimeError>`, que delega en el `RuntimeManager::hardware()` existente (el comando `runtime_hardware` pasa a usarlo, sin cambiar su comportamiento). `hf_search`, `hf_search_next` y `hf_model_detail` obtienen el presupuesto con `state.budget.get_or_detect(|| runtime.hardware()).await` **antes** de llamar al servicio y se lo pasan. Así la detección (con `nvidia-smi`) se hace en la primera de esas llamadas y se reutiliza durante toda la sesión; si falla, el comando sigue con `MemoryBudget::unknown()` (sin etiquetas) y no devuelve error. No se detecta nada al arrancar la app.
  - [ ] T10.3 `hf_error_to_payload(&HfError) -> ErrorPayload`, función pura. `code` en PascalCase igual a la variante (`"Network"`, `"Unauthorized"`, `"Forbidden"`, `"NotFound"`, `"RateLimited"`, `"Status"`, `"Parse"`, `"InvalidRepoId"`, `"InvalidToken"`, `"TooManyPages"`, `"ForeignUrl"`, `"Secret"`). `message` en español: los textos de CA10 (`RateLimited` con `Some(n)`: «Hugging Face limita las peticiones; reintenta en n s»; con `None`: «Hugging Face limita las peticiones; reintenta más tarde»; `Unauthorized` y `Forbidden`: «Acceso denegado: revisa tu token»), `InvalidToken`: «El token no tiene un formato válido», `InvalidRepoId`: «Identificador de modelo no válido», y para el resto un texto genérico que nunca incluye el token. `impl From<HfError> for ErrorPayload` para usar `?`.
  - [ ] T10.4 `production_state() -> Result<HfAppState, reqwest::Error>`: construye el `reqwest::Client` (T6.1), el `HfService` con la base `https://huggingface.co` y `KeyringStore`, y un `BudgetCache` vacío. No toca la red al arrancar. Los comandos no contienen lógica: solo delegan y convierten.
- **Tests unitarios** (en `commands/hf.rs`, sin red ni llavero):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `error_payload_hf_rate_limited` | `RateLimited { retry_after_secs: Some(30) }` | `code == "RateLimited"`, mensaje con «reintenta en 30 s» |
  | `error_payload_hf_rate_limited_without_secs` | `RateLimited { retry_after_secs: None }` | mensaje con «reintenta más tarde» |
  | `error_payload_hf_not_found` | `NotFound` | `code == "NotFound"`, «Modelo no encontrado» |
  | `error_payload_hf_unauthorized` | `Unauthorized` | `code == "Unauthorized"`, «Acceso denegado: revisa tu token» |
  | `error_payload_hf_forbidden` | `Forbidden` | `code == "Forbidden"`, «Acceso denegado: revisa tu token» |
  | `error_payload_hf_parse` | `Parse("x")` | «Respuesta inesperada de Hugging Face» |
  | `error_payload_hf_network` | `Network` | «Sin conexión con Hugging Face» |
  | `error_payload_hf_codes` | una instancia de cada variante de `HfError` | cada `code` es el nombre de su variante y ningún `message` está vacío |
  | `error_payload_hf_never_contains_token` | `Secret(msg)` con `msg` construido a partir de un fallo del llavero con un token `hf_…` de prueba | el JSON serializado no contiene el token |
  | `json_search_page_para_la_ui` | `SearchPageDto` con 1 `SearchItem` (`model`: id `bartowski/Meta-Llama-3.1-8B-Instruct-GGUF`, `gated: Manual`, sin `pipeline_tag`, `gguf { total: Some(8030261312), architecture: Some("llama"), context_length: Some(131072) }`; `hint` del CA17) y `next` | JSON exacto: `{"items":[{"model":{"id":"bartowski/Meta-Llama-3.1-8B-Instruct-GGUF","downloads":…,"likes":…,"gated":"Manual","last_modified":…,"pipeline_tag":null,"gguf":{"total":8030261312,"architecture":"llama","context_length":131072}},"hint":{"estimate":{"bytes":6481006131,"kv_included":true},"fit":"Gpu"}}],"next":"https://…"}` |
  | `json_search_item_without_hint` | `SearchItem` con `gguf: None` y `hint: None` | JSON exacto con `"gguf":null` y `"hint":null` |
  | `json_model_detail_para_la_ui` | `ModelDetail` de ejemplo con `ctx: 8192`, el presupuesto «RTX 4090 + 32 GiB» y una variante de 2 partes con `fit: Some(NoFit)`, `recommended: false` | JSON exacto con `ctx`, `info.gguf.context_length`, `"budget":{"gpu_bytes":20605776691,"ram_bytes":27487790694,"unified":false,"usable_percent":80}`, `variants[].variant.total_bytes`, `variants[].variant.parts[].sha256`, `variants[].estimate.kv_included`, `"fit":"NoFit"` y `"recommended":false` |
  | `json_fit_variants` | `Some(Gpu)`, `Some(Ram)`, `Some(NoFit)`, `None` | `"Gpu"`, `"Ram"`, `"NoFit"`, `null` |
  | `json_budget_unknown` | `MemoryBudget::unknown()` | `{"gpu_bytes":null,"ram_bytes":null,"unified":false,"usable_percent":80}` |
  | `json_token_status_para_la_ui` | `TokenStatus` serializado | JSON exacto `{"present":true,"user":"ana","invalid":false}`, sin ningún campo con el token |
  | `sort_by_deserializa` | `"downloads"`, `"likes"`, `"x"` | `Downloads`, `Likes`, error |

### T11 — UI: cliente tipado y utilidades de formato
- **Ficheros/módulos:** `ui/src/lib/hf/types.ts`, `ui/src/lib/hf/api.ts`, `ui/src/lib/hf/view-model.ts` y sus `*.test.ts`; `ui/src/lib/errors.ts` + `ui/src/lib/errors.test.ts` (nuevos: `ErrorPayload`, `isErrorPayload` y `errorMessage` movidos desde `ui/src/lib/runtime/types.ts` y `ui/src/lib/runtime/view-model.ts`, junto con sus tests del `describe("errorMessage")` de `view-model.test.ts`); `ui/src/lib/runtime/{types.ts,view-model.ts,view-model.test.ts,RuntimeSettings.svelte}` (solo cambian los imports)
- **Depende de:** T10
- **Cubre:** CA1, CA4, CA6, CA10, CA16, CA17
- **Subtareas:**
  - [ ] T11.1 Mover `ErrorPayload` y `errorMessage` a `ui/src/lib/errors.ts` sin cambiar su comportamiento (refactor equivalente; los tests de `runtime` siguen en verde).
  - [ ] T11.2 `types.ts`: espejo del JSON fijado en T10 (`snake_case`, `Gated = "Open" | "Auto" | "Manual"`, `SortBy = "downloads" | "likes"`, `Fit = "Gpu" | "Ram" | "NoFit"`, `MemoryBudget`, `FitHint`, `SearchItem { model, hint }`, `VariantView` con `fit: Fit | null` y `recommended: boolean`, `ModelDetail` con `budget`), con el comentario que remite a los tests `json_*_para_la_ui`, como en `ui/src/lib/runtime/types.ts`.
  - [ ] T11.3 `api.ts`, mismo patrón que `ui/src/lib/runtime/api.ts` (`invoke` importado de `@tauri-apps/api/core`, la promesa se devuelve sin transformar el rechazo): `hfSearch(query, sort)`, `hfSearchNext(nextUrl)`, `hfModelDetail(repoId)`, `hfTokenStatus()`, `hfTokenSave(token)` y `hfTokenDelete()`.
  - [ ] T11.4 `view-model.ts`: `formatGiB(bytes)` con 1 decimal y coma decimal (`Intl.NumberFormat('es-ES')`), `formatCount(n)` (`12,3 mil`, `8,2 M`), `formatDate(iso)` en `es-ES` y UTC (sin depender de la zona horaria de la máquina) y `estimateLabel(estimate)` («≈ X GiB (orientativa)» o «≈ X GiB (sin caché KV)»).
  - [ ] T11.5 `view-model.ts`: `fitLabel(fit)` («Cabe en GPU», «Cabe en RAM», «No cabe» o `null`), `fitHintBadge(hint)` (`"≈ " + fitLabel`, o `null` si no hay `hint` o su `fit` es `null`), `fitHintTitle(hint)` («Orientativa: estimada para una cuantización Q4_K_M típica (≈ X GiB)») y `budgetLegend(budget)` con los 5 textos de CA16. Los GiB, con `formatGiB`; el porcentaje, con `budget.usable_percent` (la UI no repite la constante).
- **Tests unitarios (Vitest; `mockIPC`/`clearMocks` de `@tauri-apps/api/mocks`):**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `format_gib_one_decimal` | `5_993_741_824` | `"5,6 GiB"` |
  | `format_gib_ctx_8192_example` | `6_530_612_736` | `"6,1 GiB"` |
  | `format_gib_zero` | `0` | `"0,0 GiB"` |
  | `format_gib_large` | `54_657_735_616` | `"50,9 GiB"` |
  | `format_count_millions` | `8_181_570` | `"8,2 M"` |
  | `format_count_thousands` | `12_345` | `"12,3 mil"` |
  | `format_count_small` | `999` | `"999"` |
  | `format_date_utc` | `"2026-11-12T02:15:52.000Z"` | `"12/11/2026"` |
  | `estimate_label_with_kv` | `{ bytes: 6_530_612_736, kv_included: true }` | `"≈ 6,1 GiB (orientativa)"` |
  | `estimate_label_without_kv` | `{ bytes: 5_456_870_912, kv_included: false }` | `"≈ 5,1 GiB (sin caché KV)"` |
  | `fit_label_values` | `"Gpu"`, `"Ram"`, `"NoFit"`, `null` | `"Cabe en GPU"`, `"Cabe en RAM"`, `"No cabe"`, `null` |
  | `fit_hint_badge_with_fit` | `{ estimate: { bytes: 6_481_006_131, kv_included: true }, fit: "Gpu" }` | `"≈ Cabe en GPU"` |
  | `fit_hint_badge_without_fit` | `{ estimate: {…}, fit: null }` y `null` | `null` en los dos casos |
  | `fit_hint_title_text` | el `hint` del 8B | `"Orientativa: estimada para una cuantización Q4_K_M típica (≈ 6,0 GiB)"` |
  | `budget_legend_dedicated` | `{ gpu_bytes: 20_605_776_691, ram_bytes: 27_487_790_694, unified: false, usable_percent: 80 }` | `"Comparado con 19,2 GiB de VRAM y 25,6 GiB de RAM utilizables (80 % del total)"` |
  | `budget_legend_unified` | `{ gpu_bytes: 13_743_895_347, ram_bytes: 13_743_895_347, unified: true, usable_percent: 80 }` | `"Memoria unificada: comparado con 12,8 GiB utilizables (80 % del total)"` |
  | `budget_legend_ram_only` | `{ gpu_bytes: null, ram_bytes: 27_487_790_694, unified: false, usable_percent: 80 }` | `"VRAM desconocida: solo se compara con la RAM (25,6 GiB utilizables, 80 % del total)"` |
  | `budget_legend_gpu_only` | `{ gpu_bytes: 10_302_888_345, ram_bytes: null, unified: false, usable_percent: 80 }` | `"Comparado con 9,6 GiB de VRAM utilizables (80 % del total); RAM desconocida"` |
  | `budget_legend_unknown` | `{ gpu_bytes: null, ram_bytes: null, unified: false, usable_percent: 80 }` | `"No se pudo detectar la memoria del equipo"` |
  | `api_hf_search_invokes_command` | `hfSearch("qwen", "downloads")` con `mockIPC` que registra `(cmd, args)` | una llamada `("hf_search", { query: "qwen", sort: "downloads" })`; resuelve con la respuesta del mock (`toEqual`) |
  | `api_hf_search_next_sends_next_url` | `hfSearchNext("https://huggingface.co/api/models?cursor=abc")` | una llamada `("hf_search_next", { nextUrl: "https://huggingface.co/api/models?cursor=abc" })` |
  | `api_hf_model_detail_sends_repo_id` | `hfModelDetail("a/b")` | una llamada `("hf_model_detail", { repoId: "a/b" })` |
  | `api_hf_token_save_sends_token` | `hfTokenSave("hf_…")` | una llamada `("hf_token_save", { token: "hf_…" })` |
  | `api_propagates_rejection` | `mockIPC` lanza un `ErrorPayload` concreto `{ code: "NotFound", message: "Modelo no encontrado" }` | `rejects.toBe(ese mismo objeto)` (nunca `toBeDefined`) |
  | tests existentes del `describe("errorMessage")` de `runtime/view-model.test.ts` | los mismos | movidos a `errors.test.ts` sin cambiar nombres ni aserciones; siguen en verde |

### T12 — UI: pantalla Explorar (búsqueda, orden y paginación)
- **Ficheros/módulos:** `ui/src/views/Explore.svelte` (sustituye el marcador «Próximamente»; conserva el `<h1>` con `sectionLabel("explore")`), `ui/src/views/Explore.test.ts` (nuevo), `ui/src/lib/hf/ModelCard.svelte`, `ui/src/lib/hf/explore-state.svelte.ts` (estado con runas) y `ui/src/lib/hf/explore-state.test.ts`. La navegación (`ui/src/lib/navigation.ts`) no cambia: `explore` ya existe.
- **Depende de:** T11
- **Cubre:** CA1, CA2, CA3, CA10, CA17
- **Subtareas:**
  - [ ] T12.1 Estado `ExploreState`: `query`, `sort`, `items` (`SearchItem[]`), `next`, `loading` y `error` (texto ya traducido con `errorMessage`). `search()` sustituye los resultados; `loadMore()` los añade y evita peticiones dobles mientras `loading`.
  - [ ] T12.2 La búsqueda se lanza con Enter o con el botón (sin debounce por tiempo, para no depender del reloj). Al cambiar el orden se relanza la búsqueda.
  - [ ] T12.3 Tarjeta (`ModelCard` recibe un `SearchItem`): id, descargas, likes, fecha (de `item.model`) e insignia «Restringido» si `gated !== "Open"`. Insignia de encaje con `fitHintBadge(item.hint)` y `title` = `fitHintTitle(item.hint)`; si `fitHintBadge` devuelve `null`, no se pinta nada (ni «?»). Al hacer clic se abre la ficha (T13).
  - [ ] T12.4 Estados vacíos («Sin resultados») y de error (`ErrorPayload.message`, con un botón «Reintentar»).
- **Tests unitarios (Vitest + Testing Library, con `mockIPC` respondiendo a `hf_search`/`hf_search_next`; las respuestas tienen la forma `{ items: [{ model, hint }], next }` de T10):**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `explorar_renders_results` | `hf_search` devuelve 2 modelos, uno con `gated: "Manual"` | 2 tarjetas; 1 insignia «Restringido» |
  | `explorar_renders_fit_badges` | 3 resultados: `hint = { estimate: { bytes: 6_481_006_131, kv_included: true }, fit: "Gpu" }`, `hint = { estimate: { bytes: 52_761_967_679, kv_included: true }, fit: "NoFit" }` y `hint: null` | insignias «≈ Cabe en GPU» y «≈ No cabe», la primera con `title` «Orientativa: estimada para una cuantización Q4_K_M típica (≈ 6,0 GiB)»; la 3.ª tarjeta sin insignia de encaje; 1 sola llamada a `hf_search` |
  | `explorar_hint_without_fit_no_badge` | resultado con `hint = { estimate: {…}, fit: null }` (hardware desconocido) | la tarjeta no muestra insignia de encaje |
  | `explorar_sort_change_replaces_results` | buscar y cambiar a «Likes» | 2.ª llamada con `sort: "likes"`; la lista solo tiene los nuevos |
  | `explorar_load_more_appends` | 1.ª página con `next`, 2.ª sin él | 4 tarjetas; el botón «Cargar más» desaparece |
  | `explorar_load_more_no_double_request` | doble clic rápido en «Cargar más» con `hf_search_next` pendiente | 1 sola llamada a `hf_search_next` |
  | `explorar_empty_state` | `hf_search` devuelve `{ items: [], next: null }` | texto «Sin resultados» |
  | `explorar_shows_error` | `hf_search` lanza `{ code: "RateLimited", message: "Hugging Face limita las peticiones; reintenta en 30 s" }` | texto «reintenta en 30 s» y botón «Reintentar» |
  | `explorar_retry_repeats_search` | error y luego clic en «Reintentar» con el mock ya respondiendo bien | 2.ª llamada a `hf_search` con los mismos argumentos; desaparece el error |
  | `explorar_enter_triggers_search` | escribir «qwen» y pulsar Enter | 1 llamada con `query: "qwen"` |
  | `explore_state_search_replaces_items` | `ExploreState` con `items` previos; `search()` | `items` solo con los nuevos (test del estado, sin componente) |

### T13 — UI: ficha del modelo con variantes, estimación y botón Descargar
- **Ficheros/módulos:** `ui/src/lib/hf/ModelDetail.svelte`, `ui/src/lib/hf/VariantRow.svelte`, `ui/src/lib/hf/ModelDetail.test.ts`, `ui/src/views/Explore.svelte` (alterna lista ↔ ficha) y `ui/src/views/Explore.test.ts`
- **Depende de:** T12
- **Cubre:** CA4, CA5, CA6, CA11, CA16
- **Subtareas:**
  - [ ] T13.1 `ModelDetail.svelte` recibe la prop `repoId`, llama a `hfModelDetail(repoId)` al montarse y muestra «Cargando…» mientras tanto. Cabecera: id, licencia, arquitectura, contexto máximo (`info.gguf.context_length`) e insignia «Restringido» con el texto «Requiere un token de Hugging Face (en Ajustes) y aceptar las condiciones en la web del modelo».
  - [ ] T13.2 Tabla de variantes: cuantización (o «desconocida»), tamaño, «N partes» si hay más de una, «incompleta» si `!complete` y la estimación con `estimateLabel`. Debajo, la leyenda «RAM o VRAM aprox. con contexto de <ctx> tokens», con `ctx` tomado de `ModelDetail.ctx`.
  - [ ] T13.3 Botón «Descargar» por variante: deshabilitado, con `title="Disponible próximamente"`, y oculto en las incompletas. Recibe la prop `onDownload?: (req: DownloadRequest) => void` para que el plan 006 la conecte.
  - [ ] T13.4 En `Explore.svelte`, al hacer clic en una tarjeta se muestra `ModelDetail` en lugar de la lista, con un botón «Volver» que restaura la lista, la consulta y los resultados ya cargados sin volver a pedirlos.
  - [ ] T13.5 Encaje en la tabla: columna «Encaje» con `fitLabel(variant.fit)` (vacía si es `null`, p. ej. en las incompletas). La fila con `recommended` lleva la insignia «Recomendada» y se destaca visualmente (clase CSS propia y `aria-label` «Recomendada para este equipo»). Si ninguna fila tiene `recommended` y al menos una tiene `fit === "NoFit"`, sobre la tabla aparece «Ninguna cuantización cabe en la memoria de este equipo». Bajo la leyenda del contexto, `budgetLegend(detail.budget)`.
- **Tests unitarios (Vitest + Testing Library, `mockIPC` respondiendo a `hf_model_detail`):**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `ficha_renders_variants` | detalle con 3 variantes | 3 filas en el orden recibido, con cuantización y tamaño formateado |
  | `ficha_unknown_quant_label` | variante con `quant: null` | texto «desconocida» |
  | `ficha_split_shows_parts` | variante de 2 partes | texto «2 partes» |
  | `ficha_incomplete_variant` | `complete: false` | texto «incompleta» y sin botón «Descargar» |
  | `ficha_estimate_orientative` | `ctx: 8192`, variante con `estimate: { bytes: 6_530_612_736, kv_included: true }` | «≈ 6,1 GiB (orientativa)» y la leyenda «con contexto de 8192 tokens» |
  | `ficha_estimate_fallback_ctx` | `ctx: 4096`, `info.gguf: null`, `estimate: { bytes: 5_456_870_912, kv_included: false }` | «≈ 5,1 GiB (sin caché KV)» y la leyenda «con contexto de 4096 tokens» |
  | `ficha_download_button_disabled` | variante completa | botón deshabilitado con `title` «Disponible próximamente» |
  | `ficha_fit_labels` | detalle con el presupuesto «RTX 4090 + 32 GiB» y 4 variantes con `fit` `"Gpu"`, `"Gpu"`, `"Ram"`, `"NoFit"` | en cada fila, en orden: «Cabe en GPU», «Cabe en GPU», «Cabe en RAM», «No cabe»; leyenda «Comparado con 19,2 GiB de VRAM y 25,6 GiB de RAM utilizables (80 % del total)» |
  | `ficha_recommended_highlight` | el mismo detalle con `recommended: true` solo en la 2.ª (`Q4_0`) | «Recomendada» aparece exactamente 1 vez, dentro de la fila cuya columna de cuantización muestra `Q4_0` (el `quant`, no el `name`) (`within(fila)`) |
  | `ficha_no_recommended` | 2 variantes con `fit: "NoFit"` y ninguna `recommended` | ningún «Recomendada»; texto «Ninguna cuantización cabe en la memoria de este equipo» |
  | `ficha_incomplete_no_fit_label` | variante con `complete: false`, `fit: null` | su fila sin «Cabe en…» ni «No cabe» ni «Recomendada» |
  | `ficha_budget_unknown` | `budget` con `gpu_bytes: null`, `ram_bytes: null` y todas las variantes con `fit: null` | «No se pudo detectar la memoria del equipo»; ninguna etiqueta de encaje ni el aviso «Ninguna cuantización cabe…» |
  | `ficha_gated_notice` | `info.gated: "Manual"` | aviso con «token de Hugging Face (en Ajustes)» y las condiciones |
  | `ficha_error_not_found` | `hf_model_detail` lanza `{ code: "NotFound", message: "Modelo no encontrado" }` | «Modelo no encontrado» |
  | `explorar_open_detail_and_back` | resultados cargados; clic en una tarjeta y luego en «Volver» | se pide `hf_model_detail` con `{ repoId }` de la tarjeta; tras «Volver», las mismas tarjetas y ninguna llamada nueva a `hf_search` |

### T14 — UI: sección «Hugging Face» en Ajustes (token)
- **Ficheros/módulos:** `ui/src/lib/hf/TokenSettings.svelte`, `ui/src/lib/hf/TokenSettings.test.ts`, `ui/src/lib/hf/view-model.ts` (`isValidTokenFormat`, `tokenStatusLabel`), `ui/src/views/Settings.svelte` (añade `<h2>Hugging Face</h2>` y `<TokenSettings />` tras la sección «llama.cpp») y `ui/src/views/Settings.test.ts` (el `mockCommands` existente responde también a `hf_token_status`, porque hoy su rama `default` devuelve `"NotInstalled"`)
- **Depende de:** T11
- **Cubre:** CA7, CA8
- **Subtareas:**
  - [ ] T14.1 Al montarse, `TokenSettings` llama a `hfTokenStatus()`. El estado muestra «Conectado como <user>», «Token no válido» (`invalid`), «Token guardado (sin verificar)» (`present` sin `user` ni `invalid`, p. ej. sin red) o «Sin token».
  - [ ] T14.2 Campo `type="password"` con los botones «Guardar» y «Borrar» («Borrar» solo si `present`). Con formato inválido muestra «El token debe empezar por hf_ y tener al menos 30 caracteres alfanuméricos más» sin invocar el comando (la misma gramática que en T8, replicada en `isValidTokenFormat` para dar feedback inmediato; la autoridad sigue siendo el backend). Si `hfTokenSave` rechaza con `code === "Unauthorized"` muestra «Token no válido»; con otro código, `errorMessage(error)`. Tras guardar se vacía el campo. El token nunca se guarda en estado global ni en `localStorage`.
  - [ ] T14.3 Explorar **no** muestra estado ni gestión del token; la ficha solo remite a Ajustes en el aviso de los repos restringidos (T13.1).
- **Tests unitarios (Vitest + Testing Library, `mockIPC`):**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `token_format_valid` | `isValidTokenFormat("  hf_" + 34 alfanuméricos + " ")` | `true` |
  | `token_format_invalid` | `"abc"`, `"hf_abc"`, `"hf_abc def…"` (≥ 30 tras `hf_` con espacio interior), `""` | `false` en todos |
  | `token_settings_save_valid` | `hf_` + 34 caracteres; `hf_token_save` devuelve `{ name: "ana", role: "read" }` | `hf_token_save` llamado una vez con `{ token }`; «Conectado como ana»; campo vacío |
  | `token_settings_invalid_format_no_call` | `abc` | mensaje de formato; `hf_token_save` no llamado |
  | `token_settings_unauthorized` | `hf_token_save` lanza `{ code: "Unauthorized", message: "Acceso denegado: revisa tu token" }` | «Token no válido»; el estado previo («Sin token») no cambia |
  | `token_settings_other_error` | `hf_token_save` lanza `{ code: "Secret", message: "No se pudo acceder al llavero del sistema" }` | se muestra ese mensaje |
  | `token_settings_delete` | `hf_token_status` → `{ present: true, user: "ana", invalid: false }`; clic en «Borrar» | `hf_token_delete` llamado; «Sin token»; desaparece «Borrar» |
  | `token_settings_revoked` | `hf_token_status` → `{ present: true, user: null, invalid: true }` | «Token no válido» |
  | `token_settings_unverified` | `hf_token_status` → `{ present: true, user: null, invalid: false }` | «Token guardado (sin verificar)» |
  | `token_not_persisted_in_local_storage` | guardar un token válido | `localStorage` sin claves nuevas |
  | `ajustes_incluye_seccion_hugging_face` (en `Settings.test.ts`) | render de `Settings` con `hf_token_status` → `{ present: false, user: null, invalid: false }` | encabezado `h2` «Hugging Face» y el texto «Sin token»; los tests existentes de `Settings.test.ts` siguen en verde |

### Verificación manual (usuario, al final del plan)

No forma parte del gate. Toca la red real y el llavero del sistema, así que no puede ser un test unitario (R3).
1. Buscar «qwen gguf» por Descargas y luego por Likes, y cargar 2 páginas más.
2. Abrir `unsloth/Qwen3.8-27B-GGUF` (o un repo similar con splits): la variante BF16 sale como una sola fila de 2 partes y no aparecen `mmproj`, `imatrix` ni `MTP/mtp-…`.
3. En Ajustes → «Hugging Face», guardar un token real: aparece «Conectado como …» y se comprueba la entrada en el Administrador de credenciales (Windows) o en Acceso a Llaveros (macOS). Borrarlo: la entrada desaparece.
4. Encaje con el hardware real: en la ficha del paso 2, comprobar que la leyenda muestra el 80 % de la RAM total del equipo (Administrador de tareas → Rendimiento → Memoria en Windows; «Acerca de este Mac» en macOS) y, si hay una NVIDIA, el 80 % de su VRAM; que la fila «Recomendada» es la mayor con «Cabe en GPU» (o, si no hay, con «Cabe en RAM»); y que las tarjetas de la búsqueda muestran la insignia «≈ …».

## Fuera de alcance

- Descarga, progreso, pausa/reanudación, cancelación, verificación SHA256 y espacio en disco: **plan 006**. Aquí solo se definen `DownloadRequest` y el botón deshabilitado.
- Comprobar si el usuario tiene concedido el acceso a un repo gated concreto (se detectará con un 403 al descargar, en el plan 006).
- Archivos `mmproj` (multimodal, fuera del MVP) e `imatrix`.
- Archivos MTP y decodificación especulativa (borrador + modelo principal): fuera del MVP; los `mtp*` solo se excluyen de la ficha.
- El formato antiguo de split `*.gguf.partXofY`.
- Selector de contexto para recalcular la estimación y el encaje (los perfiles con `ctx-size` llegan en el plan de ejecución).
- Filtro u orden «solo los que caben» en la búsqueda, y pedir el árbol de cada resultado para afinar su insignia (la insignia se calcula solo con lo que trae el listado).
- Descarga parcial en la GPU (`n-gpu-layers` < total) como categoría propia, sumar la VRAM de varias GPUs y considerar la RAM disponible en cada momento: se usa la GPU de más VRAM y la RAM total con margen.
- Tener en cuenta el backend instalado (CPU, CUDA, Vulkan, Metal) en el encaje: se evalúa el hardware.
- Detectar la VRAM de GPUs no NVIDIA (AMD/Intel con Vulkan) y mostrar la RAM en la sección «llama.cpp» de Ajustes.
- Recalibrar la constante de la insignia por arquitectura (MoE, modelos con vocabularios muy grandes).
- Insignia o gestión del token en la pantalla Explorar (el token solo se gestiona en Ajustes).
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
| Mover `ErrorPayload` (Rust) y `errorMessage` (UI) rompe el runtime del plan 004 | Se mueven sin cambiar forma ni comportamiento, en la primera subtarea de T10 y T11, y los tests existentes de `runtime` actúan de red de seguridad (CA13). |
| La tabla de tipos espejo TS diverge del JSON real | Tests `json_*_para_la_ui` en `src-tauri` con el JSON exacto, como en el plan 004. |
| El contexto de la estimación (hasta 8192) no coincide con el que el usuario configure después | La ficha muestra el `ctx` usado en la leyenda; el selector de contexto queda fuera de alcance. |
| La heurística Q4_K_M de la insignia de búsqueda se aleja del repo real (repos que solo publican Q8_0 o F16, MoE, embeddings enormes) | Se marca con «≈» y un `title` que explica que es para una Q4_K_M típica; la constante está calibrada con un archivo real (error del 0,06 %) y es única; la ficha, con tamaños reales, es la referencia. |
| El margen del 20 % no basta o sobra (en Apple Silicon macOS limita la memoria que la GPU puede fijar a una fracción de la RAM; búferes de cómputo grandes con contextos altos) | Constante única `SAFETY_MARGIN_PERCENT`, visible en la leyenda («80 % del total») y fácil de recalibrar; todo el encaje se presenta como orientativo. |
| «Cabe en GPU» con un runtime de CPU instalado, o GPU AMD/Intel sin cifra de VRAM | La etiqueta describe el hardware (decisión explícita); sin cifra de VRAM solo se compara con la RAM y la leyenda lo dice («VRAM desconocida»). |
| La primera búsqueda o ficha espera a la detección de hardware (`nvidia-smi`, con un límite de 10 s por llamada del plan 004) | `BudgetCache` detecta una sola vez por sesión, en `spawn_blocking`; si falla, se sigue sin etiquetas y se reintenta en la siguiente petición. |
| `expand[]=gguf` en el listado trae `chat_template` (unos 4 KB por resultado, unos 80 KB por página de 20) o HF deja de devolverlo | El DTO ignora los campos desconocidos y `gguf` es opcional: sin él no hay insignia, pero la búsqueda funciona. |
| Añadir `ram_bytes` a `HardwareInfo` rompe la UI del plan 004 | Campo opcional al final del JSON, espejo TS actualizado en la misma tarea (T15), test `json_hardware_status_y_update_para_la_ui` y `npm run check` en el gate. |
| Nueva dependencia `sysinfo`, aprobada el 2026-11-27 (tamaño, `unsafe` interno, cambios de API entre versiones) | Sin features por defecto y solo el de memoria; aislada tras `SystemProbe::total_memory_bytes`; los tests usan `FakeProbe`; el developer fija la versión tras consultar docs.rs. |

## Preguntas abiertas

