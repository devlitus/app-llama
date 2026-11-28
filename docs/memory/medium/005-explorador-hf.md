---
schema: memoria-plan/2
plan: 005-explorador-hf
estado: abierto
---

# Memoria del plan 005-explorador-hf

## T1

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a399c0c885a43537a | verde | APPROVED |

### Resúmenes

- C1 · developer · T1 hf: tipos de dominio (Gated, ModelSummary, ModelInfo, TreeEntry, WhoAmI), HfError y fixtures recortadas de la API real.
- C1 · reviewer · T1 hf aprobada: tipos de dominio, Gated estricto, DTO crudos y HfError conformes al plan; gate verde

### Decisiones

- **limitacion-documentada** — whoami.json sintetico aceptado (sin token de HF); campos obligatorios y Network unitaria segun plan (C1)
- **refactor-equivalente** — DTO crudos privados con serde(from) para cardData.license y auth.accessToken.role; HfError::Network sin payload segun plan. (C1)

### Errores

- **dato-inventado** — minor: model_info_gated_manual.json mezcla id meta-llama con _id, cardData y licencia de unsloth/Qwen (C1)

## T2

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a9c62bec2cacf973c | verde | CHANGES_REQUESTED |
| 2 | 2026-11-27 | 1 | a7de46f3b0a472d4b | verde | APPROVED |

### Resúmenes

- C1 · developer · T2 extract_quant a mano sin regex en hf/quant.rs y fixture gated_manual coherente (id de unsloth/Qwen3.8-27B-GGUF)
- C1 · reviewer · extract_quant correcto y gramática completa sin regex, pero faltan 2 tests de la tabla del plan y otros 2 tienen nombre y entrada distintos
- C2 · developer · Ciclo 2 de T2: tests de quant.rs alineados con los 15 nombres y entradas del plan, sin helper envoltorio.
- C2 · reviewer · los 15 tests de la tabla de T2 existen con nombre y entrada exactos; helper quant eliminado; produccion sin cambios; gate verde

### Decisiones

- **refactor-equivalente** — tests extra (rejects_out_of_grammar, mxfp4_moe, file_name_wins) y asertos adicionales en quant_none/rightmost_wins se aceptan porque no sustituyen a los de la tabla (C1, C2)

### Errores

- **fixture-distinto-del-plan** — Tests con nombre o entrada distintos de la tabla del plan y dos filas ausentes (corregido en ciclo 2). (C1, C2)
- **reserva-sin-test** — faltan quant_model_name_digits_not_matched (Qwen2.5-7B-Instruct.gguf) y quant_empty_path ("") de la tabla de T2 (C1)

## T3

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | ab5c9c64fe663e5c0 | verde | APPROVED |

### Resúmenes

- C1 · developer · T3 filtrado de GGUF, agrupación de splits y DownloadRequest en hf/files.rs
- C1 · reviewer · T3 aprobada; filtro, agrupación de splits y DownloadRequest correctos, 17 filas del plan con test exacto, gate verde

### Decisiones

- **etiqueta-vs-quant** — UD-IQ2_XXS/UD-Q4_K_XL del plan son etiquetas; name conserva UD- y quant sigue T2; conviene aclararlo en el plan (T3, T7, CA16) (C1)
- **limitacion-documentada** — download_request recibe &str hasta que T5 defina RepoId; el planner debe anotar el cambio de firma en T5 (C1)
- **refactor-equivalente** — parse_split_suffix/strip_gguf_extension compartidos desde quant.rs sin cambiar el comportamiento ni los tests de T2 (C1)

### Errores

- (ninguno)

## T4

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | ab767607f0d9e1ffc | verde | APPROVED |

### Resúmenes

- C1 · developer · T4 memory.rs con estimation_ctx y estimate_memory (u128, KV truncado por token), 14 tests de la tabla.
- C1 · reviewer · T4 estimación de memoria correcta; 14 tests de la tabla con entradas exactas; aritmética CA6 verificada a mano; gate verde

### Decisiones

- **aritmetica-u128-sin-desborde** — el peor caso intermedio (u64::MAX*16384*u32::MAX ~ 2^80) cabe en u128, así que la saturación final a u64::MAX basta (C1)
- **refactor-equivalente** — kv_cache_bytes aparte para truncar B/token antes de multiplicar por ctx. (C1)

### Errores

- (ninguno)

## T5

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a83574ccfb62a9a43 | verde | APPROVED |

### Resúmenes

- C1 · developer · T5 url.rs (RepoId, SearchParams, URLs, parse_next_link same-origin) y T5.5 download_request(&RepoId)
- C1 · reviewer · T5 aprobada en ciclo 1; RepoId, URLs y Link seguros, 19 tests del plan + extras, gate verde

### Decisiones

- **limitacion-documentada** — separar el header Link por comas se acepta: una URL con coma solo corta la paginacion (None), nunca acepta otro origen (C1)
- **origen-estricto-link** — parse_next_link compara esquema, host normalizado y port_or_known_default; relativas descartadas por Url::parse (C1)
- **refactor-equivalente** — regex de CA12 a mano con split_once y segmentos validados; sin crate regex (C1)

### Errores

- **literal-duplicado** — minor: tree_url fija "main" a mano mientras files.rs tiene DEFAULT_REVISION (C1)

## T6

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a6791ed68333e994f | verde | APPROVED |

### Resúmenes

- C1 · developer · T6 cliente HfClient (search, search_next, mapeo de errores, bearer solo a origen de confianza) y 2 minor de T5 en url.rs
- C1 · reviewer · T6 HfClient (search, search_next, mapeo de estados, bearer solo a origen de confianza) y minors de T5; 14 filas del plan cubiertas, gate verde, solo minors

### Decisiones

- **limitacion-documentada** — T8.1 (HfToken minimo) adelantada a T6 aceptable; el plan debe anotarlo para que T8 no duplique los 8 tests (C1)
- **origen-estricto-link** — is_trusted_target compartido por parse_next_link y HfClient::get; reqwest 0.13 quita Authorization en redirecciones a otro origen (C1)

### Errores

- **aserto-debil** — search_sends_expected_query solo exige 2 de los 6 expand[] de CA1 y no comprueba que haya una unica peticion (minor) (C1)
- **fixture-distinto-del-plan** — el token de los tests tiene hf_ + 36 caracteres y el plan dice 34 (minor) (C1)

## T7

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a2a745e66429288da | verde | APPROVED |

### Resúmenes

- C1 · developer · T7 hecha: HfClient::model_info/tree paginado (MAX_TREE_PAGES=20) y HfService con model_detail/search/search_next con encaje e insignias.
- C1 · reviewer · T7 aprobada; gate verde, CA4/5/6/12/16/17 de T7 cubiertos; 3 hallazgos minor sin issue.

### Decisiones

- **limitacion-documentada** — Se aceptan HfService::new(client) provisional, Url serializada con serialize_with (la cubrirá json_search_page_para_la_ui) e info antes que árbol en secuencia. (C1)

### Errores

- **aserto-debil** — search_next_computes_hints no exige cursor=abc en el mock; pasaría aunque search_next ignorase la query (minor). (C1)
- **fixture-distinto-del-plan** — model_detail_incomplete_variant_no_fit conserva IQ2_XXS y Q4_K_XL en vez de solo BF16 parte 1 + Q4_0; mismo resultado esperado (minor). (C1)

## T8

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a09f86ebec7d3c37b | verde | APPROVED |

### Resúmenes

- C1 · developer · SecretStore, KeyringStore (keyring 3), MemoryStore y FailingStore de test en hf/token.rs
- C1 · reviewer · SecretStore, KeyringStore (keyring 3.6.3 nativo Win/macOS), MemoryStore y FailingStore correctos; gate verde; solo minors

### Decisiones

- **backend-externo-opcional** — keyring 3.6.3 con windows-native y apple-native aceptado: el plan delega la versión y el Display de keyring::Error no incluye el secreto (C1)

### Errores

- **aserto-debil** — memory_store_with_token_starts_filled solo comprueba is_some y no el valor del token (C1)

## T9

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a4215f00957012a1c | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a5ff838b716d1be7f | verde | APPROVED |

### Resúmenes

- C1 · developer · T9 servicio del token (whoami, save/delete/status) con HfService::new(http, base, store) y cliente tras RwLock.
- C1 · reviewer · T9 correcto en lógica y seguridad del token; status_network_error usa un MockServer del pool que sigue escuchando tras drop
- C2 · developer · T9 C2: status_network_error usa puerto cerrado real y se añade whoami_network_error en el cliente (#40)
- C2 · reviewer · Ciclo 2 de T9: #40 resuelto con puerto efímero liberado, test whoami_network_error fija Err(Network); gate verde, sin regresiones; #40 cerrada

### Decisiones

- **aserto-por-identidad** — el test de cliente fija la variante Network; el de servicio solo verifica la agrupación de errores no-401 (C2)
- **limitacion-documentada** — token_status propaga el error del almacén (Secret) en vez de fingir ausencia; solo el constructor degrada a sin token. (C1)
- **refactor-equivalente** — endpoint sin query vacía y get_with(token) aceptados: todas las URLs previas llevan query y el token solo va a destinos de confianza (C1)

### Errores

- **aserto-debil** — status_network_error pasaba por 404 del pool de wiremock, no por error de red (#40) (C1, C2 · #40)
- **codigo-muerto** — minor: el helper closed_port_base de client.rs se duplica en línea en status_network_error de service.rs; carrera residual teórica si otro listener toma el puerto liberado (C2)

## T10

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a6c663540280b3ddd | verde | APPROVED |

### Resúmenes

- C1 · developer · T10 comandos Tauri del explorador HF, ErrorPayload movido a error.rs, hardware en RuntimeAppState
- C1 · reviewer · T10 comandos HF revisados; gate verde, CA cubiertos, ErrorPayload con JSON intacto, cliente HTTP y registro conformes al plan; solo minors

### Decisiones

- **limitacion-documentada** — production_state devuelve Box<dyn Error> por Url::parse falible; aceptado, setup lo propaga igual (C1)
- **refactor-equivalente** — ErrorPayload movido a commands/error.rs sin cambiar forma ni JSON; tests de runtime intactos (C1)

### Errores

- **aserto-debil** — json_search_item_without_hint compara solo gguf y hint en vez del JSON exacto que pide el plan (minor) (C1)

## T11

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a42cbaa51489fb9d3 | verde | APPROVED |

### Resúmenes

- C1 · developer · T11 cliente tipado HF, utilidades de formato es-ES y errors.ts movido desde runtime
- C1 · reviewer · T11 aprobada; tipos HF coinciden campo a campo con los json_* de T10 y el refactor de errors.ts es equivalente

### Decisiones

- **refactor-equivalente** — errorMessage y ErrorPayload movidos a lib/errors.ts sin cambio de forma ni de comportamiento (C1)

### Errores

- **condicion-sin-test** — formatCount muestra "1000 mil" entre 999950 y 999999 al redondear sin pasar a millones (minor) (C1)

## T15

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a3ff501bb439ff293 | verde | APPROVED |

### Resúmenes

- C1 · developer · T15 RAM total en runtime con sysinfo 0.40 (feature system), ram_bytes al final del JSON y espejo TS.
- C1 · reviewer · RAM total vía sysinfo 0.40.4 (feature system) verificada contra el fuente del crate; cambio aditivo, tests del 004 en verde, gate verde

### Decisiones

- **limitacion-documentada** — refresh_memory refresca también swap; se acepta como minor (podría usarse MemoryRefreshKind::nothing().with_ram()) (C1)

### Errores

- (ninguno)

## T16

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | ab5e9a28923314ddb | verde | APPROVED |

### Resúmenes

- C1 · developer · T16 fit.rs (Fit, MemoryBudget, fit, recommended) con 26 tests de la tabla; T15 refresh_memory_specifics solo RAM.
- C1 · reviewer · fit.rs conforme a la regla de encaje y a la de recomendada; 26 tests de la tabla con cifras de CA15 verificadas; arreglo de T15 (solo RAM) correcto

### Decisiones

- **aritmetica-u128-sin-desborde** — 80 % aplicado en u128 sobre VRAM en bytes y saturación final a u64, verificado con budget_saturates (C1)

### Errores

- (ninguno)

## T17

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a16cf4f93b41a009c | verde | APPROVED |

### Resúmenes

- C1 · developer · T17 hint.rs (typical_quant_bytes, fit_hint) y budget_cache.rs (BudgetCache) con los 15 tests de la tabla
- C1 · reviewer · T17 revisada; cifras de CA17/CA15 recalculadas a mano y coinciden, gate verde, sin hallazgos blocker/major

### Decisiones

- **aritmetica-u128-sin-desborde** — typical_quant_bytes en u128; como 606 < 1000 el resultado siempre cabe en u64 y la saturacion es defensiva (C1)
- **refactor-equivalente** — BudgetCache con OnceCell::get_or_try_init: el Err deja la celda vacia y el test concurrente es determinista en el runtime current_thread (C1)

### Errores

- (ninguno)
