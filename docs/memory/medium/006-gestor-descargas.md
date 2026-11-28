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

## T3

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | ab576a4752fcba3ed | verde | APPROVED |

### Resúmenes

- C1 · developer · T3 reloj inyectable, ProgressTracker (ventana 5 s, ETA), Throttle 250 ms y ProgressReporter con tests en verde
- C1 · reviewer · T3 reloj, ProgressTracker, Throttle y ProgressReporter correctos; 12 tests del plan cubiertos, gate verde, solo minors

### Decisiones

- **aritmetica-u128-sin-desborde** — velocidad en u128 por diferencia de muestras de la ventana; already_done no puede inflarla por construcción (C1)
- **refactor-equivalente** — Throttle::force(now) -> bool aceptado: reinicia el intervalo y T5/T11/T12 solo usan el reporter (C1)

### Errores

- **aserto-debil** — un test con MIB/10 no era entero y falló; se cambió a bloques de 1 MiB cada 1 s (C1)
- **codigo-muerto** — minor: force siempre devuelve true y finish ignora el valor; el plan sigue diciendo force() sin argumentos (C1)

## T4

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a2e74f81684aa8fcd | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a917081e17ca30486 | verde | APPROVED |

### Resúmenes

- C1 · developer · T4 http.rs: range_header, parse_content_range estricto y classify de respuestas, 24 tests, gate verde
- C1 · reviewer · T4 http.rs revisado; gate verde, parser estricto sin panic y tabla de tests completa; 416 con offset 0 puede reiniciar sin fin
- C2 · developer · T4 C2: 416 con offset 0 ya no reinicia, falla con UnexpectedContentRange (#67)
- C2 · reviewer · ciclo 2 de T4; #67 resuelto en código (416 con offset 0 falla) y en plan (todo Restart cuenta como intento sin progreso, marca máxima); issue cerrada; gate verde

### Decisiones

- **analizador-null-ante-duda** — 206 con total * o cabecera inválida da UnexpectedContentRange; aceptado porque T6 reinicia desde 0 una vez (C1)
- **condicion-sin-test** — 416 con offset==expected==0 sigue siendo AlreadyComplete (fichero vacio); el brazo va antes del de offset 0 (C2)
- **limitacion-documentada** — el 403 de la CDN de GitHub sin reintento en T18 es aceptable: conserva el contrato de 004 (SimpleHttpFetcher ya devolvía HttpStatus(403) sin reintentar) (C1, C2)

### Errores

- **condicion-sin-test** — 416 con offset 0 daba RestartFromZero en bucle infinito; corregido con test classify_416_offset_cero_no_reinicia (#67) (C1, C2 · #67)
- **fixture-distinto-del-plan** — tabla de tests de T5 en el plan: las filas sin_origen_hf_403_es_gated y crea_directorio_padre quedaron fusionadas en una sola línea (minor) (C2)
- **reinicio-sin-limite** — 416 con offset 0 da RestartFromZero y el plan no limita Restart en T6: posible bucle sin backoff #67 (C1 · #67)

## T5

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a5164ccd0e4606a81 | verde | APPROVED |

### Resúmenes

- C1 · developer · T5 transfer_once con .part, Range, cancelacion por select! y conversion unica de errores reqwest sin URL; 16 tests en verde y gate verde
- C1 · reviewer · T5 transfer_once revisada; gate verde, 14 tests de la tabla presentes y sin blocker ni major; 4 minor para el orchestrator

### Decisiones

- **aserto-por-identidad** — network_failure descarta el reqwest::Error y devuelve Failed(Network): FailureKind::Network no lleva texto, asi no puede filtrar la URL firmada (C1)
- **limitacion-documentada** — UnexpectedContentRange conserva el .part y T6 llama a reset_part; T6 debe no reiniciar si len(.part)==0 (416 con offset 0) (C1)
- **refactor-equivalente** — Restart en un solo intento y Retry plegado en Interrupted son coherentes con T5.3 y T6.2: T6 mide progreso con len(.part) (C1)

### Errores

- **aserto-debil** — error_reqwest_no_filtra_url_firmada no comprueba que el error crudo contenga la URL antes de convertirlo (minor) (C1)
- **condicion-sin-test** — rama is_builder de transport_error sin test (minor) (C1)
