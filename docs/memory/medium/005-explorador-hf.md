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
