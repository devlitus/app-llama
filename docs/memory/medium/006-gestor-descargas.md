---
schema: memoria-plan/2
plan: 006-gestor-descargas
estado: cerrado
---

# Memoria del plan 006-gestor-descargas

## Patrones del plan

### Decisiones

- **analizador-null-ante-duda** — 206 con total * o cabecera inválida da UnexpectedContentRange; aceptado porque T6 reinicia desde 0 una vez (T4)
- **aritmetica-u128-sin-desborde** — la base del rebobinado cuenta el inicial de las otras Pending, coherente con el already_done del reporter; downloaded_bytes nunca supera total_bytes (T3, T8, T12)
- **aserto-por-identidad** — fetch_tamano_distinto recupera SizeMismatch exacto con 1 petición y sin esperas (#95) (T1, T5, T13, T14, T15, T16, T17, T18)
- **condicion-sin-test** — verificadas con mutaciones las dos comprobaciones nuevas de classify: fallan 5 y 4 tests, incluidos retry, transfer, runner y fetch (T4, T18)
- **csp-estricta** — los dos progress salen sin style y la barra principal solo lee progress.download (CA13 cubierto) (T17)
- **deuda-aceptada-usuario** — los minor bytesOnDisk con !== "Pending" y actionError sin limpiar van a una issue del orchestrator, no bloquean (T17)
- **formato-version-dual** — fs4 1.1.0 con features por defecto; available_space se verá en T8 (T1)
- **hash-en-spawn-blocking** — on_bytes es FnMut+Send+'static para moverlo a spawn_blocking; la cancelacion se comprueba entre bloques de 1 MiB. (T7)
- **limitacion-documentada** — la verificación manual no registró hash, modelo dividido ni repo gated; la anotación lo declara explícitamente (T1, T2, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T18, T19)
- **limpieza-best-effort** — el fetcher borra el .part ante cualquier error; los mensajes de Io son fijos y no filtran rutas ni URLs (T10, T13, T15, T18)
- **origen-estricto-link** — remove en Cancelled nunca borra finales porque el estado Verified no prueba la autoría del final (#87) (T9, T10, T13)
- **refactor-equivalente** — ramas 416 de classify extraídas a classify_unsatisfied con el mismo orden de reglas (T2, T3, T5, T6, T11, T12, T13, T16, T17, T18)
- **renombrado-final-atomico** — save escribe .tmp y rename, igual que runtime/state.rs; rename reemplaza en Windows (T7, T10)
- **validacion-hex-estricta** — sha256 con hex::decode_to_slice a 32 bytes y revisión "main" o 40 hex minúsculas, como pide el plan (T7, T14)

### Errores

- **aserto-debil** — fetch_tamano_distinto pasa de SizeMismatch a Network: un 200 con Content-Length menor que expected_size se reintenta 5 veces en vez de fallar (#95) (T2, T3, T5, T9, T10, T11, T13, T14, T17, T18 · #63, #95)
- **codigo-muerto** — minor: force siempre devuelve true y finish ignora el valor; el plan sigue diciendo force() sin argumentos (T2, T3)
- **comentario-inexacto** — verify.rs:107 atribuye solo a Windows la sobrescritura de rename, pero POSIX rename tambien sobrescribe (T7)
- **condicion-sin-test** — la comprobacion de generation tras downloadsList (store.svelte.ts:126) no la ejercita ningun test; el mock devuelve [] y no se puede observar la fusion (minor) (T1, T2, T4, T5, T6, T8, T11, T12, T13, T16 · #67, #73, #75, #79, #82, #85, #91, #92)
- **criterio-divergente** — split.rs no filtra con is_valid_split: rechaza variantes únicas que la ficha ofrece (x-00000-of-00002) y acepta un split sin prefijo; #63 (T2 · #63)
- **dato-inventado** — remove en Cancelled confía en estados Verified que cancel no reinicia y borra el modelo de una descarga posterior ya quitada de la lista (#87) (T1, T8, T11, T12, T13 · #72, #87)
- **estado-obsoleto** — applyState conserva phase Verify y verify tras Paused, Failed y retry, y T17.2 mostraria Verificando #92 (T16 · #92)
- **fixture-distinto-del-plan** — fetch_tamano_distinto no puede dar SizeMismatch con la descarga reanudable: un cuerpo corto 200 se reintenta y acaba en Network. (T4, T11, T13, T18)
- **normalizacion-unicode** — dest_key usa str::to_lowercase como pide el plan, pero la regla de la sigma final da claves distintas para QΣ y qσ, que en NTFS son el mismo fichero (#79) (T11 · #79)
- **orden-sin-especificar** — T13.1 no dice qué hace cancel si la tarea activa termina en Completed mientras espera (cancelar en Verifying justo antes de finalize_unit) (minor) (T8)
- **pending-ignorado** — finalize_unit ignora en silencio los Pending; un fallo del runner dejaria a la vista un conjunto incompleto (T7)
- **recuperacion-renombrado-parcial** — si el proceso muere a mitad de finalize_unit, al reintentar da DestinationExists y no hay recuperacion prevista en T12/T13 (T7)
- **reinicio-sin-limite** — 416 con offset 0 da RestartFromZero y el plan no limita Restart en T6: posible bucle sin backoff #67 (T4 · #67)
- **reserva-sin-test** — #87 remove en Cancelled borraba el final de una descarga posterior de la misma variante; corregido con PartsOnly. (T6, T8, T13 · #85, #87)

## T1

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | aefb76af142cf9e89 | verde | APPROVED |

## T2

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a058a6188eab80070 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a5ba854d18f43928e | verde | APPROVED |

## T3

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | ab576a4752fcba3ed | verde | APPROVED |

## T4

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a2e74f81684aa8fcd | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a917081e17ca30486 | verde | APPROVED |

## T5

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a5164ccd0e4606a81 | verde | APPROVED |

## T6

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a90a7a56ed834af7e | verde | APPROVED |

## T7

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a9433f397d8e1f321 | verde | APPROVED |

## T8

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | acc7607bd94e228aa | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 0 | a3359950fc086d535 | verde | CHANGES_REQUESTED |
| 3 | 2026-11-28 | 0 | a5e5338118e220a4e | verde | APPROVED |

## T9

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a0ac52949ee618296 | verde | APPROVED |

## T10

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a7bd3038ddaf760ac | verde | APPROVED |

## T11

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a88bceceb61eb20fc | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | ae4db4364a0355367 | verde | APPROVED |

## T12

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a1b1cb63c60f6a537 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a70cd6dab28db08c7 | verde | APPROVED |

## T13

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | aed712c38cccea952 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | aec599d6a6ee0ab72 | verde | CHANGES_REQUESTED |
| 3 | 2026-11-28 | 1 | a0fa8e07bb0027f60 | verde | APPROVED |
| 4 | 2026-11-28 | 1 | a5da4fa985061042a | verde | CHANGES_REQUESTED |
| 5 | 2026-11-28 | 1 | a85eae14b2abc9076 | verde | APPROVED |

## T14

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a53cc09b86ba8846d | verde | APPROVED |

## T15

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a87797106a3cf894e | verde | APPROVED |

## T16

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a36bcc9099a5c8461 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a5f01527a6a721861 | verde | APPROVED |

## T17

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | af31edac381ff466e | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a0e38ced61c75f4bd | verde | APPROVED |

## T18

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a489f9fb62d62a4c7 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a4c6a2ec9b108cc27 | verde | APPROVED |

## T19

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 0 | a898128accf118f3f | verde | APPROVED |
