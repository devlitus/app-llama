---
schema: memoria-plan/2
plan: 005-explorador-hf
estado: cerrado
---

# Memoria del plan 005-explorador-hf

## Patrones del plan

### Decisiones

- **aritmetica-u128-sin-desborde** — typical_quant_bytes en u128; como 606 < 1000 el resultado siempre cabe en u64 y la saturacion es defensiva (T4, T16, T17)
- **aserto-por-identidad** — clave del each de variantes = parts[0].path, única porque cada fichero del árbol cae en un solo grupo y todo grupo tiene al menos una parte (T9, T13)
- **backend-externo-opcional** — keyring 3.6.3 con windows-native y apple-native aceptado: el plan delega la versión y el Display de keyring::Error no incluye el secreto (T8)
- **etiqueta-vs-quant** — UD-IQ2_XXS/UD-Q4_K_XL del plan son etiquetas; name conserva UD- y quant sigue T2; conviene aclararlo en el plan (T3, T7, CA16) (T3)
- **limitacion-documentada** — refresh_memory refresca también swap; se acepta como minor (podría usarse MemoryRefreshKind::nothing().with_ram()) (T1, T3, T5, T6, T7, T9, T10, T12, T15)
- **origen-estricto-link** — is_trusted_target compartido por parse_next_link y HfClient::get; reqwest 0.13 quita Authorization en redirecciones a otro origen (T5, T6)
- **refactor-equivalente** — BudgetCache con OnceCell::get_or_try_init: el Err deja la celda vacia y el test concurrente es determinista en el runtime current_thread (T1, T2, T3, T4, T5, T9, T10, T11, T12, T13, T14, T17)

### Errores

- **aserto-debil** — json_search_item_without_hint compara solo gguf y hint en vez del JSON exacto que pide el plan (minor) (T6, T7, T8, T9, T10 · #40)
- **carrera-estado-inicial** — minor: mientras hf_token_status no responde (o si falla) la etiqueta dice Sin token, y su respuesta tardia puede pisar el estado tras un guardado (T14)
- **carrera-paginacion-duplicados** — loadMore añadía duplicados y rompía el each con clave; corregido, ver #49 (T12 · #49)
- **clave-each-no-unica** — variant.name no es único y rompía la ficha con each_key_duplicate (#52) (T13 · #52)
- **codigo-muerto** — minor: el helper closed_port_base de client.rs se duplica en línea en status_network_error de service.rs; carrera residual teórica si otro listener toma el puerto liberado (T9)
- **condicion-sin-test** — minor: ningun test comprueba que Guardar/Borrar se reactivan tras un guardado o borrado fallido (lo garantiza el finally) (T11, T12, T13, T14)
- **dato-inventado** — minor: model_info_gated_manual.json mezcla id meta-llama con _id, cardData y licencia de unsloth/Qwen (T1)
- **fixture-distinto-del-plan** — model_detail_incomplete_variant_no_fit conserva IQ2_XXS y Q4_K_XL en vez de solo BF16 parte 1 + Q4_0; mismo resultado esperado (minor). (T2, T6, T7)
- **literal-duplicado** — minor: tree_url fija "main" a mano mientras files.rs tiene DEFAULT_REVISION (T5)
- **reserva-sin-test** — faltan quant_model_name_digits_not_matched (Qwen2.5-7B-Instruct.gguf) y quant_empty_path ("") de la tabla de T2 (T2)

## T1

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a399c0c885a43537a | verde | APPROVED |

## T2

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a9c62bec2cacf973c | verde | CHANGES_REQUESTED |
| 2 | 2026-11-27 | 1 | a7de46f3b0a472d4b | verde | APPROVED |

## T3

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | ab5c9c64fe663e5c0 | verde | APPROVED |

## T4

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | ab767607f0d9e1ffc | verde | APPROVED |

## T5

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a83574ccfb62a9a43 | verde | APPROVED |

## T6

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a6791ed68333e994f | verde | APPROVED |

## T7

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a2a745e66429288da | verde | APPROVED |

## T8

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a09f86ebec7d3c37b | verde | APPROVED |

## T9

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a4215f00957012a1c | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a5ff838b716d1be7f | verde | APPROVED |

## T10

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a6c663540280b3ddd | verde | APPROVED |

## T11

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a42cbaa51489fb9d3 | verde | APPROVED |

## T12

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | ab5e04a2e63ea6f29 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a9afa4524ca013dbd | verde | APPROVED |

## T13

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a0b05ffdab0a966fb | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a63c698cf1da1159a | verde | APPROVED |

## T14

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a910a6ab00bdf30e5 | verde | APPROVED |

## T15

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a3ff501bb439ff293 | verde | APPROVED |

## T16

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | ab5e9a28923314ddb | verde | APPROVED |

## T17

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a16cf4f93b41a009c | verde | APPROVED |
