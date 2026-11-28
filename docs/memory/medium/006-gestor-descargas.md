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

## T6

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a90a7a56ed834af7e | verde | APPROVED |

### Resúmenes

- C1 · developer · T6 reintentos con backoff inyectado (Sleeper, RetryPolicy, transfer_with_retry) y cutting_server de test
- C1 · reviewer · T6 reintentos con backoff inyectado correcto; trazas a mano de los tests coinciden con el plan; solo hallazgos minor

### Decisiones

- **limitacion-documentada** — Releer el token por intento y reajustar el progreso tras un Restart interno no tienen tarea duena; se asignan a T9/T12 via orchestrator (C1)
- **refactor-equivalente** — Politica sin progreso verificada: Restart siempre suma, marca maxima inicial = len(.part), sin espera tras progreso, acotado por expected_size (C1)

### Errores

- **condicion-sin-test** — Ningun test comprueba que el reinicio unico por UnexpectedContentRange espera su backoff (sleep [1]) (C1)
- **reserva-sin-test** — La duracion de 10 s viene del connect a puerto cerrado en Windows (~2 s por intento), no de sleeps reales (C1)

## T7

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a9433f397d8e1f321 | verde | APPROVED |

### Resúmenes

- C1 · developer · T7 verify.rs: sha256_file, verify_file y finalize_unit con tests en rojo y luego en verde; gate verde.
- C1 · reviewer · T7 verify.rs (sha256_file, verify_file, finalize_unit) cumple CA8 y los 11 tests del plan mas 5 extra; gate verde; solo hallazgos minor

### Decisiones

- **hash-en-spawn-blocking** — on_bytes es FnMut+Send+'static para moverlo a spawn_blocking; la cancelacion se comprueba entre bloques de 1 MiB. (C1)
- **limitacion-documentada** — la carrera comprobacion-rename solo es explotable por un proceso externo; AlreadyQueued cubre la interna y hard_link no es portable a FAT/exFAT (C1)
- **renombrado-final-atomico** — finalize_unit comprueba todos los destinos antes de renombrar (en Windows rename sobrescribe) y revierte los ya renombrados si uno falla. (C1)
- **validacion-hex-estricta** — InvalidRequest en verify_file es defensa en profundidad; T14.3 ya rechaza el sha laxo en la entrada (C1)

### Errores

- **comentario-inexacto** — verify.rs:107 atribuye solo a Windows la sobrescritura de rename, pero POSIX rename tambien sobrescribe (C1)
- **pending-ignorado** — finalize_unit ignora en silencio los Pending; un fallo del runner dejaria a la vista un conjunto incompleto (C1)
- **recuperacion-renombrado-parcial** — si el proceso muere a mitad de finalize_unit, al reintentar da DestinationExists y no hay recuperacion prevista en T12/T13 (C1)

## T8

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | acc7607bd94e228aa | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 0 | a3359950fc086d535 | verde | CHANGES_REQUESTED |
| 3 | 2026-11-28 | 0 | a5e5338118e220a4e | verde | APPROVED |

### Resúmenes

- C1 · developer · T8 comprobación de espacio en disco (DiskSpace, Fs4DiskSpace, required_bytes, check_space) con quality gate verde
- C1 · reviewer · código de T8 correcto y gate verde; dos major en el plan (T13.5 y candidatas a Preexisting en el cálculo de espacio)
- C2 · reviewer · plan corrige #72 y #73 (cerradas); la defensa de borrado de #72 es demasiado amplia y rompe CA5 (#75)
- C3 · reviewer · plan corrige #75 (cerrada) con protected_keys coherente en §8, T13.2, T13.5, CA5 y Riesgos; código de T8 válido y gate verde

### Decisiones

- **aritmetica-u128-sin-desborde** — check_space y required_bytes saturan; needed incluye el margen y los límites justos están cubiertos (C1)
- **limitacion-documentada** — excepción de CA5 por caída a mitad de finalize_unit aceptada: acotada a una Completed previa con mismo destino y tamaño y cubierta por cancelar_no_borra_ficheros_de_otra_unidad (C1, C2, C3)

### Errores

- **condicion-sin-test** — la defensa de borrado cuenta unidades Cancelled/Completed y Cancelar deja el .part si queda otra Cancelled del mismo destino #75 (C1, C2 · #73, #75)
- **dato-inventado** — el riesgo de T13.5 da por hecho que AlreadyQueued protege destinos de unidades Failed; Quitar puede borrar el Preexisting de otra unidad #72 (C1 · #72)
- **orden-sin-especificar** — T13.1 no dice qué hace cancel si la tarea activa termina en Completed mientras espera (cancelar en Verifying justo antes de finalize_unit) (minor) (C2, C3)
- **reserva-sin-test** — rama unwrap_or(path) de Fs4DiskSpace sin ancestro existente sin test; TDD sin rojo confirmado (C1)

## T9

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a0ac52949ee618296 | verde | APPROVED |

### Resúmenes

- C1 · developer · T9 token de HF: TokenProvider, auth_header_for/auth_source por origen, transfer_with_retry con closure de auth (T9.4), HfService como provider en memoria
- C1 · reviewer · T9 token de HF aprobado: origen estricto via is_trusted_target, closure de auth por intento, token en memoria sin llavero; gate verde

### Decisiones

- **limitacion-documentada** — HfService::token usa el token en memoria del HfClient y nunca el llavero (riesgo #45) (C1)
- **origen-estricto-link** — auth_header_for usa la URL de la peticion; reqwest 0.13.5 (redirect.rs:241) quita Authorization si cambia host, puerto o esquema (C1)

### Errores

- **aserto-debil** — redireccion_no_filtra_token solo cubre cambio de puerto (127.0.0.1 a 127.0.0.1); falta el cambio de host, caso real de la CDN (minor) (C1)

## T10

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a7bd3038ddaf760ac | verde | APPROVED |

### Resúmenes

- C1 · developer · T10 persistencia de la cola (JsonQueueStore, QueueFile, QueuePersistence, RecordingStore) con tests en verde
- C1 · reviewer · T10 persistencia de la cola conforme al plan y al patron de runtime/state.rs; gate verde; solo minors

### Decisiones

- **limitacion-documentada** — sin sync_all, .bak pisado y copia fallida se aceptan como en 004; el plan no exige durabilidad ante corte de luz (C1)
- **limpieza-best-effort** — la copia a .bak de un fichero ilegible ignora su error, como en 004 (C1)
- **origen-estricto-link** — la url persistida no abre fuga del token: auth_header_for solo firma URLs con el origen de HF y sin credenciales (C1)
- **renombrado-final-atomico** — save escribe .tmp y rename, igual que runtime/state.rs; rename reemplaza en Windows (C1)

### Errores

- **aserto-debil** — no_persiste_token_ni_url_cdn es tautologico: el token nunca entra en la estructura guardada (C1)

## T11

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a88bceceb61eb20fc | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | ae4db4364a0355367 | verde | APPROVED |

### Resúmenes

- C1 · developer · T11 alta de unidades (enqueue/list), EventSink, UnitRequest, ManagerDeps, resolve_url y dest_key; sin planificador (T12).
- C1 · reviewer · T11 completa y con sus tests del plan, gate verde; dest_key con to_lowercase no detecta en NTFS destinos iguales que difieren en la sigma final (#79)
- C2 · developer · T11 C2: dest_key normaliza en mayúsculas carácter a carácter y el test de reversión comprueba id y version
- C2 · reviewer · ciclo 2 de T11; #79 resuelto con char::to_uppercase por componente, tests de la sigma y aserto de reversión corregido, gate verde; issue cerrada

### Decisiones

- **limitacion-documentada** — NFC/NFD en macOS queda en Riesgos sin normalizar; no se repitió la prueba en NTFS porque el entorno lo denegó, la verificación es analítica (C1, C2)
- **refactor-equivalente** — to_uppercase sin reglas de contexto iguala Σ/σ como NTFS; los grupos de más (ς, ſ, ß→SS) solo dan rechazos innecesarios, que son seguros (C1, C2)

### Errores

- **aserto-debil** — enqueue_revierte_si_falla_el_guardado no comprueba que se reviertan next_id ni version (C1)
- **condicion-sin-test** — sigma final: Σ y σ daban claves distintas en dest_key (#79) (C1, C2 · #79)
- **dato-inventado** — minor: la fila de Riesgos pone ſ/s como par que dest_key no iguala, pero to_uppercase('ſ') da 'S' y sí lo iguala (C2)
- **fixture-distinto-del-plan** — minor: dest_key_sigma_final no incluye qς ni la clave literal "QΣ/M.GGUF", y dest_key_normaliza no comprueba "A/B.GGUF", como pide la tabla del plan (C2)
- **normalizacion-unicode** — dest_key usa str::to_lowercase como pide el plan, pero la regla de la sigma final da claves distintas para QΣ y qσ, que en NTFS son el mismo fichero (#79) (C1 · #79)

## T12

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a1b1cb63c60f6a537 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a70cd6dab28db08c7 | verde | APPROVED |

### Resúmenes

- C1 · developer · T12 planificador FIFO, runner de unidades, rewind de progreso, unit_failure y finalize que rechaza Pending
- C1 · reviewer · T12 correcto en orden, FIFO, cancelación por token, unit_failure y borrado del .part; el progreso de la fase Verify usa un reporter aparte y salta (#82)
- C2 · developer · T12 C2: un solo ProgressReporter por unidad con total constante y un unico limitador para Download y Verify (#82)
- C2 · reviewer · T12 C2 aprobada; #82 cerrada con un reporter y un limitador por unidad y total constante; el salto entre fases exige cambiar el plan antes de T16/T17

### Decisiones

- **aritmetica-u128-sin-desborde** — la base del rebobinado cuenta el inicial de las otras Pending, coherente con el already_done del reporter; downloaded_bytes nunca supera total_bytes (C1)
- **limitacion-documentada** — contrato de progreso por fase: total constante, cada phase monótona, Download y Verify se intercalan en Downloading; la UI (T16.3/T17.2) debe guardar y pintar el progreso por fase (C1, C2)
- **refactor-equivalente** — record_at y finish_at fijan el absoluto con el mismo Throttle; Mutex de std sin await dentro, emisión fuera del cerrojo (C1, C2)

### Errores

- **condicion-sin-test** — minor: progreso_un_solo_limitador detecta un limitador doble solo si el primer Verify cae a menos de 250 ms del último Download (depende de los trozos de red) (C1, C2 · #82)
- **dato-inventado** — minor: la ventana de velocidad mezcla muestras de hash y de red; tras verificar una parte grande, bytes_per_sec y eta de Download son irreales unos 5 s (C2)

## T13

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | aed712c38cccea952 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | aec599d6a6ee0ab72 | verde | CHANGES_REQUESTED |
| 3 | 2026-11-28 | 1 | a0fa8e07bb0027f60 | verde | APPROVED |
| 4 | 2026-11-28 | 1 | a5da4fa985061042a | verde | CHANGES_REQUESTED |
| 5 | 2026-11-28 | 1 | a85eae14b2abc9076 | verde | APPROVED |

### Resúmenes

- C1 · developer · T13 hecha: pause/resume/cancel/retry/remove con JoinHandle en active, borrado protegido (#72/#75), #76 revalidado, finalize idempotente y paso (0) del runner.
- C1 · reviewer · T13 casi correcta (borrado protegido, #76 resuelta y cerrada, T13.4 bien), pero pausar o cancelar una Queued sin ejecución tiene una carrera con finish_run/start_waiting (#85)
- C2 · developer · T13 ciclo 2: reserva atómica de la unidad bajo el cerrojo de la cola en stop_run; start_waiting no relanza unidades reservadas (#85)
- C2 · reviewer · la reserva de #85 es correcta (sin atascos, interbloqueos ni envenenamiento; los tests de download:: son estables en 8 ejecuciones), pero ningún test fija que pause/cancel conserven el guard: la mutación let _ = sobrevive
- C3 · developer · T13 C3: tests de regresión que sondean la reserva de B al emitir Paused/Cancelled; renombrados los dos tests de stop_run.
- C3 · reviewer · T13 C3 aprobada; la sonda en transition_locked fija que pause/cancel conservan la reserva, la mutación let _ = falla 2 de 258, #85 cerrada, gate verde
- C4 · developer · T13.6 remove sobre Cancelled vuelve a llamar a delete_unit_files con protected_keys antes de sacar la unidad de la lista
- C4 · reviewer · T13.6 remove reintenta el borrado en Cancelled; gate verde, pero borra finales ajenos por FileSpec Verified obsoletos
- C5 · developer · T13.6 ciclo 2: remove en Cancelled solo borra .part (Scope::PartsOnly), nunca finales; tests de final posterior y de fallo de borrado.
- C5 · reviewer · T13.6 ciclo 2: remove en Cancelled solo borra .part (Scope::PartsOnly); #87 resuelta y cerrada; gate verde

### Decisiones

- **aserto-por-identidad** — el test de fallo de borrado discrimina drop_unit antes del borrado: exige Err y la unidad en list() (C1, C2, C3, C5)
- **limitacion-documentada** — remove sin evento y fallo de borrado en cancel con la unidad ya Cancelled: el plan no lo define, se pasa al planner como minor (C1)
- **limpieza-best-effort** — delete_unit_files recibe Scope (Everything/PartsOnly); cancel y remove en Failed usan Everything. (C2, C4, C5)
- **origen-estricto-link** — remove en Cancelled nunca borra finales porque el estado Verified no prueba la autoría del final (#87) (C5)
- **refactor-equivalente** — remove borra para Failed y Cancelled con el mismo camino; una Completed no borra nada (C1, C2, C4)

### Errores

- **aserto-debil** — quitar_cancelada_conserva_unidad_si_falla_borrado no comprueba que el directorio siga en disco, como pide la tabla de tests (minor) (C5)
- **condicion-sin-test** — stop_run no reserva una Queued sin ejecución: finish_run->start_waiting la relanza con un token vivo durante pause/cancel #85 (C1 · #85)
- **dato-inventado** — remove en Cancelled confía en estados Verified que cancel no reinicia y borra el modelo de una descarga posterior ya quitada de la lista (#87) (C4 · #87)
- **fixture-distinto-del-plan** — pausa_conserva_part y los cancel "desde on_bytes" usan stalling_server; con el servidor parado la aserción del .part queda vacía (C1)
- **reserva-sin-test** — #87 remove en Cancelled borraba el final de una descarga posterior de la misma variante; corregido con PartsOnly. (C2, C3, C5 · #85, #87)

## T14

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a53cc09b86ba8846d | verde | APPROVED |

### Resúmenes

- C1 · developer · T14 revisión fijada (sha del commit), tree por revisión y UnitRequest::try_from validado.
- C1 · reviewer · T14 revisión fijada al commit y TryFrom DownloadRequest->UnitRequest; gate verde, 8 de 10 mutaciones detectadas

### Decisiones

- **aserto-por-identidad** — model_detail_usa_sha_del_commit compara la lista exacta de rutas /tree/ pedidas, así detecta peticiones a /tree/main (C1)
- **limitacion-documentada** — VariantRow.svelte sigue enviando revision "main"; usar detail.revision queda fuera de T14. (C1)
- **validacion-hex-estricta** — sha256 con hex::decode_to_slice a 32 bytes y revisión "main" o 40 hex minúsculas, como pide el plan (C1)

### Errores

- **aserto-debil** — unit_request_sha_laxo solo usa longitudes impares (63, 65) y revision_invalida no tiene 40 hex en mayúsculas: usar hex::decode sin longitud o aceptar A-F sobreviven (minor) (C1)

## T15

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a87797106a3cf894e | verde | APPROVED |

### Resúmenes

- C1 · developer · T15 puente Tauri de descargas: TauriEventSink, DownloadsAppState, 7 comandos, From<DownloadError> para ErrorPayload y 6 tests.
- C1 · reviewer · puente Tauri de descargas correcto: cliente propio 30/60 s sin timeout total, app_local_data_dir, TryFrom de T14, codigos por variante y mensajes Io fijos; gate verde

### Decisiones

- **aserto-por-identidad** — match exhaustivo sin comodin en download_error_code y failure_code: una variante nueva obliga a darle codigo (C1)
- **limitacion-documentada** — un fallo al crear el estado en setup aborta el arranque con error (sin unwrap), igual que runtime y hf; se acepta (C1)
- **limpieza-best-effort** — emit del sink ignora el error (informativo); mensajes de Io fijos para no filtrar datos ni token. (C1)

### Errores

- (ninguno)

## T16

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a36bcc9099a5c8461 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a5f01527a6a721861 | verde | APPROVED |

### Resúmenes

- C1 · developer · T16 UI de descargas: types, api, format y store (funciones puras y DownloadsStore) con tests, gate en verde.
- C1 · reviewer · T16 ciclo 1, gate verde y tabla de tests completa, pero dos majors en DownloadsStore.start y applyState reproducidos con Vitest
- C2 · developer · T16 ciclo 2: token de generación en start/stop (#91) y progreso limpiado fuera de ejecución en applyState/mergeList (#92).
- C2 · reviewer · ciclo 2 de T16; #91 (token generation en start/stop) y #92 (replaceUnit limpia progress fuera de ejecucion) resueltos y cerrados; gate verde

### Decisiones

- **aserto-por-identidad** — las carreras de start/stop se prueban por efecto observable (un emit tras stop no cambia el store), y fallan sin el token (C1, C2)
- **limitacion-documentada** — conservar progress entre Downloading y Verifying es correcto para CA13; el fallo es arrastrarlo fuera de los estados en ejecucion (C1)
- **refactor-equivalente** — replaceUnit centraliza la regla de conservar progress solo entre Downloading y Verifying. (C2)

### Errores

- **condicion-sin-test** — la comprobacion de generation tras downloadsList (store.svelte.ts:126) no la ejercita ningun test; el mock devuelve [] y no se puede observar la fusion (minor) (C1, C2 · #91, #92)
- **estado-obsoleto** — applyState conserva phase Verify y verify tras Paused, Failed y retry, y T17.2 mostraria Verificando #92 (C1 · #92)

## T17

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | af31edac381ff466e | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a0e38ced61c75f4bd | verde | APPROVED |

### Resúmenes

- C1 · developer · T17 UI: vista Descargas con DownloadItem, seccion en navegacion, boton Descargar habilitado con revision de la ficha y alta desde Explorar
- C1 · reviewer · T17 ciclo 1, gate verde y UI correcta; falta test de que App cablea onNavigate y la seccion activa pasa a Descargas
- C2 · developer · Ciclo 2 T17: test e2e en App.test.ts que comprueba que Descargar desde la ficha navega a Descargas; rojo verificado quitando onNavigate.
- C2 · reviewer · ciclo 2 de T17; el test descargar_desde_la_ficha_navega_a_descargas cubre CA13/T17.1, la mutación sin onNavigate en App.svelte lo pone en rojo; gate verde, Vitest sin rechazos ni avisos

### Decisiones

- **aserto-por-identidad** — el test comprueba h1 «Descargas» y aria-current="page" en el botón de la sección, no solo que se invoque el comando (C1, C2)
- **csp-estricta** — los dos progress salen sin style y la barra principal solo lee progress.download (CA13 cubierto) (C1)
- **deuda-aceptada-usuario** — los minor bytesOnDisk con !== "Pending" y actionError sin limpiar van a una issue del orchestrator, no bloquean (C2)
- **refactor-equivalente** — el cast Component<ViewProps> en App es seguro en runtime: las vistas sin props ignoran onNavigate (C1)

### Errores

- **aserto-debil** — El test de Explore solo comprobaba el callback, no que App cambiara de seccion; cubierto con test de extremo a extremo. (C1, C2)

## T18

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-28 | 1 | a489f9fb62d62a4c7 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-28 | 1 | a4c6a2ec9b108cc27 | verde | APPROVED |

### Resúmenes

- C1 · developer · T18 hecha: download_one, CancelToken sobre CancellationToken, ResumableHttpFetcher (+RuntimeError::Network) y SimpleHttpFetcher eliminado.
- C1 · reviewer · T18 bien migrado y gate verde, pero fetch_tamano_distinto cambia su aserción a Network (incumple CA14) por un hueco de transfer.rs que también rompe CA8
- C2 · developer · T18.5 implementada: tamaño anunciado (200 Content-Length y 416 */N) falla con SizeMismatch sin reintento; gate verde
- C2 · reviewer · T18.5 resuelve #95; 200 con Content-Length distinto y 416 */N distinto fallan en SizeMismatch sin reintento y borran el .part; mutaciones detectadas por tests de integración; gate verde

### Decisiones

- **aserto-por-identidad** — fetch_tamano_distinto recupera SizeMismatch exacto con 1 petición y sin esperas (#95) (C2)
- **condicion-sin-test** — verificadas con mutaciones las dos comprobaciones nuevas de classify: fallan 5 y 4 tests, incluidos retry, transfer, runner y fetch (C2)
- **limitacion-documentada** — la regla de Content-Length depende de que reqwest no tenga la descompresión activada; documentado en http.rs y en Riesgos, sin async-compression en Cargo.lock (C1, C2)
- **limpieza-best-effort** — el fetcher borra el .part ante cualquier error; los mensajes de Io son fijos y no filtran rutas ni URLs (C1)
- **refactor-equivalente** — ramas 416 de classify extraídas a classify_unsatisfied con el mismo orden de reglas (C1, C2)

### Errores

- **aserto-debil** — fetch_tamano_distinto pasa de SizeMismatch a Network: un 200 con Content-Length menor que expected_size se reintenta 5 veces en vez de fallar (#95) (C1 · #95)
- **fixture-distinto-del-plan** — fetch_tamano_distinto no puede dar SizeMismatch con la descarga reanudable: un cuerpo corto 200 se reintenta y acaba en Network. (C1)
