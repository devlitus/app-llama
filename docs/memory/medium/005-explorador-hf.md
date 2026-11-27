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
