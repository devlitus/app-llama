---
schema: memoria-plan/2
plan: 006-gestor-descargas
estado: abierto
---

# Memoria del plan 006-gestor-descargas

## T1

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | aefb76af142cf9e89 | verde | APPROVED |

### Resúmenes

- C1 · developer · T1 hecha: tipos de dominio, DownloadError, safe_join/part_path, transiciones; deps tokio-util y fs4 en el workspace
- C1 · reviewer · T1 aprobada; safe_join probado en Windows real sin escape (".. ", "...", ". " fallan con NotFound), fs4 available_space sin feature y con statvfs en macOS

### Decisiones

- **aserto-por-identidad** — safe_join rechaza cualquier componente con ':' (prefijos de unidad y flujos NTFS) además de los del plan (C1)
- **formato-version-dual** — fs4 1.1.0 con features por defecto; available_space se verá en T8 (C1)
- **limitacion-documentada** — nombres reservados de Windows y puntos/espacios finales no se rechazan en safe_join: dan Io, no escape; el plan no los pide (C1)

### Errores

- **condicion-sin-test** — can_transition tiene brazos sin test (Paused->Queued, Downloading->Queued, Verifying->Cancelled...); el plan solo pide 5 filas (C1)
- **dato-inventado** — incoherencia del plan: Diseño 7 falla por espacio "antes de pasar a Downloading" pero T1.4 no permite Queued->Failed; resolver antes de T12 (C1)

## T2

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a058a6188eab80070 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a5ba854d18f43928e | verde | APPROVED |

### Resúmenes

- C1 · developer · T2 validate_parts y display_name en download/split.rs reutilizando el parser de hf/quant.rs
- C1 · reviewer · T2 con el gate en verde y los 13 tests del plan; validate_parts y display_name no aplican is_valid_split como hf/files.rs
- C2 · developer · T2 C2, una sola definición de split válido compartida por files.rs y download/split.rs (#63)
- C2 · reviewer · T2 C2 aprobada; #63 cerrada, criterio de split valido unico en parse_valid_split_suffix y equivalencia ficha/backend verificada

### Decisiones

- **limitacion-documentada** — warning cargo::unused_dependencies de fs4 es lint de cargo, no lo eleva clippy -D warnings y la toolchain esta fijada; se resuelve en T8 (minor) (C2)
- **refactor-equivalente** — files.rs y split.rs comparten parse_valid_split_suffix; toda variante completa pasa validate_parts y display_name coincide con GgufVariant::name (C1, C2)

### Errores

- **aserto-debil** — validate_parts/display_name no aplicaban is_valid_split, desalineados con la ficha (#63) (C2 · #63)
- **codigo-muerto** — parse_split_suffix queda pub(crate) pero solo se usa dentro de quant.rs (minor) (C2)
- **condicion-sin-test** — falta un test con dos ficheros únicos distintos (IncompleteSplit) (C1)
- **criterio-divergente** — split.rs no filtra con is_valid_split: rechaza variantes únicas que la ficha ofrece (x-00000-of-00002) y acepta un split sin prefijo; #63 (C1 · #63)
